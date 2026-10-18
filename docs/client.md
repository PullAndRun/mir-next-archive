# 客户端：联机世界、面板交互与实体渲染

本文记录 `mir-client` 在「离线看地图」之外的联机能力：登录链路、世界快照、输入映射、
物品/装备交互、世界内文字、面板、小地图、音效与无窗口端到端测试。协议细节见
[`protocol/login-flow.md`](protocol/login-flow.md)，渲染几何见
`crates/mir-render/src/lib.rs` 与 `crates/mir-render/src/geometry.rs` 的模块文档。

## 1. 两种模式

| 模式 | 启动参数 | 行为 |
|---|---|---|
| 离线地图 | `--map 0 --cell 330,270` | 直接读本地 `.map`，方向键/WASD 平移相机；`S` 截图 |
| 联机世界 | `--connect 127.0.0.1:7000 --account NAME --password PASS --character NAME` | 连真实服务端，登录/建角/进入世界；相机跟随自己 |

联机示例：

```powershell
cargo run -p mir-client -- --connect 127.0.0.1:7000 `
  --account diana --password secret --character 裁决者 `
  --create-character 裁决者 --class 0 --gender male
```

* `--create-character` 只在角色不存在时使用；角色已存在会直接选它。
* 账号不存在时默认自动注册；加 `--no-register` 可关闭。
* `--class` 支持 `0..4` 或 `warrior/wizard/taoist/assassin/archer`；
  `--gender` 支持 `0/1` 或 `male/female`。
* 联机模式忽略 `--cell`：相机跟随自己的坐标；地图名来自服务端 `MapInformation.file_name`，
  再按 `--map-dir` 在本地找 `<name>.map`。本地地图缺失时退化为空白地图，仍然显示实体。
* 窗口创建失败（无显示器 CI）只打印原因并返回，不 panic；`NetClient` 的 `Drop` 会发送
  `Quit` 并回收网络线程。
* 界面默认中文；`--lang en` 切换成英文（写入 `settings.json`，`--set-language` 只改设置）。
  所有固定文案都在 `crates/mir-client/src/ui/labels.rs`。
* 音效默认开启、音量 0.5（`settings.json` 的 `sound` / `volume`）；`--no-sound` 与
  `--volume <0..1>` 只影响本次启动，`--sound-dir <目录>` 可追加音效目录。
  没有音频设备时只打印一次警告，窗口照常运行（见 §5.6）。

## 2. 登录包序（实现于 `crates/mir-client/src/net.rs`）

```
TCP 连接
  ← S.Connected
  → C.ClientVersion { version_hash: 16 字节 "mir-next-client!" }
  ← S.ClientVersion { result = 1 }
  → C.Login { account, password }
  ← S.Login { result = 3 }          // 账号不存在
  → C.NewAccount { 空资料 }          // 自动注册；--no-register 时直接报错
  ← S.NewAccount { result = 8 }
  → C.Login { account, password }
  ← S.LoginSuccess { characters }
  → C.StartGame { character_index }  // 角色不存在时先 C.NewCharacter
  ← S.NewCharacterSuccess / S.NewCharacter
  ← S.StartGame { result = 4 }
  ← S.MapInformation
  ← S.NewMapInfo                     // 服务端有数据库时
  ← S.UserInformation                // 自己的对象 ID / 坐标 / HP/MP / 背包 / 装备
```

Crystal 的版本哈希是客户端 exe 的 MD5（`Client/MirScenes/LoginScene.cs:133-152`），
`mir-server` 默认 `check_version = false`，所以这里用固定 16 字节占位值，长度与真实哈希一致。
进世界后的 `StartGame{4} → MapInformation → NewMapInfo → UserInformation` 顺序与
`PlayerObject.CheckMapInfo`（`Client/MirObjects/PlayerObject.cs:1009-1056`）和
本作 `mir-server/src/session.rs` 的发包顺序一致。

网络实现选择**阻塞 socket + 后台线程**：`TcpStream` 设 20 ms 读超时，UI 通过 channel
发命令，线程每轮先处理命令再读包；不引入额外的异步运行时（`mir-client` 的 `tokio`
依赖只用于测试里在进程内起真实服务端）。

## 3. 快照

`WorldSnapshot`（`mir-client/src/net.rs`）是网络层与渲染层之间唯一的接口：

* `self_player`：`UserInformation` 里的对象 ID/名字/坐标/朝向/HP/MP/等级/职业/金币；
* `inventory` / `equipment`：背包 46 格与 14 个装备槽（`EquipmentSlot` 顺序：武器/衣服/
  头盔/火把/项链/左手镯/右手镯/左戒指/右戒指/护身符/腰带/靴子/宝石/坐骑）；
  `S.EquipItem` / `S.RemoveItem` 用本地物品数据补全背包 ↔ 装备槽的搬运
  （Crystal `PlayerObject.EquipItem`，`PlayerObject.cs:5674-5678`）；
* `item_infos`：`S.NewItemInfo` 的物品模板，除名字/图标/价格外还有 `ItemType`、
  `RequiredType`/`RequiredAmount`、耐久上限、`ItemInfo.stats`（tooltip 的攻防属性来源）
  与 `ItemInfo.shape`（纸娃娃外观号，见第 6 节）；
* `buffs`：`S.AddBuff` / `S.RemoveBuff` 的 `ClientBuff` 子集；`ExpireTime` 是 Unix 毫秒，
  由 UI 用本地时钟倒计时（Crystal `Client/MirScenes/Dialogs/BuffDialog.cs:166-171,368-376`）；
* `damage_events`：`S.ObjectStruck`（无伤害值，画受击标记）与 `S.DamageIndicator`
  （伤害值/类型）产生的递增 ID 事件，UI 只消费新增事件，避免快照克隆造成重复飘字；
* `effects`：`S.ObjectEffect` 的短光效事件（魔法盾/元素结界等）；
* `entities`：`ObjectPlayer`、`ObjectMonster`、`ObjectNPC`、`ObjectItem`、`ObjectGold`、
  `NewMapInfo` 的 NPC 列表建立的 `BTreeMap<u32, WorldEntity>`；
* `ObjectWalk/ObjectRun/ObjectTurn/ObjectAttack/ObjectStruck/ObjectDied/ObjectRevived` 增量更新，
  并把 `action` / `action_started_ms`（`ActorAction`）写到实体上；走/跑/攻击/受击/死亡
  的逐帧选择由渲染层按真实时间完成（见 §6.3）；
* `HealthChanged` 更新自己，`ObjectHealth` 更新怪物血量百分比；
* `Chat`、`ObjectChat` 进聊天记录（最多 100 条）；`NPCResponse` 存对话页，
  `NPCGoods` 记货架；`GainedItem/GainedGold` 写一条系统消息并更新金币；
* `ObjectRemove`/`MapChanged` 清理实体；
* `self_player.looks` / `entities[].looks`（`PlayerLooks`）：纸娃娃外观（衣服/武器/发型/
  盾牌）。自己的由装备槽 + 物品模板推导（Crystal `UserObject.RefreshEquipmentStats`，
  `Client/MirObjects/UserObject.cs:205-250`），别人的直接取 `S.ObjectPlayer` /
  `S.PlayerUpdate` 的外观号；每处理完一个包重算一次，所以装备与模板谁先到都能收敛；
* `packet_counts` 记录包种类次数，供测试断言。

客户端每批包处理完把快照整体克隆给 UI；UI 侧 `poll_network` 用最新快照重绘。
`UiState::time_millis` 是可注入的客户端时钟（窗口模式每帧写真实时间），伤害飘字与
buff 倒计时都基于它，所以无头测试可以任意拨表。

动作时间由 `WorldSnapshot::apply_packet_at(packet, now_ms)` 注入（生产代码用
`apply_packet` 取当前 Unix 毫秒），所以 `tests/anim.rs` 能在不睡觉的情况下断言
「599 ms 还在走、600 ms 回 Standing」。自己的走/跑/攻击/转身没有服务器回显
（服务端只给自己发 `UserLocation`），由 `NetCommand::local_animation` 在发包时
同步写入快照；`tick_animations(now)` 每帧把播完的一次性动作归一化
（走/跑/攻击/受击 → Standing，死亡 → Dead），渲染层只管按 `frame_at` 选帧。

## 4. 键盘与鼠标

### 键盘（联机模式）

本次动作动画改动**不改变任何按键**：走/跑/攻击/受击/死亡都由收到的包或已有的
移动/攻击键驱动，下面这张表与之前一致。

| 按键 | 行为 |
|---|---|
| 方向键 / WASD | 与当前朝向相同 → `C.Walk`，不同 → `C.Turn`；按住左键走路时方向键仍可用 |
| Shift + 方向 | 与朝向相同 → `C.Run`（服务端仍校验跑步计数） |
| Space | `C.Attack { direction: 当前朝向 }` |
| P | `C.PickUp` |
| T | 打开聊天输入；回车 `C.Chat`，Esc 取消 |
| I | 开关背包 |
| E | 开关装备纸娃娃面板 |
| M | 开关小地图（`ui::minimap`，见 §5.5） |
| F3 | 开关技能列表 |
| 1..9 | 施法（未绑定技能会先发 `C.MagicKey`） |
| 商店打开时 ↑/↓ | 滚动货架（不移动角色） |
| Esc | 依次关闭：右键菜单 → 拖拽 → 聊天输入 → NPC 对话（发 `@exit`）→ 背包/装备/技能（小地图只由 `M` 开关，与 Crystal 的 `MiniMapDialog.Toggle()` 一致） |
| F1 | 调试叠层 |
| F2 | 截图到 `build/screenshot-<n>.png` |
| Q | 退出 |

离线模式：方向键/WASD 平移相机、`S` 截图、`F1` 叠层、`Q` 退出。

### 鼠标

| 操作 | 位置 | 行为 |
|---|---|---|
| 左键单击 | NPC 对话按钮 | `C.CallNPC` |
| 左键单击 | 商店行 | 买一件（`C.BuyItem`） |
| 左键单击 | 技能行 | 选中并按绑定键施法 |
| 左键单击 | 背包格 | 选中该格（并开始拖拽） |
| 左键双击 | 背包物品 | `C.UseItem { unique_id }`（药品/卷轴/技能书） |
| 右键 | 背包物品 | 弹出「使用/装备/丢弃」菜单（`C.UseItem` / `C.MoveItem` / `C.DropItem`） |
| 拖拽 | 背包 → 装备槽 | 穿戴：`C.MoveItem { grid: Inventory, from: 背包格, to: 装备槽 }` |
| 拖拽 | 装备槽 → 背包 | 脱下：`C.MoveItem { grid: Equipment, from: 装备槽, to: 背包格 }` |
| 拖拽 | 背包 → 背包 | 交换：`C.MoveItem { grid: Inventory, from, to }` |
| 悬停 | 背包/装备物品 | 跟随鼠标的 tooltip（名字/类型/需求/属性/耐久/堆叠/价格） |
| 滚轮 | 商店面板 | 货架滚动（同时画比例滚动条） |
| 左键单击/按住 | 地面 | 朝该格走一步；按住连续走 |
| 左键单击 | NPC（8 格内） | 打开对话（`C.CallNPC { key: "@main" }`） |

点地走路的规则（`ui::walk_intent`，**不做 A\***）：目标与自己在同一行/列且相邻 →
直接发 `C.Walk`；否则朝目标方向——已经面向该方向就再走一步，否则先 `C.Turn`。
按住左键时 `app.rs` 每 150 ms 重新计算一次，坐标变化后自然转向/继续走。

Crystal 客户端的键盘移动不是逐键发 `C.Walk`：`GameScene.CheckInput` 用
`MouseDirection()` 得到方向并解析成 `MirAction.Walking/Running/Standing`
（`Client/MirScenes/GameScene.cs:11522-11553`，方向函数 `:12017`），随后
`PlayerObject` 在动作开始时发 `C.Walk`、`C.Run` 或 `C.Turn`
（`Client/MirObjects/PlayerObject.cs:1313-1334`）。本作没有动作缓冲与寻路，
把「方向是否等于当前朝向」直接映射成 Turn/Walk 两种动作；Shift 强制跑。

## 5. 面板与交互

### 背包与物品

* 46 格背包（`Globals.InventorySize = 46`）、图标、堆叠数量、持久度条；
* 双击（同一格 350 ms 内两次按下，`ui::inventory::DOUBLE_CLICK_MS`）→ `C.UseItem`；
* 右键菜单：使用 / 装备（仅可装备类型）/ 丢弃；点菜单外关闭菜单；
* 拖拽时画一个半透明图标幽灵（`ui::inventory::draw_drag_ghost`），松手时按落点
  产生 `MoveItem`（起点/落点网格分别记录，见 `NetCommand::MoveItem` 文档）。

### 装备纸娃娃（E）

`ui::equipment` 画 14 个装备槽的**实际装备图标**（`items.wzl` 里该物品的 `Image`，
缺失时只显示模板编号）+ 名称，并在下方汇总
DC/AC/MAC/魔法/道术/准确/敏捷/攻速/幸运。数据来自
`UserInformation.equipment` 与 `NewItemInfo`；槽位顺序与 Crystal
`EquipmentSlot`（`Shared/Enums.cs`）一致，角色面板同款布局见
`Client/MirScenes/Dialogs/CharacterDialog.cs:227-320`。悬停装备槽同样弹 tooltip。
**地图上**的纸娃娃（身体/武器/头发分层）见 §6.1；面板中央的角色小预览还没做（§8）。

### 物品 tooltip

`ui::tooltip` 跟随鼠标（偏移 16 px），贴到屏幕右/下边缘时翻到鼠标另一侧并收边。
内容按 Crystal `NameInfoLabel`（`GameScene.cs:6835-6905` 的名字/耐久/类型段，
`:7128+` 的属性段）依次为：名字（按品级着色）、类型、需求、攻/防/魔防/魔法/道术、
准确/敏捷/攻速/幸运、持久度、堆叠、价格。

### 商店滚动

商品多于可见行（`ui::shop::MAX_SHOP_ROWS = 11`）时，滚轮或上下键滚动，
右侧画轨道 + 比例滑块，底部显示 `当前页首/总数`。
Crystal 的商店同样用滚轮翻页（`Client/MirScenes/Dialogs/NPCDialogs.cs:1310-1330`）。

### 增益栏与光效

状态栏下方显示可见 buff 的图标（增益名首字 + 类型色块）、名称与剩余秒数
（`8秒` / `∞` / `暂停`）。装备或商店面板打开时增益栏自动右移，避免被左侧面板盖住。
`S.AddBuff` 自带 `ExpireTime`，UI 用本地时钟倒计时；`S.RemoveBuff` 立即移除。
`S.ObjectEffect` 会在目标脚下画一圈半透明圆环（约 0.6 s 淡出），
对应 Crystal 的 `ObjectEffect` 处理（`GameScene.cs:4717+`）。

### 世界内文字

* 头顶名字：玩家/怪物/NPC 的名字画在精灵上方，带 8 方向黑描边；
* 伤害数字：`ObjectStruck`（无伤害值 → 红色 ✕）与 `DamageIndicator`
  （数字 / `闪避` / 暴击加 `!`）在目标头顶向上飘 50 px、1 s 内淡出。
  时长与位移对应 Crystal `Client/MirObjects/Damage.cs:16-46`
  （`duration = 1000`、`Distance = 50`、`-75` 的基准偏移）。

### 5.5 小地图（M）

面板贴屏幕右上角，半透明底 + 边框，从上到下是：地图名（`MapSnapshot.title`）、
120×108 的小地图视窗、坐标行（`(x, y)`）与 `M 关闭` 提示。

映射与几何全部照抄 Crystal 的 `MiniMapDialog`：

| 项 | 本项目 | Crystal 出处 |
|---|---|---|
| 图库 | `resource/data/mmap.wzl`（`mmap.wzx` 索引） | `Client/MirGraphics/MLibrary.cs:23` `MiniMap = new MLibrary(Settings.DataPath + "MMap")` |
| 图像号 | `S.MapInformation.mini_map`（`0` = 没有小地图） | `Shared/ServerPackets.cs:479`；`MirDatabase/MapInfo.cs:20` 的 `MapInfo.MiniMap` |
| 面板位置 | `x = 屏宽 - 126`、`y = 0` | `Client/MirScenes/Dialogs/MainDialogs.cs:1780` |
| 标题行 | 高 18 | `MainDialogs.cs:1786-1793` |
| 视窗 | 面板内 `(3, 22)`、120×108 | `MainDialogs.cs:1912-1914` |
| 坐标行 | 高 18 | `MainDialogs.cs:1795-1802`、`:2079-2080` |

投影公式（`mir-render/src/minimap.rs`，与 `MainDialogs.cs:1916-1943` 逐行对应）：

```text
scaleX = 图像宽 / 地图宽(格)        scaleY = 图像高 / 地图高(格)
view.x = (int)(scaleX × 自己.x) - 120/2     越界时贴右/下边，再夹到 0
view.y = (int)(scaleY × 自己.y) - 108/2
点像素 = 视窗原点 + (格坐标 - view/scale) × scale
```

* 图像是 **1:1 裁剪**绘制的（`MainDialogs.cs:1933` 的 `Draw(index, viewRect, …)`），
  缩放只用于「格 → 像素」换算；地图宽高优先用 `S.NewMapInfo`，没有时退回本地地形尺寸
  （Crystal 用 `MapControl.Width/Height`）。
* 点位颜色与 Crystal 一致：队友蓝（`0,0,255`）、其他玩家白、NPC 绿（`0,255,50`）、
  怪物等红（`255,0,0`），2×2 像素；**自己画黄色**（Crystal 是白色，
  这里按需求改色，`MainDialogs.cs:1950-1962`）。
* 与 Crystal 一样跳过地面物品、尸体、法术对象（`MainDialogs.cs:1941`）。
  队友点已经支持（`MarkerKind::Group`），但本作还没有组队数据来源，所以目前不会出现。
* 缺素材（没有 `mmap.wzl`，或 `MapInformation.mini_map == 0`）时：视窗画纯色占位
  （`[26,34,46]`）+ `无小地图素材`，标题仍然显示地图名，不 panic。
  Crystal 对应的处理是把面板切成无图小档（`MainDialogs.cs:1898-1901,2046-2058`）。
* 面板开着时点在它上面的鼠标不会变成「点地走路」（`UiState::minimap_clicked`）。

### 5.6 音效

事件 → 文件名 的映射是**纯数据**，放在 `crates/mir-render/src/sound.rs`（`SoundId` +
`sound_for_event`）；播放只在 `crates/mir-client/src/audio.rs`（`rodio`）——`mir-render`
不依赖任何音频库。

文件名来自 Crystal 客户端的 `Sound/SoundList.lst`
（`Client/MirSounds/SoundManager.cs:85-92` 查表；查不到才用编号公式
`index/10` + `-` + `index%10`，见 `:87-89`）：

| `SoundId` | 事件 | Crystal 常量（`SoundList.cs` 行） | 编号 | `SoundList.lst` 行 | 文件 |
|---|---|---|---|---|---|
| `Walk` | 走路 | `WalkGroundL`（`:61`） | 10001 | `:2` | `1.wav` |
| `Run` | 跑步 | `RunGroundL`（`:63`） | 10003 | `:4` | `3.wav` |
| `Swing` | 攻击挥空 | `SwingShort`（`:102`） | 10050 | `:36` | `50.wav` |
| `Hit` | 攻击命中 | `StruckBodySword`（`:119`） | 10070 | `:52` | `70.wav` |
| `Struck` | 受击 | `MaleFlinch` / `FemaleFlinch`（`:131-132`） | 10138/10139 | `:83-84` | `138.wav` / `139.wav` |
| `Die` | 死亡 | `MaleDie` / `FemaleDie`（`:133-134`） | 10144/10145 | `:85-86` | `144.wav` / `145.wav` |
| `LevelUp` | 升级 | `LevelUp`（`:49`） | 10156 | `:93` | `levelup.wav` |
| `Pickup` | 拾取 | `PetPickup`（`:177`，简化） | 10520 | `:145` | `pet_pickup.wav` |
| `Gold` | 金币（得/失） | `Gold`（`:44`） | 10106 | `:68` | `106.wav` |
| `Drink` | 喝药 | `ClickDrug`（`:46`） | 10108 | `:70` | `108.wav` |
| `Cast` | 施法 | `20000 + Spell×10`（`SoundManager.cs:87-89`） | 20000+ | — | `M<技能号>-0.wav`（缺省 `M31-0.wav`） |
| `NpcDialog` | 打开 NPC 对话 | `ButtonA`（`:41`，简化） | 10103 | `:65` | `103.wav` |

触发来源：

* **别人**的动作由快照事件触发（`WorldSnapshot.sound_events`，`net.rs` 在
  `ObjectWalk`/`ObjectRun`/`ObjectAttack`/`ObjectStruck`/`ObjectDied` 处压事件）；
  升级（`S.LevelChanged`）、拾取（`S.GainedItem`）、金币（`S.GainedGold`/`S.LoseGold`）、
  挨打（`S.Struck`）、NPC 对话（`S.NPCResponse` 非空页：开窗与翻页都响，空页不响）同理。
* **自己**的走/跑/攻击/施法/喝药在**发包前本地预播**（`LiveState::send`、
  `dispatch_ui_action`）：服务端不会把 `ObjectWalk`/`ObjectAttack` 回发给动作发起者
  （`mir-server/src/world.rs:2428-2449`，Crystal 也一样），不本地播就没有自己的脚步声。
* 频控：同一音效 100 ms 内只播一次（`mir_render::SOUND_COOLDOWN_MS`，`SoundThrottle`）。
  Crystal 没有这张表（它的节流靠步帧播放 `PlayerObject.cs:3596-3599` 与延迟队列
  `SoundManager.cs:79-82`），本作按需求加了一层。频控同时挡住「本地预播 + 服务端回显」
  可能造成的双响。
* 素材目录按顺序找同名文件：`resource/wav`（786 个经典 WAV）→
  `resource/community/gqf2008-crystal/Sound`（Crystal 完整音效集，补上
  `levelup.wav`/`pet_pickup.wav`/`M<技能>-*.wav`）。找不到就静默跳过（`tracing::debug!`）。
* 降级：`rodio` 打不开默认输出设备时只打印**一次**警告，之后所有播放都是空操作，
  窗口/渲染/网络完全照常（CI 没有声卡）。音量默认 0.5，`--volume` / `settings.json` 可调。
* 简化：走路统一用「普通地面」音（Crystal 按地块选 8 种，`PlayerObject.cs:3601-4208`）；
  受击不区分武器类型；喝药用 `ClickDrug`（真正的 `EatDrug` 在 Crystal 客户端里
  **从未被调用**）；NPC 对话没有专用音效，借用界面按钮音；怪物攻击/死亡不按 `BaseSound`
  取各自音效（`MonsterObject.cs:217,601,1395`）。

## 6. 实体图库映射

扁平素材（`resource/data/*.wzl`）与 Crystal 的「一个外观一个 `.Lib`」不同，因此用文件名
+ 固定步长定位；映射表如下：

| 实体 | 文件 | 图像号 → 帧号 | Crystal 出处 |
|---|---|---|---|
| 玩家身体 | `hum.wzl` / `hum2.wzl` / `hum3.wzl` | `block*600 + action.start + direction*stride + n`（动作表见 §6.3） | 外观选择 `PlayerObject.cs:553`；帧号 `PlayerObject.cs:761`；玩家帧集合 `Frames.cs:157-171`；身体绘制 `PlayerObject.cs:5045` |
| 玩家武器 | `weapon.wzl` / `weapon2.wzl` / `weapon3.wzl` | 同上（与身体**同步**，同一个动作帧号） | `PlayerObject.cs:415-431`、`:5062-5073` |
| 玩家头发 | `hair.wzl` / `hair2..4.wzl` | 同上（与身体**同步**） | `PlayerObject.cs:554`、`:5054` |
| 玩家盾牌 | `shield.wzl`（语料里**没有**） | 同上 | Crystal 没有盾牌层（见 §6.2） |
| 怪物 | `mon{image/10+1}.wzl` | `(image%10)*430 + action.start + direction*stride + n`（默认动作表见 §6.3） | 图像 → 图库 `MonsterObject.cs:208`；默认怪物帧集合 `Frames.cs:25-31`；绘制 `MonsterObject.cs:4307-4309` |
| NPC | `npc.wzl`、`npc2.wzl`…（每 100 个图像换库） | `(image%100)*60`，取站立首帧 | 图像 → 图库 `NPCObject.cs:77-80`；帧号 `NPCObject.cs:101`；默认 NPC 帧集合 `Frames.cs:18-19` |
| 物品 | `items.wzl`（缺失时 `dnitems.wzl`） | 帧号 = `ObjectItem.image`；金币按数量用 112..116 | `ItemObject.cs:32,37`（物品）、`:49,55-64`（金币）、`:72-73`（绘制） |

几何锚点：

* 玩家/怪物/NPC：格原点 + 图像头里的 `offset_x/offset_y`（`MLibrary.cs:658-665`，
  `PlayerObject.cs:5045` 的 `offSet: true`）；玩家的**四层**共用同一个格原点，各自叠加
  自己的图像偏移，所以不需要额外的层间偏移常量；
* 物品：在格子里居中（`ItemObject.cs:78-79`），不叠加图像偏移（`:72-73` 的默认 `offSet: false`）；
* 绘制顺序：地图 Y 升序、同一行 X 升序（`GameScene.cs:10798-11008`）。
* 精灵缺失：画 12×18 的彩色方块（玩家蓝、怪物红、NPC 绿、物品黄；死亡为灰色），
  物件有 `ObjectHealth` 时额外画 3px 血条。无 `resource/` 的 CI 也能得到非空画面。

### 6.1 玩家装备分层（纸娃娃）

`mir_render::EntityView` 带五个外观字段，`EntityRenderer` 按 Crystal 的顺序逐层叠加：

```text
朝 Left/Up/UpLeft/DownLeft：武器 → 身体 → 头发 → 盾牌
朝 UpRight/Right/DownRight/Down：身体 → 头发 → 盾牌 → 武器
```

顺序来自 `PlayerObject.Draw`（`Client/MirObjects/PlayerObject.cs:4877-4926`）：

| 步骤 | Crystal | 说明 |
|---|---|---|
| 1 | `:4884` `DrawMount()` | 坐骑，本作未做 |
| 2 | `:4886-4892` 方向∈{Left,Up,UpLeft,DownLeft} → `DrawWeapon()`（身体**之前**） | 朝左上/上/左下/左时武器被身体挡住 |
| 3 | `:4894` `DrawBody()` | 身体层 |
| 4 | `:4896-4911` `DrawHead()` | 头发层（`DrawWings` 在中间，本作未做翅膀） |
| 5 | `:4914-4923` 方向∈{UpRight,Right,DownRight,Down} → `DrawWeapon()`（身体**之后**） | 朝右上/右/右下/下时武器在最前面 |
| 6 | `:4921-4922` 弓箭手 `DrawWeapon2()` | 双持/职业武器，本作未做 |

帧公式（三层完全一致）：

```text
DrawFrame = block * 600 + direction * 8 + n     // n = 0..=3（站立 4 帧）
```

* `block` 是**图库内**的外观块号，不是 `ItemInfo.Shape` 的原始值：
  * 衣服：`Shape` 0..23 → `hum.wzl`，24..53 → `hum2.wzl`，54..99 → `hum3.wzl`（实测块数
    `HUM_BLOCKS = [24, 30, 46]`，越界落在最后一库的最后一块）；
  * 武器：`Shape / 100 + 1` 选库（`weapon.wzl`/`weapon2.wzl`/`weapon3.wzl`）、
    `Shape % 100` 是块号。Crystal 用同一套 100 分段区分普通/刺客/弓箭手武器
    （`PlayerObject.cs:41-55,402-409,497-502`）；
  * 头发：`hair` 0..5 → `hair.wzl`、6..27 → `hair2.wzl`、28..32 → `hair3.wzl`、
    33.. → `hair4.wzl`（`HAIR_BLOCKS = [6, 22, 5, 152]`）。
* 素材实测（`cargo test -p mir-render --test player_layers -- --nocapture`）：
  `hum.wzx` 14400 帧 = 24×600、`hum2.wzx` 18000 = 30×600、`hum3.wzx` 27600 = 46×600、
  `weapon.wzx` 45600 = 76×600、`weapon2.wzx` 25200 = 42×600、`hair.wzx` 3600 = 6×600、
  `hair2.wzx` 13200 = 22×600、`hair3.wzx` 3000 = 5×600；每块内 `方向*8 + 0..3` 有图、
  `+4..+7` 是空位（与 `PLAYER_DIRECTION_STRIDE = 8` 一致）。
* 男/女偏移（Crystal `ArmourOffSet = 808`、`WeaponOffSet = 416`、`HairOffSet = 808`，
  `PlayerObject.cs:584-587`）**不适用**于扁平语料：Crystal 的 `.Lib` 把男女塞在同一个
  文件的两个偏移里，而扁平语料把男女编成**不同的块号**。所以这里不加偏移。
* 头发**不染色**：Crystal 的 `DrawHead` 用的 `DrawColour` 只随中毒状态变化
  （`PlayerObject.cs:865-883,5042-5054`），没有发色参数，也没有 `Settings.DrawBlack`。
* 缺层降级：`weapon/hair/shield == NO_LAYER` 时不画该层；精确帧为空时先退回该外观块的
  站立段首帧（含「站立段整体后移 8 帧」的武器块补偿，见 §9.14），仍为空就跳过该层；
  身体层缺失才画占位方块。
* 持久度为 0 的装备不算外观（`Client/MirObjects/UserObject.cs:234`），所以武器/衣服
  损坏后角色会变回空手/裸体——与 Crystal 一致。

### 6.2 已知的素材/枚举偏差

本作服务端协议里的 `Monster` 枚举是 Crystal 的**紧凑枚举**（例如 `Scarecrow = 5`、
`Skeleton = 22`），而 `resource/data/mon1..mon53.wzl` 是经典 Mir2/盛大布局（怪物图像按
0..=9、10..=19 分组，对应 `Mon1`、`Mon2`…）。两者不是同一套编号，所以演示怪物可能显示成
同一 `monN` 文件里的另一个怪物外观。`resource/data/!Mon.dat` 里有每只怪的图库与动作帧表，
本模块尚未解析；解析后可以做到逐个怪物精确映射。怪物帧号还做了逐级回退：
精确帧 → 该图像站立帧 → 库首帧 → 占位方块，因为并非每个 `monN.wzx` 都恰好有
`10*430` 帧（例如 `mon1.wzx` 只有 1086 帧）。

### 6.3 动作帧表与时长（`mir-render/src/animation.rs`）

Crystal 的 `Frame` 是 `Start / Count / Skip / Interval`（`Client/MirObjects/Frames.cs:203-244`），
绘制帧号 `DrawFrame = Frame.Start + (Frame.OffSet * Direction) + FrameIndex`
（`PlayerObject.cs:761`、`MonsterObject.cs:357`），其中 `Frame.OffSet = Count + Skip`
（`Frames.cs:209-212`）。**`Skip` 是每个方向段末尾的空槽数，不是播放前跳过的帧**。

扁平 `resource/data/hum.wzl` 每个外观块 600 帧，动作顺序与经典 `Hum.wil` 相同，
但每个方向固定 8 帧。`Start/Skip` 按这个布局适配，`Count/Interval` 与 Crystal
完全一致（`Frames.cs:157-170`）：

| 动作 | `ActorAction` | Start | Count | Skip | 步长 | Interval | 时长 |
|---|---|---|---|---|---|---|---|
| 站立 | `Standing` | 0 | 4 | 4 | 8 | 500 ms | 循环 |
| 行走 | `Walking` | 64 | 6 | 2 | 8 | 100 ms | 600 ms |
| 跑步 | `Running` | 128 | 6 | 2 | 8 | 100 ms | 600 ms |
| 攻击 1 | `Attacking` | 200 | 6 | 2 | 8 | 100 ms | 600 ms |
| 攻击 2 | （未选择） | 264 | 6 | 2 | 8 | 100 ms | 600 ms |
| 攻击 3（双手） | （未选择） | 328 | 8 | 0 | 8 | 100 ms | 800 ms |
| 受击 | `Struck` | 472 | 3 | 5 | 8 | 100 ms | 300 ms |
| 死亡 | `Dying` | 536 | 4 | 4 | 8 | 100 ms | 400 ms |
| 尸体 | `Dead` | 539 | 1 | 7 | 8 | 1000 ms | 保持 |

* 192~199 是 8 个方向的 1 帧姿态，所以攻击 1 从 200 开始；这些起点由
  `tests/animation_assets.rs` 对真实 `hum.wzl` 逐帧解码验证。
* 攻击的**输入间隔**在 Crystal 里另由 `User.AttackSpeed` 控制
  （`UserObject.cs:173-174`：`1400 - AttackSpeed*60 - min(370, Level*14)`，最低 550 ms）；
  本作只播放 `Frames` 的可见动作（6 帧 × 100 ms），不模拟攻击输入冷却。
* 攻击统一用 `Attack1`；Crystal 的 `Attack2/Attack3/Attack4`（刺客/双手/随机变招，
  `PlayerObject.cs:1055-1075`）尚未按职业/武器选择。
* 播放规则：站立循环 4 帧；走/跑播完回 Standing 第 0 帧；攻击/受击/死亡播完保持
  最后一帧；`Dead` 永远停尸体帧（`PlayerObject.cs:2317-2323,2500-2508`）。

怪物默认表来自 Crystal `Frames.cs:25-31`（站立 4×500、行走 6×100、攻击 6×100、
受击 2×200、死亡 10×100、尸体 1×1000）。扁平 `monN.wzl` 没有 Crystal `.Lib` 的
逐怪物 `Frames` 数据（`MonsterObject.cs:276`），所以按 `mon1.wzl` 图像 0 实测布局
保守映射为：站立 0、行走 64、攻击 128、受击 280、死亡 344、尸体 349；
除死亡段按素材实测取 6 帧外，`Count/Interval` 取 Crystal 默认表。未收录怪物至少能
正确播放站立 4 帧 + 走 6 帧；
`!Mon.dat` 的逐怪物精确帧表尚未解析。NPC 只有站立 4×450（`Frames.cs:18-19`），
其它动作回退到站立。

### 6.4 渲染接口

* `EntityView` 新增 `action: ActorAction`（默认 `Standing`）与 `action_started_ms: u64`；
* `EntityRenderer::render_at(..., now_ms)` 用 `ActorAnimation::frame_at(now_ms)` 选帧，
  旧的 `render(..., animation_frame)` 把动画计数按 100 ms/拍换算成时间；
* `MapScene::render_with_entities_at(..., now_ms)` 是窗口用的入口，离线模式仍走
  `MapScene::render`，不受影响；
* 玩家身体/武器/头发/盾牌共用同一个动作帧号；怪物/NPC 的帧号加上图像首帧
  （怪物 `(image%10)*430`、NPC `(image%100)*60`），精确帧缺失时回退站立帧，
  再缺就回退库首帧/占位方块。

## 7. 无窗口端到端测试

`crates/mir-client/tests/live_world.rs`：真实服务端 + 登录 + 走路 + 渲染
`target/client-live.png`。

`crates/mir-client/tests/ui_test.rs`：真实登录后注入假 NPC/商店/背包/聊天，渲染
`target/client-ui.png`，并验证对话按钮/货架/背包格的命中测试。

`crates/mir-client/tests/ui_interaction.rs`：真实登录后注入
`UserInformation`（14 个装备槽 + 背包）、`NewItemInfo`（带 `stats`）、`ObjectMonster`、
`AddBuff`/`RemoveBuff`、`ObjectStruck`/`DamageIndicator`，然后：

1. 悬停背包物品 → tooltip 矩形内文字像素 > 0；
2. 双击背包 → `UiAction::UseItem` 且 `net_command()` 等于 `NetCommand::UseItem`；
3. 拖拽背包 2 号格到武器槽 → `NetCommand::MoveItem { grid: Inventory, from: 2, to: 0,
   to_grid: Equipment }`；
4. 注入时钟 `T`，在 `T+0.5s` 画帧断言伤害数字像素存在、`T+2.6s` 断言消失；
5. buff 文本含剩余秒数（`魔法盾 8秒`）且状态栏下方有文字像素；
6. 输出 `target/client-interact.png`。

`crates/mir-client/tests/anim.rs`：**注入式动作动画测试**（不连服务器）。用
`WorldSnapshot::apply_packet_at(packet, now_ms)` 注入固定时间：

1. `ObjectWalk` 后实体 `action == Walking`、`action_started_ms` 等于注入时间；
   `frame_at(t)` 与 `frame_at(t+300ms)` 返回**不同帧**；599 ms 仍在走、600 ms 回
   `Standing` 且帧号等于站立第 0 帧；
2. `ObjectAttack` 600 ms、`ObjectStruck` 300 ms、`ObjectDied` 400 ms 后分别回
   Standing/Dead；
3. 自己的 Walk/Run/Attack/Turn 通过 `apply_local_action` 驱动（`NetCommand::local_animation`），
   不依赖服务端回显；
4. 用真实素材渲染 `target/client-anim.png`（一个走路的玩家 + 一只攻击中的怪物），
   断言非黑像素 > 2000、颜色 > 8，且两个时间点的画面差异 > 100 像素。

`crates/mir-render/tests/animation_assets.rs`：**真实素材动作帧冒烟**。缺 `resource/`
时打印跳过；有素材时对 `hum.wzl` 块 0 的站立/走/跑/攻击 1/2/3/受击/死亡/尸体，
以及 `mon1.wzl` 图像 0 的站立/走/攻击/受击/死亡/尸体，逐个方向、逐帧解码并打印
「N 帧/方向 · 步长 · 起止帧号 · 间隔」。

`crates/mir-client/tests/player_looks.rs`：**注入式纸娃娃测试**（不连服务器）。用
`WorldSnapshot::apply_packet` 注入 `S.NewItemInfo`（带 `shape` 的武器/衣服模板）、
`S.UserInformation`（14 槽装备 + 发型）、两个 `S.ObjectPlayer`（朝上的路人甲、朝下的
路人乙），断言：

1. 快照推导出 `armour = 2`、`weapon = weapon_shape = 36`、`hair = 2`；
2. `entity_views()` 把同样的层号交给渲染层（`EntityView` 是唯一接口）；
3. `S.EquipItem`（换 210 段武器）后 `weapon` 刷新成 `10`、`weapon_shape` 变成 `210`，
   且重绘后角色区域**有像素变化**；
4. 整帧非空白（非黑像素 > 5000、颜色 > 20 种）、装备面板区域有内容；
5. 输出 `target/client-looks.png`（真实素材：人物站在 `0.map` 上，装备面板同时打开）。

`crates/mir-render/tests/player_layers.rs`：**真实素材冒烟**。缺 `resource/` 时打印
跳过原因；有素材时断言 `hum.wzl`/`weapon.wzl`/`hair.wzl` 的帧总数（14400/45600/3600）、
若干 (外观块, 方向) 的站立帧能解码、语料里**没有** `shield.wzl`，并把
「裸体 / 衣服+头发 / 衣服+武器+头发」三种外观 × 8 方向渲染到
`target/players-layers.png`，断言每多一层画面都会变。

`crates/mir-render` 单元测试（`src/entities.rs` 的 `mod tests`）用合成 `.wzl`/`.wzx`
（RGB24 纯色）验证层合成：带装备的帧与裸体帧不同、武器/头发颜色确实出现在身体区域、
**朝下时武器盖住身体、朝上时身体盖住武器**（层顺序画反就会被抓出来）、缺层时安静降级。

`crates/mir-render/tests/minimap.rs`：**小地图渲染**。

1. 现场拼一个最小的 `mmap.wzl` + `mmap.wzx`（`mir-asset` 只读不写，测试按
   `WeMadeLibrary.cs:140-147,313-331` 的记录格式自己写），用
   `MinimapScene::open_dir` 打开，断言空帧/越界帧返回 `None`、3 号帧是 160×120；
2. 画到 `Frame`：底图颜色正确、玩家点是 2×2 黄色、怪物点是红色、视窗外不画；
   同一张图重复画逐字节一致；
3. 真实素材：解 `resource/data/mmap.wzl` 前 64 帧并打印尺寸（第一张是 0 号 600×399），
   用真实底图画一帧到 `target/minimap-real.png`，断言非黑像素 > 1000。

`crates/mir-client/tests/minimap_panel.rs`：**M 面板无头测试**（不连服务器）。

1. 注入 `S.UserInformation` + `S.MapInformation`（小地图号）+ `S.NewMapInfo`（宽高）
   + `S.ObjectMonster` + 一个 NPC，断言 `view_for` 的图号/宽高/点位；
2. `UiState::toggle_minimap()` 后 `show_minimap` 为真，鼠标落在面板上时点击被吃掉
   （不会变成走路）；
3. 渲染整屏到 `target/client-minimap.png`，断言面板区域变化 > 5000 像素、
   玩家点（黄）≥ 4 像素；
4. 缺素材时视窗是纯色占位；`mini_map == 0` 时即使图库里有 0 号帧也不画。

`crates/mir-client/tests/sound.rs`：**音效事件 + 真实 WAV 冒烟**（不连服务器、不开窗口）。

1. 注入 `ObjectWalk`/`ObjectRun`/`ObjectAttack`（带技能）/`ObjectStruck`/`S.Struck`/
   `ObjectDied`/`LevelChanged`/`GainedItem`/`GainedGold`/`NPCResponse`，断言事件顺序、
   `GainedGold(0)` 与空 NPC 页不产生事件、编号严格递增且不重复消费；
2. 本地事件（`push_local_sound`）与队列上限；
3. 100 ms 频控（`SoundThrottle`）与无设备播放器的空操作行为；
4. 真实素材：`resource/wav` 里 12 个映射文件逐个用 `rodio::Decoder` 解码并打印
   采样率/声道（至少 5 个必须成功，缺素材时打印跳过），并确认 12 个事件的文件都能定位。

```powershell
cargo test -p mir-client --test sound -- --nocapture
cargo test -p mir-client --test minimap_panel -- --nocapture
cargo test -p mir-render --test minimap -- --nocapture
cargo test -p mir-client --test ui_interaction -- --nocapture
cargo test -p mir-client --test player_looks -- --nocapture
cargo test -p mir-client --test anim -- --nocapture
cargo test -p mir-render --test player_layers -- --nocapture
cargo test -p mir-render --test animation_assets -- --nocapture
cargo test -p mir-client -p mir-render
# `--no-deps` 只跳过依赖 crate 的 lint（mir-client 的 dev-dependency 会拉进
# mir-server；并行开发行会系统时它可能暂时有自己的 clippy 告警）。
cargo clippy -p mir-client -p mir-render --all-targets --no-deps -- -D warnings
cargo fmt --all -- --check
```

## 8. 尚未实现

* 坐骑、翅膀、武器发光（`CHumEffect`/`WeaponEffect`）、中毒染色与等级特效；
* 移动插值（Crystal `OffSetMove`/`Movement` 的平滑位移，`PlayerObject.cs:765-810`）：
  实体直接站在服务端给的格子上，动作帧逐帧播放，但没有格间滑动；
* 攻击变招（Attack2/3/4 按职业/武器/随机选择，`PlayerObject.cs:1055-1075`）、
  弓箭手用 `WalkingBow/RunningBow/AttackRange*`、刺客 `Sneek`；目前统一用 Attack1；
* 怪物逐图像帧表：`!Mon.dat` / Crystal `.Lib` 的 `BodyLibrary.Frames` 尚未解析，
  只用 §6.3 的保守默认；某些怪物（如 `mon1.wzl` 图像 1/2）的受击/死亡段不在默认
  偏移上，会回退到站立帧；
* 刺客双持（`WeaponLibrary2`）与弓箭手第二遍武器（`PlayerObject.cs:4921-4922`）；
* 装备面板里的角色小预览（面板目前只有 14 个槽的图标 + 名称 + 属性汇总）；
* A\* 寻路（点地走路只有「朝目标方向一步」）；拾取/攻击的鼠标目标选择；
* 地图切换后的素材重载（服务端 `MapChanged` 会更新快照地图名，窗口仍用启动时加载的地形）；
* **音效**：背景音乐（`SoundManager.PlayMusic`）、循环音（`LoopProvider`）、
  怪物各自的 `BaseSound`、按地块选择脚步音、按武器选择受击音、按距离衰减、
  音量条 UI（`MainDialogs.cs:2864-2887`）、解码缓存与回收
  （Crystal 的 `CachedSound` / `SoundManager.cs:195-223`；本作每次播放现解码，
  短 WAV 够快，但没有缓存与 30 秒回收）；
* **小地图**：大地图面板（`BigMapDialog`，图像号用 `MapInfo.BigMap`）、NPC 任务感叹号、
  两档尺寸/底部按钮（邮件/大地图/亮度）、滚轮缩放的「大地图搜索」；
* 天气、光照（`LightSetting`/`MapDarkLight`）、聊天频道切换与物品链接；
* 交易/仓库/摆摊/邮件/任务面板；装备升级/镶嵌/觉醒等数值系统；
* `!Mon.dat` 精确怪物外观映射（见 §6.2）。

## 9. 对拍 Crystal 时发现的怪癖

1. **`MapInformation` 不带 NPC/传送点**：它们走单独的 `S.NewMapInfo`（`PlayerObject.cs:1054`），
   所以客户端必须同时处理两个包，不能只等 `MapInformation`。
2. **自己的走路只回 `UserLocation`**：服务端把 `ObjectWalk` 广播给**别人**，自己收到的是
   `UserLocation`（`mir-server` 的 `World::walk` 与 `send_user_location`）。因此客户端不能只靠
   `ObjectWalk` 更新自己的坐标；端到端测试断言的是 `UserLocation`。
3. **`StartGame{Result=4}` 在 `UserInformation` 之前**：`result=4` 只表示服务器放行，
   真正的对象 ID/坐标在随后的 `UserInformation` 里；客户端把「InGame」和「有 self_player」
   分成两个条件（`WorldSnapshot::is_ready`）。
4. **版本哈希不是版本号**：Crystal 对客户端 exe 算 MD5（`LoginScene.cs:133-152`），
   不是字符串哈希，也不是 CRC；服务端 `check_version = false` 时长度对即可。
5. **Crystal 的键盘移动是间接的**：输入层只产生鼠标方向与动作，`PlayerObject` 再按动作发
   Turn/Walk/Run（`GameScene.cs:11522-11553`、`PlayerObject.cs:1313-1334`）；没有「按方向键
   直接发包」的路径。本客户端用「方向是否等于当前朝向」复刻动作判定。
6. **金币是物品图库里的 112..116 帧**（`ItemObject.cs:55-64`），不是独立图库。
7. **物品在格子里居中**，而角色/怪物用格原点 + 图像偏移；两者锚点不同（`ItemObject.cs:78-79`
   对比 `PlayerObject.cs:5045`）。
8. **`.wzl` 调色板 alpha 全 255**：实体和地图图一样需要把纯黑当透明（`remove_black`）；
   Crystal 的 `.Lib` 在生成时已经做过这一步。
9. **扁平素材的 NPC/怪物外观不是 Crystal 的枚举编号**，见第 5 节末尾。
10. **装备穿脱在 Crystal 客户端是两条包**（`C.EquipItem` / `C.RemoveItem`，
    `MirConnection.cs:1297-1307`），本作服务端把它们并进 `C.MoveItem { grid: Equipment }`
    （`mir-server/src/items.rs` 的 `World::move_equipment`）；客户端用起点/落点两个网格
    判断方向，网络层再折算成服务端期望的 `grid = Equipment`。
11. **`S.DamageIndicator` 才是伤害数字的来源**，`S.ObjectStruck` 只带受击动作
    （`GameScene.cs:3452-3510` 对比 `:3511-3540` 的 `DamageIndicator`）；本作两者都会
    生成飘字，`ObjectStruck` 用「✕」标记。
12. **扁平语料把男/女外观编成不同块号**，而 Crystal 的 `.Lib` 用 `ArmourOffSet = 808` /
    `WeaponOffSet = 416` / `HairOffSet = 808`（`PlayerObject.cs:584-587`）在同一个文件里
    翻页。所以本作**不**加这些偏移：`ItemInfo.Shape` 直接就是 `hum*.wzl` 的块号。
13. **`weapon.wzl` 的前两块、`hair.wzl` 的前两块都是空的**（武器形状 0/1 = 空手，
    发型 0/1 = 光头）。`EntityView::weapon == NO_LAYER` 与「块号落到空块」都会让该层
    安静消失，不会画出别人的帧。
14. **`weapon.wzl` 的块内相位不整齐**：76 个武器外观块里有 25 个的站立段整体**后移 8 帧**
    （块首 8 帧全空；`weapon2.wzl`/`weapon3.wzl` 与 `hum*`/`hair*` 都是对齐的）。
    渲染器用「块首 4 帧里有没有图」判断相位并按块缓存补偿（`EntityRenderer::phase_offset`），
    否则朝上（方向 0）的武器会整层消失、其他方向会取到隔壁方向的帧。
15. **扁平 `hum.wzl` 的每个方向占 8 帧，而 Crystal 的 `Frames.cs` 是按 `.Lib` 的连续
    布局写的**（站立每方向 4 帧、行走每方向 6 帧）。所以动作 `Start` 不能直接照抄：
    扁平布局是站立 0~63、走 64~127、跑 128~191、攻击 192~263（前 8 帧是姿态，
    攻击 1 从 200 开始）、双手攻击 264~327、强行攻击 328~391、施法 392~455、
    挖肉 456~471、受击 472~535、死亡 536~599。`Frame.Skip` 在原版里也是
    「每方向末尾空槽」而不是「播放前跳过」（`Frames.cs:209-212`），
    `mir-render/src/animation.rs` 的常量表按这个语义填写。
16. **`SoundList.cs` 的编号不等于文件名**：`SoundManager.PlaySound(index)` 先查
    `Sound/SoundList.lst`（`SoundManager.cs:85-92`），表里 10001 对应 `1.wav`、10106 对应
    `106.wav`、10156 对应 `levelup.wav`；只有查不到时才用 `index/10` + `-` + `index%10`
    的公式（`:87-89`）。所以「按公式算出来的 `1000-1.wav`」在语料里**并不存在**，
    必须用表里的名字。
17. **`SoundList.EatDrug`（10107）在整个 Crystal 客户端里从未被调用**，真正响的喝药音是
    点背包药品时的 `ClickDrug`（10108，`MirItemCell.cs:2047-2048`）。
18. **NPC 对话没有打开音效**：`GameScene.NPCResponse`（`GameScene.cs:3891-3913`）只
    Show/Hide 面板；界面按钮的音效来自 `MirControl.OnMouseClick` 的 `Sound` 属性
    （`MirControl.cs:823-824`），默认 `ButtonA`（`SoundList.cs:41`）。本作用它当「打开对话」音。
19. **小地图的两个图像号是两个控件**：`MiniMapDialog` 用 `MapControl.MiniMap`
    （`MainDialogs.cs:1916`），`BigMapDialog` 用 `MapInfo.BigMap`（`BigMapDialog.cs:642`），
    但都取自同一个 `mmap.wzl`（`MLibrary.cs:23`）。本项目小地图面板只做前者，
    `big_map` 已经在快照里备好（`MapSnapshot::big_map`）等大地图面板用。
20. **`mmap.wzl` 的 0 号帧是真实地图图**（600×399），不是「空图」；Crystal 用
    `MapInfo.MiniMap <= 0` 判断「这张图没有小地图」（`MainDialogs.cs:1898-1907`），
    所以客户端把 0 当哨兵值、不去查图库，否则会画出别的地图。
