# 客户端：联机世界、面板交互与实体渲染

本文记录 `mir-client` 在「离线看地图」之外的联机能力：登录链路、世界快照、输入映射、
物品/装备交互、世界内文字、面板、小地图、音效/背景音乐、坐骑/翅膀、A\* 寻路与无窗口
端到端测试。协议细节见
[`protocol/login-flow.md`](protocol/login-flow.md)，渲染几何见
`crates/mir-render/src/lib.rs` 与 `crates/mir-render/src/geometry.rs` 的模块文档。

## 1. 三种模式

| 模式 | 启动参数 | 行为 |
|---|---|---|
| 离线地图 | `--map 0 --cell 330,270` | 直接读本地 `.map`，方向键/WASD 平移相机；`S` 截图 |
| 联机世界 | `--connect 127.0.0.1:7000 --account NAME --password PASS --character NAME` | 连真实服务端，登录/建角/进入世界；相机跟随自己 |
| 登录窗口 | `--login`（可加 `--server 地址`） | 显示登录页 → 选角页 → 进入世界；服务器地址存本地设置（§5.17） |

联机示例：

```powershell
cargo run -p mir-client -- --connect 127.0.0.1:7000 `
  --account tester --password secret --character 裁决者 `
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
* 界面默认中文；`--lang en` 切换成英文（写入 `config/client.toml` 的 `language`，
  `--set-language` 只改设置；旧 `settings.json` 仍兼容）。
  所有固定文案都在 `crates/mir-client/src/ui/labels.rs`。
* 音效默认开启、音量 0.5（`client.toml` 的 `sound_volume`）；`--no-sound` 与
  `--volume <0..1>` 只影响本次启动，`--sound-dir <目录>` 可追加音效目录。
  没有音频设备时只打印一次警告，窗口照常运行（见 §5.6）。
* 背景音乐默认开启、独立音量 0.5（`client.toml` 的 `music_volume`）；
  `--no-music` 与 `--music-volume <0..1>` 只影响本次启动。切图时按
  `S.MapInformation.Music` / `S.MapChanged.Music` 换曲；编号为 0（示例数据库全部如此）
  时用 `music_for_map` 的主题曲兜底，缺少 `300xx.wav` 时安静静音（见 §5.6）。
* 游戏内设置面板：`O`（Crystal 默认 `F12` 作备用）——音效/音乐/主音量/语言/分辨率/缩放/窗口模式/小地图传送点 + 6 行可重绑键位（§5.15）。
* 商城页签：市场面板的第四个页签，用元宝购买（§5.16）；技能表现（火墙/雷电/瞬息移动/
  隐身/召唤）见 §5.18。

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

* `self_player`：`UserInformation` 里的对象 ID/名字/坐标/朝向/HP/MP/等级/职业/金币/**元宝
  （`credit`）**；
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
  `NPCGoods` 记货架；`GainedItem/GainedGold` 写一条系统消息并更新金币，
  `GainedCredit/LoseCredit` 更新元宝余额（商城购买与 NPC `GIVECREDIT` 用）；
* `ObjectRemove`/`MapChanged` 清理实体；
* `self_player.looks` / `entities[].looks`（`PlayerLooks`）：纸娃娃外观（衣服/武器/发型/
  盾牌/坐骑/翅膀）。自己的衣服/武器由装备槽 + 物品模板推导（Crystal
  `UserObject.RefreshEquipmentStats`，`Client/MirObjects/UserObject.cs:205-250`），
  坐骑/翅膀来自 `S.MountUpdate` / `S.ObjectPlayer`；别人的直接取 `S.ObjectPlayer` /
  `S.PlayerUpdate` 的外观号。每处理完一个包重算一次，重算时会保留 `mount`/`wings`，
  所以装备与模板谁先到都能收敛；
* `map.music`：`S.MapInformation.Music`（`GameScene.cs:2146`）或
  `S.MapChanged.Music`（`GameScene.cs:3985`），窗口每帧用它切背景音乐；
* `quest_infos` / `quests` / `completed_quests`：任务定义（`S.NewQuestInfo`）、进行中任务
  （`S.ChangeQuest` 的进度文本 + 本地接取时刻）、已交付任务编号（`S.CompleteQuest`）；
* `quest_items` / `quest_items_version`：**任务物品栏**（40 格）。`S.GainedQuestItem`(205)
  按 `unique_id` 整堆替换/并堆、`S.DeleteQuestItem`(206) 按 `unique_id`+`count` 扣减；
  `S.UserInformation.quest_inventory` 是整栏快照（`Some` = 替换、`None` = 保留），
  版本号用于 UI 增量重绘（见 §5.8）；
* `lights` / `weather`：`S.TimeOfDay{lights}`（协议 index 61）与
  `S.MapChanged.weather`（没有独立的 `S.Weather`）。客户端已接包，天气随切图显示在 HUD；
  服务端目前**不发** `S.TimeOfDay`，所以当前版本只显示天气、不显示昼夜（见 §5.19）；
* `storage` / `storage_open` / `storage_open_version`：个人仓库 80 格（`S.UserStorage`）与
  「该不该显示仓库面板」（`S.NPCStorage`）；`S.StoreItem`/`S.TakeBackItem` 的回执直接
  搬动背包/仓库里的物品；
* `mails` / `mail_send_request` / `mail_cost` / `mail_sent_result` /
  `parcel_collected_result` / `mail_locked_items`：邮件列表（`S.ReceiveMail`）与写信页/
  操作回执；
* `guild` / `guild_ranks` / `guild_notice` / `guild_storage` / `guild_buff_infos` /
  `guild_active_buffs`：行会概况（`S.GuildStatus`）、职位与成员（`S.GuildMemberChange`
  的状态码语义见 `net::apply_guild_member_change`）、公告、112 格行会仓库与增益
  （`S.GuildStorageList` / `S.GuildStorageItemChange` / `S.GuildBuffList`）；
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

本里程碑新增任务（`Q`）、邮件（`N`）与行会（`G`）面板；仓库面板由服务端的
`S.NPCStorage` 自动打开。`Space` 在锁定目标后变成「朝目标攻击/靠近」，没有目标时
仍是原来的「朝当前朝向挥一下」。走/跑/攻击/受击/死亡都由收到的包或这些按键驱动。
`Q` 现在是任务面板（Crystal 的默认键），退出改到 `F10`。

| 按键 | 行为 |
|---|---|
| 方向键 / WASD | 与当前朝向相同 → `C.Walk`，不同 → `C.Turn`；按住左键走路时方向键仍可用 |
| Shift + 方向 | 与朝向相同 → `C.Run`（服务端仍校验跑步计数） |
| Space | 有锁定目标 → 朝目标攻击（相邻）或朝目标走一步靠近；没有目标 → `C.Attack { direction: 当前朝向 }` |
| Tab | `C.PickUp`（Crystal 的 `KeybindOptions.Pickup = Keys.Tab`，`KeyBindSettings.cs:334`；`P` 让给组队窗口） |
| T | 打开聊天输入；回车 `C.Chat`，Esc 取消；`!!` 前缀 = 组队频道（§5.14）；`@` 开头原样发送（不解析/不本地化，GM/脚本回复按服务端原文显示） |
| Y | 发起交易 `C.TradeRequest`（Crystal 是 `T`，`KeyBindSettings.cs:340`；本作 `T` 已用于聊天，见 §5.13） |
| S | 卖出**背包选中格**（`C.SellItem`）；要求商店面板已打开且选中了背包物品，否则不动作（§5.25）。本地地图查看模式的 `S` 仍是截图 |
| R | 修理**背包选中格**（`C.RepairItem`）；同样要求商店面板打开 + 选中（§5.25） |
| I | 开关背包 |
| E | 开关装备纸娃娃面板 |
| M | 开关小地图（`ui::minimap`，见 §5.5） |
| B | 开关大地图（`ui::bigmap`，见 §5.7） |
| Q | 开关任务面板（`ui::quests`，见 §5.8；Crystal 的 `KeybindOptions.Quests` 默认就是 `Q`，`KeyBindSettings.cs:219`） |
| N | 开关邮件面板（`ui::mail`，见 §5.10） |
| G | 开关行会面板（`ui::guild`，见 §5.11）；打开时自动拉公告/成员/仓库/增益 |
| P | 开关组队面板（`ui::group`，见 §5.14；Crystal `KeybindOptions.Group = Keys.P`，`KeyBindSettings.cs:225`） |
| V | 宠物模式三态循环：跟随 → 攻击 → 休息 → 跟随（`C.ChangePMode`，index 45；`P` 已被组队占用，`V` 是本作扩展键）。没有宠物时只提示 `Label::NoPetHint`、不发包；服务端回执 `S.ChangePMode`(63) 覆盖本地模式 |
| F3 | 开关技能列表 |
| 1..9 | 施法（未绑定技能会先发 `C.MagicKey`） |
| 商店打开时 ↑/↓ | 滚动货架（不移动角色） |
| Esc | 依次关闭：右键菜单 → 拖拽 → 聊天输入 → 大地图 → NPC 对话（发 `@exit`）→ 交易窗（发 `C.TradeCancel`）→ 任务/仓库/邮件/行会/组队/背包/装备/技能（小地图只由 `M` 开关，与 Crystal 的 `MiniMapDialog.Toggle()` 一致）；顺序集中在 `UiState::escape`。输入框聚焦时第一次 `Esc` 只取消焦点 |
| F1 | 调试叠层 |
| F2 | 截图到 `build/screenshot-<n>.png` |
| F10 | 退出（`Q` 现在属于任务面板，与 Crystal 一致） |

> 仓库面板没有独立按键：服务端在 NPC 仓库页发 `S.NPCStorage` 时自动打开（§5.9），
> 与 Crystal 的 `StorageDialog.Show()`（`GameScene.cs:4256`）一致。

离线模式：方向键/WASD 平移相机、`S` 截图、`F1` 叠层、`Q` 退出。

### 移动插值（第 45 轮）

自机的走/跑不再逐格瞬移：客户端记录自己每一步的 `Movement`（起点）与 `CurrentLocation`
（终点），按 Crystal `PlayerObject.cs:765-841` 的 `OffSetMove` 做 6 帧 × 100ms 的整数像素插值，
并把同一偏移加到地形与其他实体上，形成「角色平滑滑动、世界反向滚动」。

**公式（`crates/mir-client/src/camera.rs`）**

```text
index = min(elapsed_ms / 100, 5)                    // 6 帧，每帧 100ms（Frames.cs:158-159）
magnitude_x = (int)((48 * i / 6.0) * (index+1))     // i = 1 走 / 2 跑（PlayerObject.cs:786-790）
magnitude_y = (int)((32 * i / 6.0) * (index+1))     // (int) 向零截断，包住整个乘积（:809-830）
world_offset = (X % 2 + X, Y % 2 + Y)               // 偶化（:834）
```

- `world_offset` 等价于 Crystal 的 `OffSetMove`：指向本次移动的**起点**（走右/走下为负），
  加到地形和**其他**实体上（`GameScene.cs:10680/10684/10814/10820`）。
- `self_motion_offset` 是它的相反数（自机从起点推向目标），写进自机 `EntityView::pixel_offset`；
  相机中心固定为起点格，自机因此恒在屏幕中心 `(384, 256)`（800×600 的 `center_origin()`）——
  等价于 Crystal 对 `User` 不叠加 `User.OffSetMove`（`PlayerObject.cs:847-854`）。
- 走 1 格、跑 2 格：横向 Walk `8,16,24,32,40,48`、Run `16,32,48,64,80,96`；纵向 Walk
  `6,10,16,22,26,32`、Run `10,22,32,42,54,64`（已偶化）。逐帧完整数据见
  [`testing/self-motion/frame-comparison.csv`](testing/self-motion/frame-comparison.csv)。

**实现与生命周期（X1-01 后：motion 与动作同钟）**

- `camera.rs`：`SelfMotion`、`motion_after_authoritative_step`、`prepare_frame`、
  `self_motion_offset`、`world_offset`、`camera_for`、`apply_camera`；
- **motion 起点 = 网络层应用 `S.UserLocation` 的时刻**：`net::apply_self_location`
  在同一个 `now_ms` 里重设自机 `Walking`/`Running` 与 `action_started_ms`，
  `motion_after_authoritative_step` 用该时刻建 `SelfMotion`（`started_ms`）；
  因此发包→回包延迟不会让动作先于插值结束（X1-01）。
- `app.rs::render_current` 每帧调 `prepare_frame`（生产渲染前处理），顺序固定为：
  `tick_animations` → **自然到期**（`now - motion.started_ms >= 600ms` 先清 motion，
  生命周期只看 motion 时钟、不看动作）→ **显式打断**（仍在 600ms 内但自机动作已不是走/跑：
  攻击/受击/死亡/转身等，立即清）→ `apply_camera` 把自机钉在 `motion.from` 并给出世界偏移；
  渲染入口仍是 `MapScene::render_with_entities_at_offset`（`crates/mir-render/src/scene.rs`；
  实体层叠加见 `crates/mir-render/src/entities.rs::render_at_offset`）。
- 只在收到 `UserLocation` 且位移合法（1~2 格直线或 45° 对角）时建插值；同坐标快照
  （聊天/他人动作/撞墙回拉）保留 motion 且不重启动作计时；连续走/跑由最新一步替换；
  传送/出生/切图/重连/断线/死亡另行清空。上述生命周期由 `camera.rs` 新增 6 项单测覆盖
  （`delayed_walk_reply_…`/`delayed_run_reply_…` 的 200/600ms 延迟、
  `same_position_snapshot_…`、`next_authoritative_step_replaces_previous_motion`、
  `explicit_interrupts_clear_motion_immediately`、`teleport_jump_invalidates_motion_and_snaps_to_target`）。

**限制（第 45 轮范围）**

- **只做自己**：其他人/怪物仍按服务端落点逐格跳；Crystal 对其他对象也做 `OffSetMove`
  （`PlayerObject.cs:847-854`、`MonsterObject.cs:435-437`），本作未接（后续清单 P1）。
- 固定 6 帧 / 100ms / 600ms、`i = 2`；Crystal `Sprint` 的 `i = 3` 与本作无关（无疾风步），
  坐骑的 `MountWalking` 8 帧 / `MountRunning` 6 帧（`Frames.cs:189-190`）也未按坐骑帧表插值。
- 地图边缘：相机中心会 clamp 到地图范围（`app.rs::clamp_center`），但正常走/跑的
  `motion.from` 与快照自身坐标都在地图内，**clamp 是 no-op**，自机仍严格在屏幕中心
  `(384, 256)`；该函数只在异常/回退的中心越界时生效。
- 服务端走/跑节奏须保持 600ms（`MOVE_DELAY_MS`）；若改变，插值会在 600ms 处先结束/滞留。
- **本轮未测 FPS/帧耗时（帧率）**：文中出现的性能数字（如 §6.0）仅第 13 轮历史数据，
  不适用于本轮移动插值/相机跟随改动。
- **四类真实窗口边界未实测**（中央移动 / 触边回拉 / 沿边 / 反向离边），
  ≥600ms/跳变只有代码级断言、无窗口录屏佐证；现有 live 证据是**权威回包冻结后的确定性时间重放**，
  不是实时录屏。批次、冻结哈希与覆盖边界见
  [`testing/self-motion/README.md`](testing/self-motion/README.md)。

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
| 拖拽 | 背包 → 个人仓库格 | 存物品：`C.StoreItem { from: 背包格, to: 仓库格 }` |
| 拖拽 | 个人仓库格 → 背包 | 取物品：`C.TakeBackItem { from: 仓库格, to: 背包格 }` |
| 拖拽 | 背包 / 行会仓库格 → 行会仓库格 | 存物品 / 仓库内移动：`C.GuildStorageItemChange { type: 0/2 }` |
| 拖拽 | 行会仓库格 → 背包 | 取物品：`C.GuildStorageItemChange { type: 1 }` |
| 拖拽 | 背包 → 自己的交易格 | 放入交易：`C.DepositTradeItem { from: 背包格, to: 交易格 }`（§5.13） |
| 拖拽 | 交易格 → 背包 | 取回：`C.RetrieveTradeItem { from: 交易格, to: 背包格 }` |
| 拖拽 | 交易格 → 交易格 | 交易栏内移动：`C.MoveItem { grid: Trade, from, to }` |
| 左键单击 | 交易窗「准备 / 确认」/「取消交易」 | `C.TradeConfirm{locked:true}` / `C.TradeCancel`（§5.13） |
| 左键单击 | 交易窗金币框 / 「放入金币」 | 聚焦 / `C.TradeGold{amount}` |
| 左键单击 | 交易/组队邀请弹窗的「接受 / 拒绝」 | `C.TradeReply{AcceptInvite}` / `C.GroupInvite{AcceptInvite}` |
| 左键单击 | 组队成员行 / 「邀请」/「离队」/「踢人」/「允许组队」 | 选中 / `C.AddMember{Name}` / `C.DelMember` / `C.DelMember{队员}` / `C.SwitchGroup`（§5.14） |
| 左键单击 | 任务行 / 任务「放弃」 | 选中任务 / `C.AbandonQuest`（§5.8） |
| 左键单击 | 邮件行 / 收取附件 / 锁定 / 删除 | `C.ReadMail` / `C.CollectParcel` / `C.LockMail` / `C.DeleteMail`（§5.10） |
| 左键单击 | 写信页输入框 / 背包挑选格 / 贴邮票 / 询价 / 发送 | 聚焦输入框 / 切换附件 / 切换邮票 / `C.MailCost` / `C.SendMail` |
| 左键单击 | 行会成员行 / 升职 / 降职 / 踢人 | 选中成员 / `C.EditGuildMember{ChangeType=2/2/1}`（§5.11） |
| 左键单击 | 行会仓库页的资金框 / 存入 / 取出 | 聚焦金额 / `C.GuildStorageGoldChange{Type=0/1}` |
| 左键单击 | 行会增益行的「购买 / 激活」 | `C.GuildBuffUpdate{Action=1/2}` |
| 左键单击 | 行会公告输入框 / 保存 | 聚焦 / `C.EditGuildNotice` |
| 悬停 | 背包/装备物品 | 跟随鼠标的 tooltip（名字/类型/需求/属性/耐久/堆叠/价格） |
| 滚轮 | 商店面板 | 货架滚动（同时画比例滚动条） |
| 悬停 | 世界实体（怪物/玩家/NPC） | 该格画 1px 黄色方框（头顶名字本来就常显） |
| 悬停 | 地面物品 | 在该格画出物品名（`worldtext::draw_item_name`，Crystal `GameScene.cs:10619-10626`） |
| 左键单击 | 怪物 / 其他玩家 | 锁定目标（`UiState::target_object_id`），该格画 1px 红色方框并显示其血条 |
| 左键双击 | 已锁定/点中的怪物或玩家 | 相邻 → `C.Attack`；不相邻 → 朝目标走一步，最多 3 步，走到相邻自动攻击 |
| 左键单击 | 相邻地面物品 | `C.PickUp`（`GameScene.cs:11627-11634`） |
| 左键单击 | 远处地面物品 | 先朝物品走一步并记下靠近进度（≤3 步），相邻后自动 `C.PickUp` |
| 左键单击 | 空地 | A\* 路径逐格走（`UiAction::WalkStep` → `C.Walk`）；不可达时退化为朝目标走一步 |
| 左键按住 | 空地 | 保留旧的连续走（A\* 路径优先，路径存在时不重复发按键走） |
| 左键单击 | NPC（8 格内） | 打开对话（`C.CallNPC { key: "@main" }`） |
| 左键单击 | 大地图/小地图/背包等面板 | 被面板吃掉（不会变成「点地走路」） |

点地走路（`mir-render::pathfind` + `UiState::on_click_with_path`）：

* 点到空地时用 8 方向 A\*（对角要求两侧正交格都可走、节点上限 2000、路径 ≤20 格，
  `pathfind::DEFAULT_MAX_NODES` / `MAX_PATH_STEPS`）算出一条方向序列；
* `UiState` 把剩余路径存进 `walk_path`，每 600 ms（`PATH_STEP_MS`）发一格
  `UiAction::WalkStep` → `C.Walk`；按住 Shift 且连续两格同向时发 `C.Run`；
* 按时间节拍逐格发，不等待服务器回包（回包慢时相当于输入缓冲）；`from`/`expected`
  只记录路径认为角色应该在的位置，路径耗尽即停（以后可加传送/攻击打断校验）；
* 寻路失败或目标不可走时退化为 `ui::walk_intent` 的「朝目标方向走一步」；
  `on_click`（没有可走性闭包的旧入口）仍只走一步/转身，供无头旧测试使用。
* 旧行为保留：按住左键时 `app.rs` 每 150 ms 重新计算一步（`walk_intent`），但
  A\* 路径存在时 `repeat_walk` 直接返回，不抢节拍。

鼠标目标（`ui::target` + `UiState::target_object_id`）对拍 Crystal：

| 本项目 | Crystal 出处 |
|---|---|
| 悬停命中按格判断（跳过自己/尸体/地面物品） | `GameScene.cs:10492-10535`（`MouseObject` 扫描 5×5 格）、`:10605-10606`（不为物品画名字） |
| 悬停/锁定高亮是 1px 方框 | `GameScene.cs:10973-10980` 的 `DrawBlend()`（半透明重绘），本作按需求改成格线框 |
| 单击怪物/玩家 → `target_object_id`；点空地清空 | `GameScene.cs:11442-11451`（NPC/物品/尸体/AI 64/70 不锁定） |
| 双击任意锁定目标即可攻击（含其他玩家） | Crystal 打玩家需要按住 Shift（`GameScene.cs:11473,11568`）；本作没有 PK 开关，双击即攻击（简化） |
| `Space`/双击相邻目标 → `C.Attack` | `GameScene.cs:11471-11509`（相邻时 `CheckInput` 自动挥刀） |
| 不相邻 → 朝目标走一步，≤3 步后放弃，相邻自动攻击 | `GameScene.cs:11764-11773`（超出 1 格时朝目标走）；Crystal 没有步数上限，3 步是需求指定的简化 |
| 双击判定窗口 350 ms | Crystal 无「双击攻击」，这是本作用来表达「打这个」的简化（与背包双击窗口一致，`ui::inventory::DOUBLE_CLICK_MS`） |

锁定目标的血量：只有收到过 `S.ObjectHealth` 的实体才有百分比（`WorldEntity::hp_percent`），
自己的百分比由 `HealthChanged`/`UserInformation` 的 `HP / MaxHP` 算
（Crystal `UserObject.cs:176`）。目标名字来自常显的头顶名字（见 §5「世界内文字」）。

Crystal 客户端的键盘移动不是逐键发 `C.Walk`：`GameScene.CheckInput` 用
`MouseDirection()` 得到方向并解析成 `MirAction.Walking/Running/Standing`
（`Client/MirScenes/GameScene.cs:11522-11553`，方向函数 `:12017`），随后
`PlayerObject` 在动作开始时发 `C.Walk`、`C.Run` 或 `C.Turn`
（`Client/MirObjects/PlayerObject.cs:1313-1334`）。本作没有动作缓冲；键盘移动仍把
「方向是否等于当前朝向」直接映射成 Turn/Walk 两种动作，Shift 强制跑；鼠标点地则走
A\* 路径（见上）。

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
* 头顶血条（`mir-render` 的 `EntityRenderer::render_at`）：32×4 的色块，宽度按
  `PercentHealth` 百分比（`MapObject.DrawHealth`，`Client/MirObjects/MapObject.cs:465-507`
  的 `32 * PercentHealth / 100`、4 像素高），画在精灵顶边上方 6 像素、格内左移 8 像素；
  * 自己的百分比由 `HP / MaxHP` 算（`UserObject.cs:176`），其他人/怪物取
    `S.ObjectHealth.percent`（组队血条 `group.rs:273`、怪物 `world.rs:1511`）——
    没收到过血量的实体不画条；
  * 颜色按百分比分档（>50% 绿、>25% 黄、其余红、死亡灰）：Crystal 用的是
    `Prguse2` 的 1/10/11/12 号预着色条（`MapObject.cs:481-504`），本作没有这套 UI 图库，
    改按百分比着色；玩家蓝条（Crystal `PercentMana`，`:500`）暂未画；
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
* **传送点标记（第 15 轮）**：数据源是 **`S.NewMapInfo.info.movements`**（协议已下发；
  `MapChanged` 不带 → 切图后清空，等下次 `NewMapInfo`）；`MapSnapshot.movements:
  Vec<ClientMovementInfo>`，小地图新增 `MarkerKind::Movement`（青色 `[0,220,220]`）。
  鼠标距标记 **≤4px** 时画提示「传送点：<行会/地图名> (x,y)」（中英双语，`Label::MinimapGateTip`），
  位置跟随鼠标并夹在屏内。测试 `movement_markers_and_tooltip_cover_gates`。
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

#### 背景音乐

* 编号 → 文件名是纯数据，在 `mir_render::sound::MUSIC_TRACKS`：`30001..30011`、
  `30101..30113`、`31001..31006` 共 30 首，对应 `SoundList.lst:583-615` 的 `;Music` 段；
  `music_for_index(30001) == Some("30001.wav")`。
* Crystal 的流程：服务器 `MapInfo.Music` → `S.MapInformation.Music` / `S.MapChanged.Music`
  → `MapControl.Music`（`GameScene.cs:2146`、`:3985`）→ `LoadMap` 发现编号变化时
  `SoundManager.PlayMusic(Music, true)`（`GameScene.cs:10454-10466`）。
  `PlayMusic` 先 `StopMusic()`，再用 `SoundList.Indexes` 查文件并循环播放
  （`SoundManager.cs:128-136`）。
* 本作窗口每帧 `sync_music`：优先 `music_for_index(map.music)`；编号为 0 时用
  `music_for_map(file_name, title)` 兜底（`0`/比奇 → `30001.wav`，`3`/盟重 →
  `30002.wav`，其它地图 → `30001.wav`）。示例数据库 389～463 张图的 `MapInfo.Music`
  全为 0，所以这个兜底是**本作对示例数据的简化**，不是 Crystal 行为。
* 播放器是单独的 `rodio::Player` + `Source::repeat_infinite()`，音量走独立的
  `music_volume`（默认 0.5，`--music-volume` / `settings.json`），与音效互不影响；
  `--no-music` 关闭。没有设备、没有 `300xx.wav`、解码失败都安静降级。
* 素材顺序：`resource/wav`（没有 300xx.wav）→
  `resource/community/mirfiles-crystal-client/Client/Sound`（完整 30 首 BGM）→
  `resource/community/gqf2008-crystal/Sound`（补音效）。

### 5.7 大地图（B）

面板**屏幕居中**，从上到下：地图名标题、568×380 的大地图视区、坐标行与 `B / Esc 关闭`。

| 项 | 本项目 | Crystal 出处 |
|---|---|---|
| 按键 | `B` 开关；`Esc` 也能关 | `GameScene.cs:773-774`（`KeybindOptions.Bigmap` → `BigMapDialog.Toggle()`）、`:668-671,697`（`Closeall`） |
| 图库 | 与小地图同一个 `resource/data/mmap.wzl` | `Client/MirGraphics/MLibrary.cs:23` `MiniMap = new MLibrary(Settings.DataPath + "MMap")` |
| 图像号 | `S.MapInformation.big_map` / `ClientMapInfo.BigMap`（0 = 没有大地图） | `BigMapDialog.cs:642`；`Shared/Data/ClientData.cs:324`；`Show()` 在 `BigMap <= 0` 时直接返回 `:289` |
| 面板位置 | 屏幕居中 | `:98` `Location = Center` |
| 标题 | 面板内 `(19, 6)`、高 20 | `:225-234` `TitleLabel` |
| 视区 | 面板内 `(14, 52)`、568×380 | `:559`、`:650-653` |
| 整图缩放 | 绘制尺寸 = `min(568, 图像宽) × min(380, 图像高)`，整图拉伸到该尺寸并居中 | `:650`、`MLibrary.cs:756-776` 的 `Draw(index, point, size, colour)` |
| 格 → 像素 | `像素 = 格 × (绘制宽 / 地图宽)` | `:661-662`（`ScaleX/ScaleY`） |
| 点位 | 复用 `ui::minimap::big_view_for` 的点位（怪物红/NPC 绿/其他玩家白），自己最后画黄色 2×2 | `:686-705` 的点位颜色（与 `MainDialogs.cs:1945-1961` 相同） |
| 输出 | `target/client-bigmap.png`（`tests/bigmap_panel.rs`） | — |

简化（原因）：

* 面板尺寸取自控件坐标反推的 **596×460**：Crystal 的对话框尺寸来自 `Title.wzl` 的
  820 号图（`:95-98`），而本仓库素材里没有 `Title.wzl`，所以用
  「视区 14 + 568 + 14 宽」与「视区 52 + 380 + 底部坐标行（`:221` 的 y = 435）」反推，
  面板背景用与其它面板一致的半透明底 + 边框；
* 没有右侧 NPC 列表/滚动条/底部按钮（世界地图、我的位置、传送、搜索，
  `:101-255,341-432`），也没有「已探明区域」遮罩——Crystal 本身也是直接画整张图；
* 坐标行显示自己的坐标；Crystal 跟随鼠标显示大地图坐标（`:603-609`、`:257-260`）；
* 点大地图在 Crystal 里会寻路（`:612-628`）；本作已有 A\* 但还没把大地图坐标映射到
  世界格，所以这里仍只把点击吃掉，避免误触发走路；
* `mmap.wzl` 缺失或 `big_map == 0` 时画纯色占位 + 地图名（`NoBigmap`），不 panic。

Crystal 的点位公式有一处疑似的 bug：整图是**缩放绘制**的（`:676`），但对象点仍减去
`startPointX/Y`（`:678-705`），而自己的点却按 `格 × 比例` 画（`:707-710`）。玩家不在
图像边缘时两者会错位；本作按 `格 × 比例` 统一投影（需求也要求「画整张图 + 自己位置点」）。

### 5.8 任务面板（Q）

`ui::quests`。数据全部来自快照：

* `S.NewQuestInfo` → `quest_infos`：登录时服务端把**全部任务定义**发一遍
  （`Server/MirObjects/PlayerObject.cs:1185`、`:1727-1731`），界面用名字/分组/限时/奖励；
* `S.ChangeQuest` → `quests`：进度文本 `task_list` 由服务端算好（`crates/mir-server/src/quests.rs`
  的 `task_list`，对应 Crystal `QuestProgressInfo.UpdateTasks`），例如「击杀 鹿 3/10」；
  `Add` 插入、`Update` 替换进度、`Remove`（交付/放弃/超时）移除；
* `S.CompleteQuest` → `completed_quests`（已交付任务编号，面板暂未展示）；
* `S.ShareQuest` → 一条系统聊天消息。

布局（`quests::quest_layout`，纯函数、绘制与命中测试共用）：

| 项 | 本项目 | Crystal 出处 |
|---|---|---|
| 按键 | `Q` | `KeybindSettings.cs:219`（`KeybindOptions.Quests` = `Keys.Q`）、`GameScene.cs:660-664` |
| 位置 | 左侧 `(x+40, y+60)`，宽 468 | `QuestDiaryDialog` 的 `Location = new Point(ScreenWidth/2-300-20, 60)`（`QuestDialogs.cs:660`） |
| 行高 | 46 px，一屏 7 行（滚轮滚动） | 任务日记按分组逐条排（`QuestDialogs.cs:716-760`） |
| 一行内容 | 名字 · 分组 / 进度文本（最多 2 行）/ 状态 + 限时剩余 / 「放弃」按钮 | `QuestSingleQuestItem` 的 `Complete`/`InProgress` 文案（`QuestDialogs.cs:1898`） |
| 完成提示 | 名字与状态都画绿色（`COMPLETE_COLOR`）+「已完成 · 去交付」 | `ClientQuestProgress.Completed` 是「目标达成」，不是「已交付」 |
| 放弃 | 按钮发 `C.AbandonQuest { quest_index }` | `AskCancelQuest` 确认框 → `C.AbandonQuest`（`QuestDialogs.cs:592`） |

简化（原因）：

* 限时剩余从**客户端第一次看到该任务**（`S.ChangeQuest{Add}`）开始算：协议里的
  `ClientQuestProgress` 不带接取时间（Crystal 的 `StartDateTime` 只在服务端
  `QuestProgressInfo` 里），重登后会重置成完整时长；
* Crystal 的任务日记按 `QuestInfo.Group` 折叠分组、另有详情页/奖励选择/共享/追踪四个控件
  （`QuestListDialog`、`QuestDetailDialog`、`QuestTrackingDialog`），本作只做需求里的
  「进度 + 放弃 + 完成高亮」；
* 「可接取任务」不做：服务端没有「可接任务列表」包，可接任务只能从 NPC 对话页里点
  （`C.AcceptQuest` 由 NPC 脚本触发），所以面板只列进行中任务。

**任务物品栏（第 4 轮新增）**：面板底部固定一条「任务物品」栏（`Label::QuestItems`），
横向显示 `WorldSnapshot.quest_items` 的物品图标（复用 `items.wzl`）+ 数量 `×N`；
空栏显示 `Label::Empty`（中文「空」/ 英文 `-`），素材缺失时画占位方块。数据来源：

* `S.GainedQuestItem`(205)（`{item: UserItem}`）→ 同 `unique_id` 整堆替换；可堆叠按模板
  堆叠上限并堆，剩余入新条目（Crystal `GameScene.AddQuestItem`，`GameScene.cs:6559-6586`）；
* `S.DeleteQuestItem`(206)（`{unique_id, count}`）→ 按 `unique_id` 扣 `count`，扣到 0 移除；
* `S.UserInformation.quest_inventory`：整栏快照，`Some` 替换、`None` 保留旧数据，
  40 格里的空格忽略；
* 聊天提示：`Label::SystemQuestItemGained`（获得任务物品 {} ×{}）/ `Label::SystemQuestItemLost`
  （失去任务物品 {} ×{}），按玩家语言渲染；
* **YouFound**：`S.SendOutputMessage{Type=Quest}`（223）显示为系统聊天行（「你找到了 X」，
  文本由服务端按玩家语言渲染，客户端不二次翻译）；
* **滚轮翻页（第 5 轮）**：鼠标悬停在「任务物品」条上滚动翻页；滚轮在面板其它区域仍是
  滚任务列表。条右侧显示「当前页/总页 · 滚轮翻页」（`Label::QuestItemsScrollHint`）。
  按面板宽度自动分页：320 宽每页 5 件（40 格 = 8 页）、800 宽每页 10 件（4 页），
  滚到末页会夹住；未占用 ←/→（方向键用于走路）。

测试：`tests/round2_panels.rs` 的 `quest_items_track_gained_and_deleted_items`、
`quest_panel_shows_quest_items_and_fits_small_screens`，截图
`target/client-quest-items-{320x240,800x600}.png`。

### 5.9 仓库面板（服务端自动打开）

`ui::storage`。流程与 Crystal 一致：

1. NPC 脚本页 `[@STORAGE]` → 服务端发 `S.UserStorage`（80 格物品列表，每个连接只发一次，
   `PlayerObject.cs:8169-8180`）→ `S.NPCStorage`（`NPCScript.cs:1002-1006`）；
2. 快照把 `storage_open` 置真、`storage_open_version` 加一，`UiState::sync` 据此自动打开
   仓库面板，并把背包面板一起打开（拖拽存取需要两侧都可见）；
3. 离开仓库页（`S.NPCResponse` 空页/换页、切图）时 `storage_open` 置假，面板收起。

| 项 | 本项目 | Crystal 出处 |
|---|---|---|
| 格数 | 80（10 列 × 8 行，一屏画完） | `Globals.StorageGridSize = 80`（`Globals.cs:19`）、`AccountInfo.Storage = new UserItem[80]`（`AccountInfo.cs:69`） |
| 布局 | 10 列 × 8 行 | Crystal 是 `StorageDialog.Grid = new MirItemCell[10 * 16]`（`NPCDialogs.cs:2935`）里前 80 格、按页滚动 |
| 存 | 背包格 → 仓库格：`C.StoreItem { from, to }` | `PlayerObject.StoreItem`（`PlayerObject.cs:5376-5456`） |
| 取 | 仓库格 → 背包格：`C.TakeBackItem { from, to }` | `PlayerObject.TakeBackItem`（`:5457-5526`） |
| 回执 | `S.StoreItem` / `S.TakeBackItem` 的 `Success` 把物品搬进快照 | 同左 |
| 容量 | 标题右侧「容量 已用/80」 | Crystal 没有容量文案（格子数就是容量） |
| 金币 | **没有**（只显示物品） | 个人仓库没有金币功能；金币仓库是行会的（§5.11） |

简化：没有扩充仓库（`HasExpandedStorage` 恒假）、没有仓库密码（服务端恒不要求）、
没有滚动（80 格一屏放得下）；仓库内部没有「移动」包（Crystal 也只有存/取两条路径）；
点在仓库面板空白处只吞掉点击，不会穿透成走路。`Esc` 关掉面板后，重新点 NPC 的仓库选项
会让服务端再发一条 `S.NPCStorage`（`S.UserStorage` 每个连接只发一次），面板会再次打开。

### 5.10 邮件面板（N）

`ui::mail`。两个页签：

**收件箱**（`S.ReceiveMail` 的 `ClientMail` 列表）：

| 项 | 本项目 | Crystal 出处 |
|---|---|---|
| 列表 | 10 行一页 + 上一页/下一页 | `MailListDialog.Rows = new MailItemRow[10]`、`PreviousButton/NextButton`（`MailDialogs.cs:26`、`:143-215`） |
| 列 | 类型图标（金=带附件、灰=普通信）/ 发件人 / 主题 / 日期 | `TitleTypeLabel/TitleSenderLabel/TitleMessageLabel`（`MailDialogs.cs:40-76`） |
| 阅读 | 点一行 → `C.ReadMail { mail_id }`，正文与附件显示在下方 | `MailReadParcelDialog.ReadMail` → `C.ReadMail`（`MailDialogs.cs:1098`） |
| 收取附件 | `C.CollectParcel { mail_id }`（带附件且未领取才可用） | `:1189` |
| 锁定/解锁 | `C.LockMail { mail_id, lock }` | `:1074` |
| 删除 | `C.DeleteMail { mail_id }`（锁定/未领取时禁用） | `:1051` |

**写信**：收件人 / 主题 / 正文 / 金额 / 贴邮票 / 5 个附件格 + 面板内的背包挑选格 + 邮费。

* 「发送」发 `C.SendMail { name, message, gold, items_idx[5], stamped }`；
* 「询价」发 `C.MailCost { gold, items_idx, stamped }`，服务端回 `S.MailCost { cost }`；
* 「贴邮票」切换 `Stamped`（服务端：贴票免邮费且可带 5 件，未贴只认第 1 件，
  `PlayerObject.cs:11790`）；
* 服务端 `S.MailSendRequest`（NPC 邮筒页）到达时自动打开写信页；
* `S.MailSent` / `S.ParcelCollected` 的结果记录在快照里（`mail_sent_result` /
  `parcel_collected_result`），`S.MailLockedItem` 记附件格的锁定状态。

简化（原因，都在 `ui/mail.rs` 模块文档里）：

* **主题**：Crystal 的 `ClientMail` 没有主题字段（`Shared/Data/ClientData.cs:154`），
  服务端 `mail` 表的 `subject` 也不上线。列表的「主题」列显示正文首行；写信页的主题
  输入框只本地记录，发送时忽略；
* 输入框是单行（Crystal 是多行 `MirTextBox`），不处理 IME 组合中间态；
* 附件用「点背包格切换」而不是从背包拖拽（Crystal 是拖 `MirItemCell`，`MailDialogs.cs:889`）；
  未贴邮票时界面仍允许挂 5 件，但服务端只认第 1 件（`PlayerObject.cs:11790`）；
* 没有「回复 / 黑名单 / 举报」按钮；邮件类型用色块代替 `Title.wzl` 图库图标；
* 日期按 `MM-DD HH:MM`（UTC）显示，Crystal 用「今天/昨天/N 天前」文案。

### 5.11 行会面板（G）

`ui::guild`。**五个页签**（第 8 轮加「联盟」；对应 Crystal 的左右两栏按钮）：

| 页签 | 数据 | 动作 |
|---|---|---|
| 成员 | `S.GuildStatus` + `S.GuildMemberChange{Status=255}` | 选中成员；升职/降职 `C.EditGuildMember{ChangeType=2}`、踢人 `{ChangeType=1}`（按 `GuildStatus.MyOptions` 的权限位置灰） |
| 仓库 | `S.GuildStorageList{Items[112]}`、`S.GuildStorageGoldChange` | 拖拽存取/移动 `C.GuildStorageItemChange{Type=0/1/2}`；存入/取出资金 `C.GuildStorageGoldChange{Type=0/1}` |
| 增益 | `S.GuildBuffList` 的 `GuildBuffs`（可选）与 `ActiveBuffs`（已获得） | 「购买」（`Action=1`，未获得）/「激活」（`Action=2`，已获得但过期）；已激活时置灰 |
| 联盟 | 无独立协议（服务端 `/ally` 聊天命令） | 输入行会名/ID → 三个按钮「申请 / 同意 / 解除」，分别发送 `/ally request <name>`、`/ally accept`、`/ally remove <name>` |
| 公告 | `S.GuildNoticeChange` | 输入一行并「保存」→ `C.EditGuildNotice { notice: [行] }`（需要 `CanChangeNotice`） |

打开面板（`G`）时自动发四条请求（Crystal `GuildDialog.Show()` 的拉取）：
`C.RequestGuildInfo{Type=0}`（公告）、`{Type=1}`（成员表）、
`C.GuildStorageItemChange{Type=3}`（仓库列表）、`C.GuildBuffUpdate{Action=0}`（增益列表）。

成员状态码按 Crystal 的 `GameScene.GuildMemberChange`（`GameScene.cs:5813-5868`）处理：
`0` 下线、`1` 上线（+行会频道消息）、`2` 新成员、`3/4` 被踢/退会、`5` 职位调动通知、
`6/7` 职位新增/修改、`8` 我的职位变了（同时更新 `GuildStatus` 里的权限）、`255` 完整成员表。

简化（原因）：

* 成员表「地图」列**恒为空**：协议 `GuildMember` 只有名字/编号/最后登录/投票/在线
  （`Shared/Data/GuildData.cs:62-90`），没有所在地图；Crystal 的成员页也只有
  名字/职位/在线状态。这一列按需求保留，显示 `--`；
* 职位用「升职 / 降职」（职位序号 ±1）而不是 Crystal 的下拉框（`MirDropDownBox`）；
* 没有「新建职位 / 职位权限编辑 / 邀请入会」控件（服务端已实现
  `C.EditGuildMember{ChangeType=0/3/4/5}`，界面未接，见 §8）；
* 行会仓库 112 格一屏画完（Crystal 是 8 列 × 14 行、一页 8 行滚动）；
* 面板固定靠左 500 px：本作的面板不可拖动，不靠左会把右侧背包挡住，
  「背包 → 行会仓库」的拖拽就点不到；
* 资金存取没有 Crystal 的 `MirAmountBox` 弹窗，直接在面板里输入金额。

### 5.12 面板 ↔ 包对照表

| 面板 / 功能 | 客户端包 → 服务端包 | 服务端实现 | 快照字段 |
|---|---|---|---|
| 任务列表 | ← `S.NewQuestInfo` / `S.ChangeQuest` / `S.CompleteQuest` / `S.ShareQuest` | `quests.rs` | `quest_infos` / `quests` / `completed_quests` |
| 任务物品栏 | ← `S.GainedQuestItem{Item}`（205）/ `S.DeleteQuestItem{UniqueID,Count}`（206）/ `S.UserInformation.quest_inventory` | `quest_inventory.rs` | `quest_items` / `quest_items_version` |
| 宠物模式 | `C.ChangePMode{Mode}`（45）→ `S.ChangePMode{Mode}`（63） | `World::set_pet_mode` / `pet_mode`（`world.rs`） | `pet_mode` |
| 浮动输出 | ← `S.SendOutputMessage{Message,Type}`（223）：`Quest` → 系统色、`Guild` → 行会频道、`Normal` → 提示色 | 服务端系统消息（如 YouFound、行会/公告） | `chat` |
| 放弃任务 | `C.AbandonQuest{QuestIndex}` → `S.ChangeQuest{Remove}` | `World::abandon_quest`（`quests.rs`） | `quests` 移除 |
| 仓库打开 | ← `S.UserStorage{Storage[80]}` + `S.NPCStorage` | `storage.rs` | `storage` / `storage_open` / `storage_open_version` |
| 存物品 | `C.StoreItem{From,To}` → `S.StoreItem{From,To,Success}` | `World::storage_store_item` | 背包 ↔ `storage` |
| 取物品 | `C.TakeBackItem{From,To}` → `S.TakeBackItem{From,To,Success}` | `World::storage_take_back_item` | 背包 ↔ `storage` |
| 邮件列表 | ← `S.ReceiveMail{Mail[]}` | `mail.rs`（`mail_list_packet`） | `mails` |
| 阅读 | `C.ReadMail{MailID}` → `S.ReceiveMail` | `World::mail_read` | `mails` |
| 领取附件 | `C.CollectParcel{MailID}` → `S.GainedItem`/`S.GainedGold` + `S.ParcelCollected` + `S.ReceiveMail` | `World::mail_collect` | `parcel_collected_result` |
| 锁定 / 删除 | `C.LockMail` / `C.DeleteMail` → `S.ReceiveMail` | `World::mail_lock` / `mail_delete` | `mails` |
| 写信页 | ← `S.MailSendRequest`；`C.MailCost{Gold,ItemsIdx,Stamped}` → `S.MailCost{Cost}` | `World::mail_open_send_page` / `mail_cost` | `mail_send_request` / `mail_cost` |
| 发送 | `C.SendMail{Name,Message,Gold,ItemsIdx[5],Stamped}` → `S.DeleteItem`×N + `S.LoseGold` + `S.MailSent` | `World::mail_send` | `mail_sent_result` |
| 附件格锁定 | `C.MailLockedItem{UniqueID,Locked}` → `S.MailLockedItem` | `MirConnection` 回显 | `mail_locked_items` |
| 行会概况 | ← `S.GuildStatus` | `guild.rs`（`Guild::status`） | `guild` |
| 成员表 | `C.RequestGuildInfo{Type=1}` → `S.GuildMemberChange{Status=255,Ranks}`；增量 `Status=0..8` | `World::guild_request_info` | `guild_ranks` |
| 升/降职/踢人 | `C.EditGuildMember{ChangeType,RankIndex,Name,RankName}` → `S.GuildMemberChange` | `World::guild_edit_member` | `guild_ranks` |
| 公告 | `C.RequestGuildInfo{Type=0}` → `S.GuildNoticeChange`；`C.EditGuildNotice{Notice[]}` | `World::guild_request_info` / `guild_edit_notice` | `guild_notice` |
| 行会仓库 | `C.GuildStorageItemChange{Type=3}` → `S.GuildStorageList{Items[112]}`；`{Type=0/1/2}` → `S.GuildStorageItemChange` | `guild_storage.rs` | `guild_storage` |
| 行会资金 | `C.GuildStorageGoldChange{Type,Amount}` → `S.LoseGold`/`S.GainedGold` + `S.GuildStorageGoldChange{Amount,Type,Name}` | `World::guild_storage_gold_change` | `guild.gold` |
| 行会增益 | `C.GuildBuffUpdate{Action=0}` → `S.GuildBuffList`；`{Action=1/2,Id}` → `S.GuildBuffList`（+ 资金变化） | `guild_buffs.rs` | `guild_buff_infos` / `guild_active_buffs` |
| 联盟页签 | 无独立请求包：`C.Chat{"/ally request|accept|remove …"}` → `S.Chat`（TextKey `ally_*`）；名单 ← `S.GuildAllies`(303) | `guild.rs::Guild::…` + `session.rs` 聊天命令 | `chat` / `guild_allies` |
| 好友/黑名单 | `C.AddFriend{Name,Blocked}`(127)、`C.RemoveFriend{CharacterIndex}`(128)、`C.RefreshFriends`(129)、`C.AddMemo{CharacterIndex,Memo}`(130) → `S.FriendUpdate{Friends}`(245) | `friends.rs`（`World::add_friend`/`remove_friend`/`set_friend_memo`/`send_friends`） | `friends` |
| 复活广播 | ← `S.ObjectRevived{effect=true}`（旁观 `S.ObjectRemove` → `S.ObjectRevived` → `S.ObjectAppearance`）；本人另收 `S.Revived` | `World::revive_player`（`world.rs`） | 实体动作/名称颜色 |
| 攻城状态 | ← `S.ConquestInfo`(300) / `S.ConquestResult`(302) | `conquest.rs`（本作扩展包） | `conquest` |
| 攻城器械 | ← `S.ConquestObject`(301) | `conquest.rs::spawn_conquest_objects` / `damage_conquest_object` | `conquest.objects` |
| 交易邀请 | ← `S.TradeRequest{Name}` | `World::trade_request` | `trade.invitation` / `invitation_version` |
| 接受/拒绝交易 | `C.TradeReply{AcceptInvite}` → `S.TradeAccept{Name}`（双方各一条） | `World::trade_reply` | `trade.open` / `trade.partner` / `open_version` |
| 放入 / 取回 | `C.DepositTradeItem{From,To}` / `C.RetrieveTradeItem{From,To}` **或** `C.MoveItem{Grid=Trade}` → `S.DepositTradeItem` / `S.RetrieveTradeItem`（含 `Success`）；对方收 `S.TradeItem{TradeItems[10]}` | `World::trade_deposit_item` / `trade_retrieve_item` / `move_item`（Trade 分支） | `trade.items` / `trade.partner_items` / `trade.version` |
| 交易金币 | `C.TradeGold{Amount}`（增量）→ 自己 `S.LoseGold`、对方 `S.TradeGold{Amount=累计}` | `World::trade_gold` | `trade.gold` / `trade.partner_gold` |
| 准备/确认 | `C.TradeConfirm{Locked}` → 双方都锁定时互换物品 + `S.TradeConfirm{}`（空包） | `World::trade_confirm` | `trade.locked` / `trade.partner_ready` |
| 取消 | `C.TradeCancel{}` → `S.TradeCancel{Unlock=false}`；双方都已锁定时校验失败发 `{Unlock=true}` | `World::trade_cancel` | `trade.open` / `trade.version` |
| 打开市场 | ← `S.NPCMarket{Listings,Pages,UserMode}` | `market.rs`（`World::open_market`） | `market.listings` / `market.pages` |
| 搜索 / 刷新 | `C.MarketSearch{Match,Type,UserMode,Min/MaxShape,MarketType}` / `C.MarketRefresh{}` → `S.NPCMarket` | `World::market_search` / `market_refresh` | `market.listings` / `market.pages` |
| 翻页 | `C.MarketPage{Page}` → `S.NPCMarketPage{Listings}` | `World::market_page` | `market.listings`（追加） |
| 买断 / 出价 | `C.MarketBuy{AuctionID,BidPrice}` → `S.LoseGold` / `S.GainedItem` / `S.MarketSuccess` / `S.MarketFail` | `World::market_buy` | `market.message` / `market.fail` |
| 提前成交 | `C.MarketSellNow{AuctionID}` → `S.MarketSuccess` / `S.MarketFail` | `World::market_sell_now` | `market.message` |
| 取回 / 收款 | `C.MarketGetBack{Mode,AuctionID}` → `S.GainedItem` / `S.GainedGold` / `S.MarketSuccess` / `S.MarketFail` | `World::market_get_back` | `market.listings` / `market.message` |
| 上架 | `C.ConsignItem{UniqueID,Price,Type}` → `S.ConsignItem{UniqueID,Success}` + `S.LoseGold` | `World::consign_item` | `market.consign_result` |
| 商城页签 | `C.MarketSearch{MarketType=GameShop}` → `S.NPCMarket`；`C.GameshopBuy{GIndex,Quantity,PType}` → `S.LoseCredit`/`S.LoseGold` + `S.GainedItem`（背包满转邮件 + `S.Chat`） | `World::gameshop_buy`（`market.rs`）、`session.rs::on_gameshop_buy` | `market.listings` / `market.message` / `self_player.credit` |
| 设置面板 | 改 `settings.json`（音效/音乐/语言/分辨率/缩放）；**客户端界面**语言即时生效，并代发 `/language <code>`（`NetCommand::Chat`）让服务端聊天/邮件/市场回执一起切换 | `mir-client/src/settings.rs` + `ui/settings.rs` + `app.rs` 的 `Action::SetLanguage` | `settings`（`app.rs` 持有） |
| 登录 / 选服 / 选角 | `C.ClientVersion` → `C.Login` → `S.LoginSuccess{Characters}`；`C.StartGame{CharacterIndex}` / `C.NewCharacter` | `net.rs`（`manual_select`）+ `ui/select.rs` | `login_stage` / `characters` |
| 技能表现 | ← `S.Magic` / `S.ObjectMagic` / `S.ObjectSpell` / `S.ObjectRemove` / `S.ObjectHidden` / `S.Poisoned` / `S.ObjectPoisoned` | `magic.rs`（§6.2-§6.7） | `spell_events` |
| 组队开关 | `C.SwitchGroup{AllowGroup}` → `S.SwitchGroup{AllowGroup}` | `group.rs`（`World::switch_group`） | `group.allow_group` |
| 邀请入队 | `C.AddMember{Name}` → 目标 `S.GroupInvite{Name}`；接受后双方 `S.AddMember` | `World::group_invite` | `group.members` |
| 接受/拒绝组队 | `C.GroupInvite{AcceptInvite}` → `S.AddMember` × N + `S.GroupMembersMap` + `S.SendMemberLocation` + `S.ObjectHealth` | `World::group_join` | `group.members`（含地图/坐标） |
| 退队 / 踢人 | `C.DelMember{Name}` → `S.DeleteMember` / `S.DeleteGroup` | `World::group_kick` / `leave_group` | `group.members` |
| 组队聊天 | `C.Chat{"!!…"}` → `S.ObjectChat{Type=Group}` 只发队员 | `World::group_chat` | `chat`（`ChatType::Group`） |

### 5.13 交易面板（T / Y）

`ui::trade`。一个面板同时画两栏（Crystal 是 `TradeDialog` + `GuestTradeDialog` 两个窗口）：

| 区域 | 内容 | 交互 |
|---|---|---|
| 标题 | 「交易（对方：名字）」 | — |
| 左栏 | 自己的 5×2 格、金币、金币输入框 + 「放入金币」 | 从背包拖入 → `C.DepositTradeItem`；点自己的格子 → `C.RetrieveTradeItem` |
| 右栏 | 对方的 5×2 格、对方金币、准备状态 | 只读（服务端 `S.TradeItem` / `S.TradeGold` 更新） |
| 底部 | 「准备 / 确认」「取消交易」 | `C.TradeConfirm{Locked=true}` / `C.TradeCancel` |

要点：

* 打开方式：对方发起后收到 `S.TradeRequest`，聊天区提示「某某 请求与你交易」；
  按 `T` 或点聊天里的邀请接受/拒绝（`C.TradeReply`）；`Y` 是 Crystal 的发起键，
  本作 `T` 已用于聊天，所以发起固定为 `Y`（见 §4 键位表）。
* **第 8 轮补齐**：`C.MoveItem{Grid=Trade}` 现支持交易格（`items.rs::move_item` 分支），
  所以「拖背包进交易格」「交易格拖回背包」也可走 `MoveItem`；协议无方向字段，
  服务端按 `from` 格在背包有无物品判断存入/取回（成功 `S.DepositTradeItem`/
  `S.RetrieveTradeItem`，对方始终收整份 `S.TradeItem`）。
* 面板打开时 `pointer_over_panel` 会拦截世界点击，不会误走路；`Esc` 发 `C.TradeCancel`。
* 只用一个「准备/确认」按钮：未准备时显示「准备」、已准备显示「确认」，两次点击都发
  `C.TradeConfirm{Locked=true}`，服务端幂等。
* 对方状态行（未准备/已准备）是**本作扩展**：Crystal 没有这个 UI 元素，但
  `S.TradeConfirm{Unlock=true}`/`S.TradeItem` 已足以推导。
* 金币没有 `MirAmountBox` 弹窗，直接输入数字 + 「放入金币」；协议是增量
  `C.TradeGold{Amount}`，对方看到的是累计额。
* 成交后服务端发 `S.TradeConfirm`（空包）关窗；`S.TradeCancel{Unlock=true}` 只解锁，
  `{Unlock=false}` 才关窗并退货。

测试：`crates/mir-client/tests/trade_panel.rs`
（`trade_panel_accepts_items_gold_and_confirm`、`trade_layouts_survive_small_windows`）。

### 5.14 市场 / 寄售 / 拍卖面板

`ui::market`。四个页签对应协议的 `MarketPanelType`：

| 页签 | `C.MarketSearch.UserMode` | 列表 | 主按钮 |
|---|---|---|---|
| 市场 | `false` | 全服在售（寄售 + 拍卖） | 寄售「买下」/ 拍卖「出价」→ `C.MarketBuy` |
| 寄售 | `true` | 自己挂的寄售 | 「取回」→ `C.MarketGetBack{Any}` |
| 拍卖 | `true` | 自己挂的拍卖 | 「取回」；有人出价时「提前成交」→ `C.MarketSellNow` |
| 商城 | `false` | 服务端商城目录（`item_type = GameShop`） | 「购买」→ `C.GameshopBuy`（元宝，见 §5.16） |

* 打开：NPC 的 `[@MARKET]` 页 → `S.NPCMarket` 打开面板；`[@CONSIGN]` → `S.NPCConsign`
  直接打开「寄售」页签（`ui::market` 的 `consign_open_version`）。
* 搜索框 + 「搜索」「刷新」：`C.MarketSearch` 的 `Match/Type/UserMode/MinShape/MaxShape/
  MarketType` 来自当前页签与输入框；`C.MarketRefresh` 走服务端上次关键字。
* 翻页：客户端页号 0 起、协议 `C.MarketPage.Page` 从 0 开始；「上一页 / 下一页」只请求
  尚未取过的页（`ui::market::max_scroll` / `MarketView::append_page`）。
* 列表列：物品（图标 + 名字）、价格、卖家/状态、到期；`ClientAuction.seller` 是**状态标签**
  （出售中/已售出/已过期/无人出价/已有出价），不是玩家名（Crystal `GetSellerLabel`）。
* 失败码 `S.MarketFail.Reason` 0..=10 全部本地化：0 死亡、1 未与商人对话、2 已售出、
  3 已过期、4 金币不足、5 背包已满、6 不能买自己的、7 商人太远/条目不存在、
  8 金币超上限、9 出价太低、10 拍卖已结束；未知编号回「操作失败」。
* `S.MarketSuccess.Message` 是服务端文案，直接显示在面板底部绿字 + 聊天系统行。
* **拍卖到期自动成交**：成交/过期的提示由服务端下发的系统聊天 + 邮件承载
  （`market_bought` / `market_sold_now` / `market_mail_auction_won` /
  `market_mail_sold_commission`），客户端原样显示（聊天系统行 + 邮件面板），
  本轮未新增 Market 相关 `Label`；服务端语义见 `docs/world/market.md` §9。
* 寄售 / 拍卖页签有「上架」表单：背包选中物品 + 价格 + 上架 → `C.ConsignItem`；
  价格输入框按页签校验（寄售 5000..=50,000,000，拍卖 0..=50,000）。
* 实际行为：市场面板打开时会把背包一起显示（Crystal `TrustMerchantDialog.Show()` 同样会
  摆出背包），**不会主动关闭 NPC 对话/商店**；两个面板可以同时存在。面板不可拖动、
  固定在屏幕中央偏上。商城页签（`GameShop`）第 2 轮已实现，见 §5.16。

测试：`crates/mir-client/tests/market_panel.rs`（布局/快照/失败码/搜索翻页/购买出价/上架/
商城页签/截图，见文件内用例）。

### 5.15 设置面板（O / F12）

`ui::settings`（Crystal `OptionDialog`，`Client/MirScenes/Dialogs/OptionDialog.cs`）。
**`O` 打开、`Esc` 关闭**（`F12` 备用）。四组：

| 分组 | 控件 | 行为 |
|---|---|---|
| 音频 | **主音量 / 背景音乐 / 音效三条 0~100 滑杆**（左右 −/+ 每次 10） | 写入 `client.toml` 的 `master_volume` / `music_volume` / `sound_volume`，下一次播放立即生效 |
| 语言 | 简体中文 / English | 点击**即时重绘**并写盘，同时发 `/language` 给服务端 |
| 显示 | 分辨率 4 档、缩放 **1x/2x/3x**、窗口模式按钮（Windowed/Fullscreen/Borderless，点击循环并调 winit `set_fullscreen`）、**小地图传送点开关** | 写 `client.toml`；缩放按整数档降采样再放大（`frame.rs`） |
| 键位 | **6 行可重绑**（攻击 Space / 魔法 F1 / 背包 I / 组队 P / 好友 F / 设置 O） | 点行尾按钮进入捕获，下一次按键绑定；**冲突时 footer 红字「键位冲突：X 已被 Y 使用」并保留原键** |

**持久化**：新模块 `client_config.rs`，自写 TOML 子集解析/序列化（**零新增依赖**）；
字段 `language/master_volume/sound_volume/music_volume/window_mode/scale/show_minimap_gates`
+ `[keys]` 6 项。缺失 → `ConfigIssue::Missing` 回落默认并在启动时生成默认文件；坏文件 →
`Invalid` 回落默认 + `eprintln!` 提示，**不 panic**；未知键/类型错误/非法缩放同样报错回落。
每次设置动作后保存 `config/client.toml`；旧 `settings.json` 仍保留兼容。

测试 5 例：默认值、TOML+磁盘往返、缺失/坏文件回落、键位冲突拒绝、语言归一化。
截图 `target/client-settings.png`（800×600）。

> 说明：缩放沿用 **1x/2x/3x 整数档**；1.5x 受整数缩放限制本轮未加（如需可下轮补分数缩放）。

### 5.16 商城页签（市场面板第四个页签）

`ui::market` 的 `TABS` 第 4 项是 `MarketPanelType::GameShop`（Crystal 的
`GameShopButton`，`TrustMerchantDialog.cs:143-155`）：

* 打开：点击页签直接发 `C.MarketSearch{MarketType=GameShop, Match:"", Type:Nothing}`
  （**不要求 NPC 对话**，服务端商城分支跳过 NPC 校验）→ `S.NPCMarket`；翻页
  `C.MarketPage` → `S.NPCMarketPage`；`C.MarketRefresh` 重新拉首页。搜索/刷新/翻页
  都受服务端 500ms 节流（`S.MarketFail{TooFar}` 或静默丢弃）。
* 列表：`ClientAuction.seller` 为空串，客户端显示「商城」；价格单位是**元宝**
  （如 `250 元宝`），标题栏余额也显示 `self_player.credit`（`S.UserInformation.Credit`
  初始化，`S.LoseCredit`/`S.GainedCredit` 更新）；表头换成「物品 / 价格 / 分类 / 库存」，
  库存列当前显示「在售」占位——`ClientAuction` 不带库存字段，页签这条路径仍然拿不到实时库存
  （见 `docs/world/gameshop.md` §7.1）。**真实库存只有 R50 新增的独立商城面板（§5.16.1）有**
  （`S.GameShopInfo.StockLevel` + `S.GameShopStock`），两套 UI 的库存显示口径不同，
  不能互相替代。
* 购买：数量输入框 +「购买」→ `C.GameshopBuy{GIndex = auction_id, Quantity, PType}`；
  页签固定用**元宝**（`PType = 0`），数量按「不超过 5 组」在客户端先夹一次
  （例如堆叠上限 10、每份 2 个时，输入 50 会被夹到 25），服务端再独立校验。
  成功时服务端发 `S.LoseCredit` + `S.GainedItem`（背包放不下转邮件 + `S.Chat`），
  页签没有 `S.MarketSuccess` 回执。
* 失败：静默类（数量/余额/支付方式）无提示；其余走 `S.Chat`（页签没有 `S.MarketFail`）。
  若走兼容路径 `C.MarketBuy`（旧客户端），失败回 `S.MarketFail`，成功回 `S.MarketSuccess`。
* 页签不显示「上架 / 取回 / 回收」按钮（商城没有卖家）。

测试：`crates/mir-client/tests/market_panel.rs`（市场页签）与 `tests/round2_panels.rs`
（`game_shop_tab_sends_market_search_and_buys_with_credits`、
`game_shop_tab_fits_small_and_large_screens`、`credit_balance_updates_from_lose_credit`）。
注意 `round2_panels` 在**没有覆盖 `TEMP`/`TMP`** 的环境里有 1 条失败
（`settings_roundtrip_through_disk_survives_restart`，`tests/round2_panels.rs:198`，
`std::env::temp_dir()` 写权限），与商城改动无关；显式覆盖后有 28 passed
（`target/round50/r50-c/c3-round2-panels-tmp-override.log`）。

### 5.16.1 独立商城面板（R50-C，`K` 键）

Crystal `GameshopDialog` 风格的**独立面板**，与 §5.16 的市场页签并存、互不替代：
页签走 `S.NPCMarket` 列表，独立面板走 `S.GameShopInfo` 目录 + `S.GameShopStock` 库存。
历史取证基线为 `HEAD = origin/main = 716db42`（2026-10-25），R50 已完成且为 **PASS-WITH-PARTIAL**；
文档批 `7802eb9`、功能批 `13f7c78` 是历史提交。R51 T1 **BLOCK** 保留为当时审计结论，后续返工后当前终态已为 **Done / PASS-WITH-PARTIAL**，见 [R51 终审 v4](testing/round51/round51-final-audit-v4.md)、[尚书验收](process/round51-shangshu-acceptance.md)及[门下确认](process/round51-menxia-confirm.md)。R52 仅进入文档/计划阶段，不能引用 R51 门禁作为 R52 功能完成证据。
历史提交与残余保持 [R50 验收](process/round50-shangshu-acceptance.md)及 [T2](testing/round50/round50-final-audit.md#L66)口径，P02–P08/P10 与 P12 历史取证边界继续保留。
`S.GameShopInfo`（Index 250）字段为 `Item: GameShopItem`（含 `ItemInfo`）、`StockLevel: i32`；
`S.GameShopStock`（Index 251）字段为 `GIndex: i32`、`StockLevel: i32`，均按此顺序编码且不压缩。
`GIndex` 是商品 Index，区别于物品模板 `ItemIndex` / `Info.Index`；详见
[商城协议定义与包链](world/gameshop.md#L349)。登录在 `UserInformation` 与物品模板之后逐商品下发目录；
购买的扣款/发货与事务提交成功后才发限量商品库存刷新，校验或事务失败均不发新的 Stock 包。
实现：`crates/mir-client/src/ui/gameshop.rs`（2,256 行，2026-10-25 实测）、`ui/mod.rs`
（`UiState::show_gameshop` / `toggle_gameshop` / `gameshop`）、`app.rs`（`K` 键 +
`live_gameshop_hit` 鼠标分发）、`net.rs`（`GameShopState`）。
证据：`target/round50/r50-c/`（`c1-lib-gameshop.log`（15 passed）、
`c2-gameshop-panel.log`（10 passed）、`k-hotkey-evidence.txt`、4 张面板截图；
该目录**没有** README，以日志原文为准）。

**布局与交互**（`680×500` 默认、最小 `300×180`；超过 8 条自动分页，4 列 × 2 行）：

* 顶部：标题「商城」+ 元宝/金币余额 + 关闭 `×`；工具栏：搜索框（24 字符、空串不筛）、
  职业下拉（全部/战士/法师/道士/刺客/弓手）、分节页签（全部/置顶/促销/新品）、
  分类下拉（取值来自商品 `Category` 字符串）。
* 列表格：图标 + 名称 + 单价（元宝或金币，随支付方式切换）+ 库存
  （`>= 99` 显示 `99+`、不限量显示 `∞`、售罄显示「售罄」灰字并禁用购买）。
* 底部：页码 `‹ 1/N ›`、滚轮翻页、数量输入（`1..=99`）、元宝/金币两个购买按钮。
* 空态：「商城暂无商品」；断线：面板只在 `Mode::Live` 可开，断线后画「未连接」态。

**`K` 键规则**（证据 `target/round50/r50-c/k-hotkey-evidence.txt`）：

* `K` 是**固定键**，不进改键表（`client_config::KEY_ACTIONS` 仍是 6 个动作）；
  改键与 `RequireAlt` 语义留 R51。
* 只在 `Mode::Live` 生效：`app.rs:2853`（`@2a7e77e3`+R50 工作树）`handle_live_key`，
  `"k"` 分支在 `app.rs:2951-2953`：`"k" => { self.live_toggle_gameshop(); return; }`。
* 文本输入优先：`app.rs:2920` 的 `if text_handled && text_consumed_key { …; return; }`
  **早于** `2951`，聊天/搜索框里输入 `k` 不会弹面板；焦点非空时 `handle_live_key`
  与 `toggle_gameshop` 两处各自早退。
* 同一时刻只开一个面板：`toggle_gameshop` 先关掉其他工具面板，再把商城面板置为
  「打开 + 焦点在列表」，再次按 `K` 关闭并清焦点。

> 行号漂移：以上 `app.rs` 行号是 `k-hotkey-evidence.txt` 的**取证快照值**；
> 2026-10-25 06:2x 复核当前工作树，`"k" =>` 已在 `app.rs:2955`、`handle_live_key` 在 `:2857`、
> `live_toggle_gameshop` 在 `:2268`。引用时按内容 grep（`"k" =>` / `fn live_toggle_gameshop`），
> 不要拿行号当永久事实。

**`GameShopState`（`net.rs`）与 `GIndex` 幂等**：

* 目录包 `S.GameShopInfo`：`upsert` 按 `item.g_index` 建键——同一 `GIndex` 的重复/迟到
  目录包**覆盖旧条目**，不追加、不复制。
* 目录与库存刷新统一使用三分语义：`item.stock == 0` → 不限量，显示 `∞`；
  `item.stock > 0 && stock_level > 0` → 显示剩余量；
  `item.stock > 0 && stock_level <= 0` → 售罄，保留条目并禁购。
  `i_stock` 仅选择个人/全服计数模式，不参与不限量判定。
* **P11 已闭环（R50-C2 → T2，修复提交 `2ef4285`）**：`b3be4af` 初版曾把初始耗尽目录
  误判为无限库存；[C2 红绿证据](../target/round50/r50-c2/README.md#L71)记录 lib 15/15、面板 13/13、
  exit 0，覆盖首次限量耗尽、真正不限量、重复目录刷新归零；[T2 复审](testing/round50/round50-final-audit.md#L38)
  与 [R50 验收](process/round50-shangshu-acceptance.md#L44)确认修复成立。
  非法负总量防御仍按源码 `<= 0`，保留其与正常商品 `== 0` 口径的差异。
* 库存包 `S.GameShopStock`：`apply_stock` 按 `GIndex` 找到条目后写 `stock_level`，
  再按上述规则判定售罄；**找不到 `GIndex`** 时只推进版本号、
  **不凭空造条目**（防止库存包晚于目录包/错包导致幽灵商品）。
* 断线/重连：世界快照重建时整个 `GameShopState` 清零，等登录链重发目录包；
  商城面板不在 `Mode::Live` 时不画列表。
* 与市场页签状态**完全隔离**（页、筛选、选中互不影响，
  测试 `market_tab_state_is_isolated_from_gameshop`）。

**缺 manifest 占位规则**（fail-closed，不伪造图标）：图标取值顺序是
①包内 `ItemInfo.image`（非 0 直接用）→②`docs/assets/gameshop-manifest.json` 的
`gindex → icon_image`→③都没有则返回 0，面板画占位方块并置 `assets_partial`。
manifest 不存在、或条目 `"partial": true`，都按「缺图标」处理、**不借用近似图标**；
授权与哈希口径见 [`assets/licensing.md`](assets/licensing.md) §5。

测试：`crates/mir-client/tests/gameshop_panel.rs`（**10 passed**，含
`wire_packet_chain_catalog_gindex_idempotent_and_stock_refresh`、
`pure_helpers_match_crystal_buy_and_quantity_rules`、`paging_and_wheel_follow_8_cells`、
`panel_draws_empty_disconnected_and_png_evidence`、`k_hotkey_wiring_evidence`）。
**PARTIAL**：只有单次截图取证，没有像素级回归基线；release 包真机往返未测。

### 5.17 服务器选择与角色选择（`--login`）

`ui::select`（Crystal `LoginScene` / `SelectScene`）：

```text
--login 启动
  → 登录页：服务器地址（默认 127.0.0.1:7000，可用 --server 改并写回设置）
            + 账号 + 口令 → 「连接」
  → S.LoginSuccess{Characters}：进入选角页（角色卡：名字/职业/等级/性别/上次登录/装备摘要）
  → 选中角色「进入游戏」→ C.StartGame{CharacterIndex}
  → 「新建角色」：名字 + 职业 + 性别 → C.NewCharacter → 回到选角页
```

* 登录失败留在登录页并给出可重试提示（连接失败/账号不存在/口令错误/封禁，
  见 `select::error_label`）；`ConnectState::Connecting` 时按钮变灰、超时 15 秒。
* 服务器地址与装备摘要缓存在 `settings.json`（`server` / `equipment`）；
  选角页的装备摘要来自上次进世界时的 `S.UserInformation.Equipment`，没有缓存时显示「暂无装备」
  （`SelectInfo` 协议里没有装备字段）。
* 命令行 `--connect` 仍走「自动选角」（`ConnectOptions::manual_select = false`）；
  `--login` 用 `manual_select = true`，登录成功后停在选角页等 UI 命令。
* 空角色列表时选角页提示「先创建角色」，`C.NewCharacter` 成功后自动刷新列表。

测试：`crates/mir-client/tests/round2_panels.rs`（`login_success_exposes_character_list_for_manual_select`、
`server_select_defaults_to_localhost_and_can_retry`、
`character_select_shows_class_level_and_equipment_summary`、`login_screens_render_at_both_sizes`）。

### 5.18 技能表现（火墙 / 雷电 / 瞬息移动 / 隐身 / 召唤）

`ui::spellfx`（Crystal `Client/MirObjects/SpellObject.cs`）。网络层把
`S.Magic` / `S.ObjectMagic` / `S.ObjectSpell` 转成 `WorldSnapshot.spell_events`
（`SpellEvent{spell, location, direction, param, started_ms}`，上限 24 条），
UI 按三类画简化表现（没有逐帧贴图素材）：

| 类别 | 技能 | 画法 | 时长 |
|---|---|---|---|
| 区域 | FireWall、PoisonCloud、ThunderStorm、IceStorm… | 沿 `direction` 铺 3 格半透明色块 | 4000ms |
| 指向 | 雷电、火球系列、HellFire | 施法者 → 落点一条抖动线（无施法者就画落点） | 700ms |
| 自身 | 隐身、魔法盾、召唤系列 | 落点一圈光环 + 技能名 | 1200ms |

伤害数字/受击动作仍走 `S.DamageIndicator` / `S.ObjectStruck`。两个注意点：

* `S.ObjectRemove` 只移除实体（`entities.remove`），**不会**提前抹掉火墙色块：
  表现按 `AREA_MS = 4s` 淡出，而服务端火墙能活 `10 + 伤害/2` 秒——客户端看到的是
  「一簇火苗 4 秒后消失、但伤害仍在」的简化表现（没有逐帧贴图素材）；
* `S.ObjectHidden` 会把该对象的外观/名字/血条一起隐藏并禁止点选
  （`GameScene.cs:5780-5790` 的等价行为），不是只隐藏名字。

测试：`crates/mir-client/tests/round2_panels.rs`
（`spell_packets_create_visible_effects`、`object_hidden_hides_entities_everywhere`），
以及服务端 `tests/magic.rs` 的包序断言。

### 5.19 HUD 时间与天气

* **时间（昼夜）**：协议有 `S.TimeOfDay{lights}`（index 61），客户端已接
  （`net.rs` 写 `lights`），收到后在 HUD 顶部右侧显示黎明/白天/黄昏/夜晚；但**服务端
  目前不下发**这个包（`mir-server/src/session.rs` 注明「本作没有 TimeOfDay」），所以当前版本
  实际看不到时间。
* **天气**：没有独立的 `S.Weather` 包；天气随 `S.MapChanged.weather`
  （`WeatherSetting`）下发，客户端已接（`net.rs` 写 `weather`），在 HUD 顶部右侧与时间同一行
  显示晴/雾/余烬/火星/雪/雨/落叶，例如「黄昏 · 雨」/“Evening · Rain”。窗口宽度 < 560 时
  顶部空间不足，不显示这一行。
* 测试：`tests/round2_panels.rs` 的 `time_and_weather_follow_server_packets`，截图
  `target/client-time-weather.png`。

### 5.20 好友/黑名单面板（F）与联盟页签（第 8 轮）

**好友面板**：`ui::friends`，**F** 开关（本作扩展键）。Crystal 其实有
`Client/MirScenes/Dialogs/FriendDialog.cs`（标题 199、12 行、好友/黑名单两 tab、
Add/Remove/Memo/Email/Whisper，`KeybindOptions.Friends` 可配键位）；本作是独立轻量
面板，固定 `F`，不做 Email/Whisper 快捷按钮（用聊天/邮件面板代替）。

* 数据：`S.FriendUpdate`(245) 整表下发，`WorldSnapshot.friends` 是
  `ClientFriend{index, name, memo, blocked, online}`；登录/增删/改备注/刷新都会整表重发；
* 列表：每行 **名字 / 备注 / 在线（绿）或离线 / `[黑名单]` 标记**；鼠标滚轮滚动
  （clamp 到 `0..len-visible_rows`），点行选中并载入备注；**Enter** 提交当前输入框；
* 底部三行：
  | 操作 | 控件 | 包 |
  |---|---|---|
  | 添加/拉黑 | 名字输入框 + 「黑名单」开关 + 「添加」 | `C.AddFriend{name, blocked}`(127) |
  | 改备注 | 备注输入框 + 「保存」 | `C.AddMemo{character_index, memo}`(130)（≤200 字） |
  | 删除 / 刷新 | 「删除」「刷新」 | `C.RemoveFriend{character_index}`(128) / `C.RefreshFriends`(129) |
* 面板 380×300（800×600 时最多 8 行），320×240 压缩到约 5 行、输入/按钮全部在面板内；
  空列表提示「好友列表为空…」；F 打开时自动 `C.RefreshFriends`。

**联盟页签**：行会面板（`G`）第 4 个页签（index 3；公告页顺延到 index 4）。
输入行会名/ID 后点「申请」「同意」「解除」，分别发聊天命令
`/ally request <name>`、`/ally accept`、`/ally remove <name>`，回复走服务端
`S.Chat`（TextKey `ally_*`，中英随语言；`session.rs::on_ally_command`）。
名单由 **`S.GuildAllies`(303)** 整表下发（本项目扩展；登录/结盟/解除/对方解散时刷新，
按行会 ID 升序，最多 5 条），客户端整表替换、不做乐观更新；每行显示「行会名 + 会长」
（`leader` 为空显示 `—`），空表给提示；`S.GuildStatus` 收到空行会时清空名单
（无行会服务端不发 303）。320×240 下面板宽 304、5 个页签均分内容宽，联盟输入/按钮分两行，
全部在屏内。

测试（`tests/friends_panel.rs`，7 条）：`friends_layout_fits_small_screen_and_row_at_uses_scroll`
（320×240 控件不重叠、`row_at(scroll)` 绝对下标）、`friend_actions_map_to_packets_127_to_130`
（含 `AddFriend`/`AddMemo` 字节编码）、`scrolled_click_selects_the_absolute_row_and_removes_that_friend`、
`friend_update_replaces_the_list_and_language_switch_redraws`（245 整表替换 + 中英切换）、
`friends_panel_renders_selects_and_saves_png`、`guild_ally_tab_fits_small_screen_and_saves_png`、
`guild_allies_packet_roundtrips_and_clears_without_guild`（303 往返/整表/无行会清空）；
截图 `target/client-friends.png`、`target/client-ally.png`、`target/client-guild-allies.png`。

### 5.21 攻城 UI（300–302，第 8 轮）

**字段映射**（`ui::conquest`）：

* `S.ConquestInfo`(300) → `ConquestView`（index/name/owner/war_is_on/start_time/end_time/
  control_points），整表覆盖但**保留已收器械**；
* `S.ConquestObject`(301) → 按 `object_id` upsert（kind: Gate/Wall/Archer、位置/朝向/
  hp/max_hp/owner/destroyed/**open**）；
* `S.ConquestResult`(302) → winner/owner/control_points + `received_ms`（供 5 秒横幅）。

**器械渲染**：按地图 `(y, x)` 排序画在格子上方；先试 `objects.wzl` 第 `3 + kind` 帧
（`scene.libraries().image(2, 3+kind)`）；缺图/空帧回退半透明色块（城门棕/城墙灰/箭塔紫）
+ 本地化类型文字（第 8 轮全量扫过 `objects.wzl` 9999 帧、抽样 62–65/94–95/120–123 与
`tiles.wzl`/`Objects81.wil`，没有公认的城门/城墙/箭塔帧，故维持回退；换真实帧号只需改常量）。
血条按 `hp/max_hp`，前景色由归属行会 ID 派生稳定色（owner=0 灰色）；
`destroyed` 后 1 秒内画确定性碎裂粒子再移除，重复 destroyed 包不重复提示。

**城门开关**：`gate_is_open = open || destroyed || hp <= 0`。关闭=色块 + 底部暗红阻挡条；
开启=绿色通道 + 「城门已开」（`draw_gate_open`），仍保留血条；摧毁先播 1 秒碎裂再保留开启标记。

**箭塔弹道**：`S.ObjectRangeAttack` 命中未毁 Archer 时生成弹道事件（from/to/started_ms，
上限 16 条），画 **400ms** 点状拖尾；复用第 8 轮远程表现（Swing 音效 + spell 命中闪光），
`spell=None` 时只画弹道。

**HUD**：右上角「攻城进行中 / 剩余 mm:ss（`end_time - 本地时钟`，clamp 0）/ 归属 #id
（无归属）/ 控制点 max/6 + 进度条」；320 宽缩到 180–250。
`S.ConquestResult` 到达后居中横幅「攻城结束 · 胜者 #id」，`RESULT_BANNER_MS = 5000` 后消失。

**聊天与 i18n**：开战/结束/器械摧毁生成 SystemChat；新增 14 个双语 `Label`
（攻城进行中/剩余/归属/控制点/结束/胜者/城门/城墙/箭塔/被摧毁/无归属 + 3 条系统句），
后续 t74 又加了 2 个（城门已开/弹道），`Label::ALL` 325→**327**。

**兼容/回退**：没收到 300–302 时 `conquest = None`，器械/HUD 不画，老服务器与单机行为不变；
未知索引/解码失败在 net 读循环 `warn` + `decode_errors++` 跳过，不 panic。

测试（`tests/conquest.rs`，7 例）：`conquest_packets_roundtrip_through_encode_decode`、
`snapshot_tracks_objects_and_destroyed_chat`、`no_conquest_packets_leave_snapshot_and_ui_unchanged`、
`hud_timer_and_result_banner_expire`、`objects_render_with_fallback_blocks_and_save_png`、
`gate_open_state_changes_the_rendering`、`archer_range_attack_spawns_a_projectile`；
截图 `target/client-conquest.png`、`target/client-conquest-gate.png`。

### 5.22 组队面板（P）与行会成员页（第 9 轮）

**组队面板（`P`，`ui::group`）**：成员表列 **名字 / 等级 / 职业 / 地图 / 在线 / HP 条与百分比**；
`members[0]` 标队长；底部「输入+邀请」「离队」「踢人」。离队发 `C.DelMember{name=自己}`（原有）；
320×240 下内容宽 <380 时按钮分两行、成员行容量按按钮上方空间收敛，面板/控件/行全部在屏内；
打开面板时**隐藏 HUD 队友条**避免遮挡。截图 `target/client-group.png`。

**行会成员页（`G` → 成员）**：响应式四列 **职位 / 队员 / 等级 / 在线**；等级从同屏
`S.ObjectPlayer` 或自己关联（离线显示 `--`）；新增「刷新」按钮发
`C.RequestGuildInfo{type=1}`，等 `S.GuildMemberChange` 回填；窄屏四按钮均分不出屏，
成员页不画底部提示避免压按钮。截图 `target/client-guild-members.png`。

### 5.23 文本折行、避头尾与输入（第 13 轮）

**折行**：新模块 `ui/typography.rs`——`char_display_width`（CJK=2、ASCII=1、控制字符=0、
`…`=2）与 `wrap_by_width(text, max_width, max_lines, width_fn)`。聊天用
`UiText::text_width` 的**真实像素宽**折行；每条消息最多 **3 行**，超出时最后一行预留
省略号宽度后加 `…`。普通/队伍/行会/系统颜色仍由 `labels::chat_color` 逐行保留。

**避头尾**：

* 收尾标点 `，。、；：？！）】》」』”’…·～` 与 ASCII `;:?!,.)]}` **不得行首**——折行点
  正好在标点前时，把本行最后一个字符一起挪到下一行；
* 开头标点 `（【《「『“‘([{` **不得行尾**，同样下移。

**截断**：`UiText::draw_clipped` 全局处理——`text_width > clip.width` 时尾部省略号，
退化顺序 `…` → `..` → `.`；物品名/技能描述/任务文本/按钮共用，不换行。

**IME/输入框**：`TextField` 新增**字符级光标**（`cursor`、`cursor_byte_index()`、
`move_left/move_right`）；`insert` 在光标处插入，`backspace` 用 `char_indices` 删光标前
一个字符（不会切 UTF-8）；聚焦时在光标位置画 `|`。中文中间插入、退格、连续退到空、
空串右移都不 panic；测试 `cursor_edits_stay_on_utf8_boundaries`。

测试：`typography` 6 例（2/1 宽度、中英混排、避头尾、最多 2 行省略、显式 `\n`、省略号退化）；
`tests/chat_wrap.rs` 四类消息（长中文/长英文/行会/系统）断言折行 >1 行且无标点行首；
截图 `target/client-chat-wrap.png`。

### 5.24 断线重连（第 15 轮）

新模块 `ui/reconnect.rs`：阶段 `Idle / Waiting / Connecting / Failed`；
`BACKOFF_MS = [1000, 2000, 4000]`、`MAX_ATTEMPTS = 3`，重复 `Closed` 不重置进度。
`NetEvent::Closed` → `disconnected()`；每帧 `due()` 到点用原 `ConnectOptions`（账号/密码/角色）
发起 `NetClient::connect`；快照 ready → `succeeded()`；单次失败 → `attempt_failed()` 排下一次；
3 次失败 → 显示「无法连接」并切回 `LoginState`（账号/密码带入，静音占位播放器）。

UI：`draw_ui` 在顶部画提示横幅「连接已断开，正在重连（N/3）」/「无法连接」；
`Label::Reconnecting` 双语（复用已有 `ConnectFailed`），非 TextKey。测试 5 例：
首次成功、指数退避 + 3 次失败、提示序号、重复断线不重置（另含 minimap 传送点标记用例）。

### 5.25 常用地图名与商店快捷操作（第 27 轮）

#### A. 常用地图名（客户端表）

- **数据源**：`config/lang/{zh-cn,en}.json` 的 **`maps` 段**（25 张常用图；键 = 地图文件名，
  不含 `.map`）。中文名逐字取自 `cjlaaa`/`Jev` 的 MirDB `MapInfo.Title`；`map0` 的英文名用
  `config/lang/en.json` 的 `Bichon Province`（不是 `BichonProvince`）。
- **实现**：`ui/map_names.rs` 用 `LanguageLibrary::embedded()` + `OnceLock` 缓存（避免每帧解析
  JSON）；新增地图只改语言包，不改代码。
- **回退链**（`localized(file_name, data_title, language)`）：
  `语言包 maps → 服务端标题（`S.NewMapInfo.title`/`S.MapChanged.title`）→ 文件名 → Label::UnknownMap`
  （中文「未知地图」/ 英文 `Unknown map`）。
- **只有标题、没有文件名**时用 `localized_title(data_title, language)` 反查语言包：
  比较时忽略大小写与空格/下划线/连字符（`BichonProvince` ↔ `Bichon Province`）；
  未收录时原样返回（空串仍是空串）。
- **接入点（5 处）**：
  1. HUD 地图名：`ui/hud.rs:226`；
  2. 小地图标题：`ui/mod.rs:4231`；
  3. 大地图标题：`ui/mod.rs:4254`；
  4. 小地图传送点悬停：`ui/minimap.rs:188`（`movement.title` 走 `localized_title`）；
  5. 组队面板队友地图：`ui/group.rs:433`（`S.GroupMembersMap` 只有标题，走 `localized_title`）。
- **测试**：`map_titles_come_from_the_language_packs`、`both_language_packs_cover_the_same_maps`、
  `unknown_maps_fall_back_to_the_server_title_then_the_file_name`、
  `titles_without_a_file_name_are_reverse_looked_up`。

#### B. 商店快捷操作 `S` / `R`

| 键 | 动作 | 前置 | 结果 |
|---|---|---|---|
| `S` | 卖背包选中格 | **商店面板已打开** + 背包有选中物品 | `UiAction::SellItem{unique_id, count:1}` → `NetCommand::SellItem` → **`C.SellItem`** |
| `R` | 修背包选中格 | **商店面板已打开** + 背包有选中物品 | `UiAction::RepairItem{unique_id}` → `NetCommand::RepairItem` → **`C.RepairItem`** |

- 判断逻辑在 `UiState::sell_selected_item` / `repair_selected_item`（`ui/mod.rs:1564/1579`）：
  未打开商店或未选中背包物品时返回 `None`（**不发包、不提示**）；卖出数量固定 1，与
  Crystal 右键出售一次一件一致。
- 窗口层在 `app.rs:2669-2673` 统一处理：`live_shop_open() && live_sell_selected()` /
  `... && live_repair_selected()` 任一不满足就不动作。
- **注意**：本地/离线地图查看模式的 `S` 仍是截图（`app.rs:2508-2513`），不是卖；`S`/`R`
  的商店语义只在联机（Live）模式生效。
- 测试：`crates/mir-client/tests/npc_flow.rs` 覆盖「未开商店/未选中 → None」「选中 → C.SellItem/
  C.RepairItem」与双语；`ui/equipment.rs` 另有修理动作映射单测。



### 5.26 125 ObjectProjectile 客户端渲染（R59：FireBounce 最小闭环）

> **口径**：R59 只做 Crystal `GameScene.cs:4687-4715` 的客户端**表现链最小闭环**（事件消费 → 飞行插值 →
> 命中闪光 → 音效）；不建 `Missile`/`Effect` 类体系，**只按最小闭环 + 已知偏离表述**。已知偏离见 §F，
> 其中 **D-R59-1** 是正式登记偏离。实现：`crates/mir-client/src/ui/projectile.rs`（D1/D2），接线
> `ui/mod.rs:1562-1564`（每帧 `sync`）与 `ui/mod.rs:4278-4279`（绘制）。

#### A. 事件来源（服务端 → 客户端快照）

- 包：`S.ObjectProjectile`(125) → `net.rs:5385-5401`；数据结构 `ProjectileEvent{spell, source, destination}`
  （`net.rs:1503-1511`），**无时间戳、无落点坐标**。
- **入队条件**：`spell == Spell::FireBounce && snapshot.object_position(source).is_some()`
  （`net.rs:5391-5393`）——即 **FireBounce 白名单 + source 必须已知**；其它 spell 与未知 source 在入队处
  直接丢弃（对齐 Crystal `GameScene.cs:4691` 的 `if (source == null) return;` 与 `:4693` 只 `switch
  (Spell.FireBounce)`、无 default）。
- 队列：`WorldSnapshot::projectile_events`（`net.rs:2763`）；容量沿用 `EFFECT_EVENT_LIMIT = 16`（`net.rs:166`），
  超限 `remove(0)` 丢最旧（`net.rs:7246-7260`，与 `effects` 同一策略，R59 **未改常量**）。
- `destination` **允许未知**：`net.rs:1508-1509` 明示 unknown destination 不得 panic、不得在入队处丢事件
  （Crystal 侧只有 `:4701` 的命中挂接受 `missile.Target != null` 门控）。

#### B. 消费语义（`Projectiles::sync`，`projectile.rs:422-439`）

- 每帧 `UiState::update` 调 `self.projectiles.sync(snapshot, self.time_millis)`（`ui/mod.rs:1562-1564`）。
- **不是 `mem::take`**：快照整体克隆、`projectile_events` 在会话侧累计且从不 drain，而绘制路径只持
  `&WorldSnapshot`，因此用**位置游标** `Projectiles::consumed` 去重（`projectile.rs:369/446-462`）；
  快照事件表长度回退（重连/换快照）时游标归零重新消费。**未 clone 快照绕过 `&mut`。**
- 一个事件最多生成 1 个 `ProjectileView`；同帧多事件按到达顺序生成；同一快照重复 `sync` 不重复生成。
- `source` 解析不到坐标 → 丢弃（入队层已拦，此处双保险）；`destination` 解析不到坐标 → **不生成实体**
  （见 §F D-R59-1）。

#### C. 飞行插值（公式按 Crystal 移植，参数取自 `GameScene.cs:4699`）

`CreateProjectile(410, Libraries.Magic, true, 6, 30, 4, targetID: p.Destination)` → `count=6` 帧/向、
`interval=30` ms、`skip=4`（方向步长 `4+6=10`）。

| 量 | 本作（`projectile.rs`） | Crystal |
|---|---|---|
| `duration` | `max_distance(source, dest_at_spawn) * 50` ms（`duration_ms`，`CELL_FLIGHT_MS=50`） | `PlayerObject.cs:3552` `MaxDistance(CurrentLocation, targetPoint) * 50` |
| `count` | `max(1, duration / 30)`（`frame_count`） | `Effect.cs:172-191` `Duration/Interval`，`0 → 1` |
| 当前帧 | `frame = elapsed_ms / 30`（`frame_at`，`now_ms` 由 `UiState.time_millis` 提供） | `Effect.cs` 每 `Interval` 加 1 帧 |
| `dir16` | `direction16()`：`GameScene.cs:12044-12065` 移植（夹角 `+11.25°`、`/22.5` 取整、`destination.X < c.X` 走 `360° - angle` 镜像；0=上、4=右、8=下、12=左） | 同 |
| `index` | `410 + frame % 6 + dir16 * 10`（`sprite_index`，恒在 `410..=565`） | `Effect.cs:201` `BaseIndex + CurrentFrame % FrameCount + Direction * (Skip + FrameCount)` |
| 位置 | `(dest - source) * (48, 32) * frame / count`（`draw_offset`；`frame=0` 源格、`frame=count` 目标格，整数向零截断） | `Effect.cs:211` `(x * CurrentFrame / Count, y * CurrentFrame / Count)` |
| 绘制 | `draw_effect(frame, FxKind::Magic, index, x+dx, y+dy)`；`draw_effect` 实为 `image(magic.wzl, index + kind.start_frame())`，`Magic` 的 `start_frame = 0`（`entities.rs:518-523`），故 index 直落 `magic.wzl[index]` | `MLibrary` 410 基座 |

- **帧段**：16 向 × 6 帧 = `410-415/420-425/…/560-565`，另加命中 `570-579`；`416-419`、`566-569` 是
  0×0 **空帧**，本式不会取到。
- 目标位置每帧重取 `snapshot.object_position(destination_id)`；目标消失则停在最后已知位置。
- 示例：水平 5 格 → `duration=250`、`count=8`、`t=0` 源格、`t=250ms` 目标格；`dir16=4` → `index=410+0+40=450`。

#### D. 命中 570（D2 已落盘）

- 到达帧（`frame >= frame_count` 即将删除）时，若 `destination_id` 仍解析出对象且该对象 `dead == false`，
  在其格播 `magic.wzl[570..=579]` × 60ms = **600ms**（`FxKind::Magic`），对齐
  `GameScene.cs:4708` `new Effect(Libraries.Magic, 570, 10, 600, sender.Target)`。
- 目标未知或已 `dead` → **不播命中**，对齐 `:4701`（仅 `missile.Target != null` 才挂 `Complete`）与
  `:4707`（`CurrentAction == Dead` 直接 return；此时 `+2` 命中音效同样不播）。
- 状态（D5b 同步）：命中 570 状态机已落盘——`HIT_EFFECT_BASE_INDEX=570`、`HIT_EFFECT_FRAME_COUNT=10`、
  `HIT_EFFECT_DURATION_MS=600`（`projectile.rs:79-85`），删除点见 `advance`（`:505-537`）；D4
  `cargo test -p mir-client --test round59_projectile_render` → **16 passed / exit 0**，覆盖目标存活/已死/无目标
  分支（`target/round59/d4/attempt-1/02-render.log`）。证据：`target/round59/d2/attempt-1/`、
  `target/round59/d4/attempt-1/`，索引见 [R59 测试索引](testing/round59/README.md)。

#### E. 音效（基准是 GreatFireBall=34，不是 FireBounce=154）

- 出生：`GameScene.cs:4697` `SoundManager.PlaySound(20000 + (ushort)Spell.GreatFireBall * 10 + 1)` =
  `20000 + 34*10 + 1 = 20341` → **`M34-1.wav`**；命中 `:4709` 的 `+2` = `20342` → **`M34-2.wav`**。
- `GreatFireBall = 34`（`Shared/Enums.cs:1168`）、`FireBounce = 154`（`:1264`）；既有
  `mir-render/src/sound.rs:220` 的 `spell_sound` 产 `M{spell}-0.wav`（FireBounce → `M154-0.wav`），
  **与投射物音效不同源**。D2 另增 `PROJECTILE_SOUND_BASE = 34`、`PROJECTILE_SOUND_SPAWN_INDEX = 20341`、
  `PROJECTILE_SOUND_HIT_INDEX = 20342`、`ProjectileSoundPhase{Spawn,Hit}` 与
  `pub const fn projectile_sound(spell, phase) -> Option<&'static str>`（`sound.rs:234-281`）：
  `FireBounce` → `Spawn="M34-1.wav"`、`Hit="M34-2.wav"`，其它 spell 返回 `None`；单测
  `cargo test -p mir-render --lib sound` → **9 passed / exit 0**。
- 出生音效**随投射物生成**：无投射物（D-R59-1）则无声，不单独播。
- 素材（D5b 同步）：`game-assets/audio/sfx` 现 105 个文件，含 `M34-0/-1/-2.mna`；`M34-1/-2` 已由
  `resource/community/gqf2008-crystal/Sound/M34-{1,2}.wav` 经 `mir-tools audio import`（本机 ffmpeg）产出，
  `audio verify --input game-assets/audio` → `files=106 bytes=4.25MB`、exit 0
  （`target/round59/d4/attempt-1/08-audio-verify.log`）。`game-assets/` 不入库，重生命令见
  [R59 索引](testing/round59/README.md) §5。
- **真实可听仍为 PARTIAL**：生产路径 `sync` 把 cue 记进 `RecordingSoundSink`（`projectile.rs:303-339`），
  `take_sound_cues()`（`:413`）等 `mir-client` 音频通道每帧 drain；但 UI 只持 `&WorldSnapshot`，`app.rs`
  接线不在 R59 写范围 → `M34-1/-2` 尚未经真实音频设备播放。D4 只断言 cue 的字面量、顺序与「每投射物各一次」
  （`spawn_and_hit_sounds_use_m34_once_each`、`production_sound_cues_are_drained_once_per_projectile`）。

#### F. 已知偏差（不得当作与 Crystal 完全等同）

1. **本作 frame 在生成时冻结**：`duration`/`frame_count` 只在 spawn 时算一次（`projectile.rs:465-497`）；
   Crystal `Effect.cs:172-191` 在 `Process` 里**每帧重算** `Duration = MaxDistance(Source, Destination) * 50`
   与 `Count = Duration / Interval`，目标移动会改变总时长。本作目标当前位置只用于插值落点。
2. **Draw offset 不含 wzl 逐帧 offset**：Crystal `MLibrary` 的 `offSet=true` 路径会叠加每帧
   `ImageInfo.offset`，`mir-render` 绘制层未实现该偏移；属既有绘制缺口，R59 未改 `mir-render`。
3. **D-R59-1（无落点即不生成）**：Crystal `GameScene.cs:4699` **无条件** `CreateProjectile`（落点可在
   `CreateProjectile` 内用 `TargetPoint` 兜底，`:4701` 才按 `missile.Target` 门控命中）；125 只带
   `source`/`destination` 两个对象 ID，`destination` 不在快照里就没有落点坐标 → 本作**不生成投射物、不生成
   命中、不播 `+2`**，也不伪造坐标。这是诚实降级，**与 Crystal 有差**，不得写作「对齐」。
4. **事件无 ID**：`ProjectileEvent` 无时间戳/序号，队列累计、按位置游标消费；队列满 16 条后
   「丢最旧 + 追加」在 `len` 上不可观测，可能漏掉新事件（残留风险；修复需在 `net.rs` 给事件编号）。
5. **无 Hero 实例、无真 socket**：本轮证据是进程内 encode/decode/快照链，不等于 socket 端到端；
   125 不回显施法者（`World::broadcast` 不含本人）仍由服务端语义决定，客户端不补偿。

## 6. 素材加载与渲染

### 6.0 真实素材渲染（第 6 轮）

**数据目录**

* 默认 `resource/data`（相对启动目录），可用 `--data-dir <目录>` 指定；
* 指定目录不存在时依次回退 `resource/data` → `resource/ld/data`；都不存在则保留原值并报错；
* 启动日志：`素材目录：resource/data`、`地图 0：700×700 · 图库 28 个（+WIL 5 个）`；
  联机进图同样打印。

**支持的图库格式**

| 格式 | 编码/说明 | 状态 |
|---|---|---|
| `.lib`（v2/v3） | gzip RGBA32 / DXT1 | 支持 |
| `.wzl` + `.wzx` | zlib；8 位调色板 / RGB565 / RGB24 | 支持 |
| `.wil` + `.wix` | WEMADE `#ILIB`/`#INDX`；16 位 RGB565，`0x0000` = 透明 | **第 6 轮新增** |

`resource/data` 实测：28 个 `.wzl` 地图库 + 9 个 `.wil` 中 **5 个可打开**
（另 4 个是非 `#INDX` 头的变体，warn 后跳过，不影响渲染）。

**地图瓦片**

* 按 `.map` 的 back/middle/front 图像号解码，等距（isometric）CPU 渲染；
* **只解码视口内瓦片**；解码后的图像缓存上限 **4096 张**，库头信息单独缓存；
* 缓存观察：暖机 177 张瓦片，连续走 30 格后 296 张，未触发上限清空；
* 切图（`S.MapChanged`）时按新地图文件**重载地形**（失败保留当前地形并打印原因）。

**实体**

* 玩家：`hum*`/`hair*`/`weapon*`/坐骑/翅膀分层（§6.1），动画复用 `ActorAnimation`；
* 怪物 `mon*`、NPC `npc*`、地面物品 `items` 按 `image`/`look`/`hair`/`weapon` 选库与帧；
* `.wzl` 没有 alpha：纯黑像素统一置透明（修复角色头发/武器的黑框）。

**缺资源回退**

素材目录缺失/图库打不开 → 空图库 + 空白地形（登录窗仍可开），`warn` 一次；
缺帧 → 站立帧 → 库首帧 → 彩色占位方块，**不 panic、不黑屏**。

**素材缺失审计（t84/t91，`asset_scan`）**：`objects.wzl` 候选器械帧 3/4/5 宽高均为 0（空帧），
且 62–65/94–95/120–123 抽样都是野地/石壁/树木纹理 → 确认素材库没有真实城门/城墙/箭塔，
器械继续用「半透明色块 + 类型文字」回退（换真实帧号只改常量）。

59 个 `mon*.wzl` 全部可打开，无整库缺图；但有**空帧区间**（t91 清单）：
`mon7.wzl` 首非空帧 **440**（前 440 空）；`Mon11.wzl` [340,359]/[700,719]/[1060,1079]/[1420,1439]；
`Mon6.wzl` [850,859]/[1280,1289]/[1710,1719]；`mon-kulou.wzl` [345,359]/[705,719]；
`mon1.wzl` [190,279]/[548,559]/[620,687]/[750,760]/[820,839]，且前 10 块中有 1 块站立帧为空。
`hum.wzl`/`objects.wzl` 没有怪物动作帧、mon 库间也无可替换块，所以保留
「站立帧→库首帧→占位方块」**静默回退**（不画错帧）。`asset_scan` JSON 新增
`first_non_empty` / `empty_intervals` / `empty_standing_blocks_first10` 字段
（产物 `target/asset-missing.json`）。HUD/人形/武器/头发/NPC/effect/magic/tiles/objects
等关键库均 present。

**性能（release 实测，第 13 轮）**

测法：`cargo run --release -p mir-render --example perf -- --map resource/map/0.map --data
resource/data --out target/perf-render.json`；摄像机每帧横移 1 格，连续 100 帧，单线程纯 CPU
渲染（含 RGBA 混合），不含窗口呈现/GPU 上传。兵部与刑部两次测量取区间（1080p 平均
**1.45 ~ 1.61 ms**：兵部 1.45 / 刑部 1.614）。

| 视口 | 首帧（含解码） | 平均 | p95 | 最差 | 瓦片缓存命中 |
|---|---|---|---|---|---|
| 800×600 | — | **0.50 ms** | — | — | 99.7% |
| 1280×720 | — | **0.76 ms** | — | — | 99.7% |
| 1920×1080 | 25.0 ms | **1.45 ~ 1.61 ms** | 1.63 ms | 3.17 ms | 99.8% |

100 个玩家同屏（1280×720，地形+实体）：平均 **1.42 ms**、p95 1.54 ms；
特效 200×100：**16 次分配 / 77.7 ms**（优化前 17,500 次分配 / 96.3 ms，分配 -99.9%）。
目标「1080p <2.5ms、100 实体 <2.0ms」均达成；首帧 1080p 25.0ms 是多了**一次性 spans 预计算**，
之后每帧受益。

**优化手段（t104）**：

* **帧合并**：`MapScene` 解码时预计算每张图的「不透明段」表（只含 alpha 0/255 的图建表，
  含半透明像素的图回退逐像素混合）；`Frame::blit_spans` 每段一次 `copy_from_slice`，
  地图瓦片/实体层不再逐像素判断；
* **特效池化**：`EntityRenderer::effect_size/draw_effect` 直接读实体解码缓存并返回尺寸供居中，
  `ui/spellfx.rs` 不再每帧克隆 `Image`；特效缓存复用固定槽位，首次解码后零分配；
* 保留 `Frame::blit` 逐像素路径作为半透明/加法混合回退，行为不变。

**与 t84 基线对比**：

| 场景 | t84 | t104 | 变化 |
|---|---|---|---|
| 1920×1080 平均 | 3.53 ms | **1.45 ~ 1.61 ms** | **-59% ~ -54%** |
| 1280×720 平均 | 1.70 ms | **0.76 ms** | **-55%** |
| 800×600 平均 | 1.01 ms | **0.50 ms** | **-50%** |
| 100 玩家 720p | 2.20 ms | **1.42 ms** | **-35%** |
| 首帧 1080p | 20.6 ms | 25.0 ms（一次性 spans） | — |

数字是单线程纯 CPU 渲染（含 RGBA 混合），不含窗口呈现/GPU 上传；debug 构建约 25–30 FPS，
验收仍以 release 为准。JSON 明细见 `target/perf-render.json`（t104 为
`target/perf-render-t104.json`，更早的优化前为 `target/perf-render-before.json`）。
启动日志每 120 帧打印一次「渲染统计」。

**素材索引（t84）**

* 攻城器械：`objects.wzl` 候选帧 `3/4/5`（Gate/Wall/Archer）全为**空帧**（宽高 0），
  已在 `target/asset-missing.json` 记录；`objects.wzl` 共 9999 帧，抽查 62–65、94–95、
  120–123 均为野地/石壁/树木纹理，`tiles.wzl` 为地面瓦片、`Objects81.wil` 为纹理，
  故维持半透明色块 + 类型文字回退（换帧号只需改 `conquest::object_rect` 旁的常量）；
* 怪物：59 个 `mon*.wzl` 全部可打开且有非空帧，无整库缺失；`mon7.wzl` 首个非空帧在
  440（前 100 帧为空），实体渲染已有「站立帧 → 库首帧 → 占位方块」回退，不需要补图。

**截图与测试**

* 截图：`target/client-render-world.png`（真实地砖 + 玩家 + 路人 + 怪物鸡 + NPC 商人）、
  `target/client-render-animation-a.png` / `-b.png`（走路两帧）；
  第 7 轮另有 `target/client-fx-attack.png`（挥砍）、`client-fx-attack-magic.png`
  （挥砍 + 雷电命中）、`client-fx-plain.png`（对照）；
* 测试：`cargo test -p mir-client -p mir-render -p mir-asset` **460 用例全绿**
  （`resource/` 改名后重跑仍全绿，验证缺素材回退）；`real_assets` 3 例、`render_map`
  走路性能、`mir-asset` 的 WIL 合成/真实 `Objects81` 解码；
* `cargo check -p mir-tools` 通过（WIL 只扩了 `LibraryKind`，无破坏性签名变更）。

扁平素材（`resource/data/*.wzl`）与 Crystal 的「一个外观一个 `.Lib`」不同，因此用文件名
+ 固定步长定位；映射表如下：

| 实体 | 文件 | 图像号 → 帧号 | Crystal 出处 |
|---|---|---|---|
| 玩家身体 | `hum.wzl` / `hum2.wzl` / `hum3.wzl` | `block*600 + action.start + direction*stride + n`（动作表见 §6.3） | 外观选择 `PlayerObject.cs:553`；帧号 `PlayerObject.cs:761`；玩家帧集合 `Frames.cs:157-171`；身体绘制 `PlayerObject.cs:5045` |
| 玩家武器 | `weapon.wzl` / `weapon2.wzl` / `weapon3.wzl` | 同上（与身体**同步**，同一个动作帧号） | `PlayerObject.cs:415-431`、`:5062-5073` |
| 玩家头发 | `hair.wzl` / `hair2..4.wzl` | 同上（与身体**同步**） | `PlayerObject.cs:554`、`:5054` |
| 玩家盾牌 | `shield.wzl`（语料里**没有**） | 同上 | Crystal 没有盾牌层（见 §6.2） |
| 玩家坐骑 | `horse_x.wzl` | `block*640 + action.start + direction*8 + n`（3 个完整块；动作表见 §6.3） | `DrawMount`（`PlayerObject.cs:5084-5089`）；`S.ObjectPlayer.mount_type/riding_mount`、`S.MountUpdate` |
| 玩家翅膀 | `wingeffect_x.wzl` | `block*600 + effect.start + direction*8 + effect`（2 个块；`Frames.cs:157-170` 的 effect 列） | `DrawWings`（`PlayerObject.cs:5075-5081`）用了 `DrawWingFrame`（`:762`）；`S.ObjectPlayer.wing_effect` |
| 怪物 | `mon{image/10+1}.wzl` | `(image%10)*430 + action.start + direction*stride + n`（默认动作表见 §6.3） | 图像 → 图库 `MonsterObject.cs:208`；默认怪物帧集合 `Frames.cs:25-31`；绘制 `MonsterObject.cs:4307-4309` |
| NPC | `npc.wzl`、`npc2.wzl`…（每 100 个图像换库） | `(image%100)*60`，取站立首帧 | 图像 → 图库 `NPCObject.cs:77-80`；帧号 `NPCObject.cs:101`；默认 NPC 帧集合 `Frames.cs:18-19` |
| 物品 | `items.wzl`（缺失时 `dnitems.wzl`） | 帧号 = `ObjectItem.image`；金币按数量用 112..116 | `ItemObject.cs:32,37`（物品）、`:49,55-64`（金币）、`:72-73`（绘制） |

几何锚点：

* 玩家/怪物/NPC：格原点 + 图像头里的 `offset_x/offset_y`（`MLibrary.cs:658-665`，
  `PlayerObject.cs:5045` 的 `offSet: true`）；玩家的**每一层**共用同一个格原点，各自叠加
  自己的图像偏移，所以不需要额外的层间偏移常量（坐骑块里的图像偏移已经是相对格原点的）；
* 物品：在格子里居中（`ItemObject.cs:78-79`），不叠加图像偏移（`:72-73` 的默认 `offSet: false`）；
* 绘制顺序：地图 Y 升序、同一行 X 升序（`GameScene.cs:10798-11008`）。
* 精灵缺失：画 12×18 的彩色方块（玩家蓝、怪物红、NPC 绿、物品黄；死亡为灰色），
  收到过 `ObjectHealth`/`HealthChanged` 的实体再画 32×4 的头顶血条（见 §5 世界内文字）。
  无 `resource/` 的 CI 也能得到非空画面。

### 6.1 玩家装备分层（纸娃娃）

`mir_render::EntityView` 带七个外观字段（`armour/weapon/weapon_shape/hair/shield/mount/wings`），
`EntityRenderer` 按 Crystal 的顺序逐层叠加：

```text
坐骑
朝 Left/Up/UpLeft：主手 → 身体 → 头发 → 翅膀 → 副手
朝 DownLeft：      主手 → 身体 → 翅膀 → 头发 → 副手
朝 UpRight/Right： 副手 → 身体 → 头发 → 翅膀 → 主手
朝 DownRight/Down：副手 → 身体 → 翅膀 → 头发 → 主手
弓手（WeaponLibrary1=null，AR 弓在 WeaponLibrary2）：
朝左组/右组都在最后无条件再画一次 DrawWeapon2；左组末尾因此出现两次副手，
右组身体前一次、末尾一次。Crystal 不按方向组跳过这额外一次。
（骑乘时 Crystal 不画武器，PlayerObject.cs:4886,4914；本作同样跳过）
```

顺序来自 `PlayerObject.Draw`（`Client/MirObjects/PlayerObject.cs:4884-4923`）：

| 步骤 | Crystal | 说明 |
|---|---|---|
| 1 | `:4884` `DrawMount()` | 坐骑，画在身体之下；`MountType < 0` 或未骑乘时不画（`:5086`） |
| 2 | `:4886-4892` `DrawWeapon()`/`DrawWeapon2()`（身体**之前**） | 朝左组画主手，朝右组画副手 |
| 3 | `:4894` `DrawBody()` | 身体层 |
| 4 | `:4896-4911` `DrawHead()` / `DrawWings()` | 朝上/左/左上/右上/右先头发后翅膀；朝下/左下/右下先翅膀后头发 |
| 5 | `:4914-4921` `DrawWeapon()`/`DrawWeapon2()`（身体**之后**） | 朝左组画副手，朝右组画主手 |
| 6 | `:4922` 弓箭手 `DrawWeapon2()` | **所有方向**在方向分支之后无条件再补一次副手 |

本作 `player_layout::draw_order` 用纯函数锁定这 8 个方向的层序；主手=`WeaponLibrary1`
（普通唯一武器/刺客 WeaponsR），副手=`WeaponLibrary2`（刺客 WeaponsL、弓手
`ARWeapons`/`ARWeaponsS`，普通职业为空）。弓手的 `WeaponLibrary1` 为 null，弓只存
在 `WeaponLibrary2`（Off）；因此左组末尾的两次副手与右组身体前/末尾各一次，都是
Crystal 同层重复绘制，不是两把弓。

帧公式：

```text
身体/武器/头发/盾牌：DrawFrame = block * 600 + direction * 8 + n   // n = 0..=3（站立）
坐骑：               block * 640 + direction * 8 + n               // horse_x.wzl，3 块
翅膀：               block * 600 + effect.start + direction * 8 + e // wingeffect_x.wzl，2 块
```

坐骑外观号 = `S.ObjectPlayer.mount_type + 1`（0 表示没有；`S.MountUpdate` 同理），
对 3 取模；翅膀外观号 = `S.ObjectPlayer.wing_effect`，对 2 取模。Crystal 的
`WingEffect >= 100` 是其它特效而不是翅膀层（`PlayerObject.cs:577,650,5077`），
渲染层不画；`MountType`/`RidingMount` 缺一不画（`:5086`）。

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
本模块尚未解析（R49 的原生表来自 Crystal `.Lib` v3 的 `frames.json`，不是 `!Mon.dat`）；
解析后可以做到逐个怪物精确映射。怪物帧号还做了逐级回退：
精确帧 → 该图像站立帧 → 库首帧 → 占位方块，因为并非每个 `monN.wzx` 都恰好有
`10*430` 帧（例如 `mon1.wzx` 只有 1086 帧）。

R49 B 起，**默认 `game-assets` 路径**优先读 A2 生成的
`gfx/sprites/monster-native/monster-{image:03}/frames.json`：动作号保持 Crystal
原始号（0/1/2/9/18/21/22），帧号是 `Start + direction*(Count+Skip) + local`，
不再套 `(image%10)*430` 的旧扁平区间。只有缺 `frames.json`、表损坏或该动作缺表时
才走上面的旧 `monN` 回退；有原生表但缺某一帧不会跨动作借帧。动作生命期为
Standing 2000ms、Walk/Run 600ms、Attack1 600ms、Struck=帧 Count×Interval、Dying 1000ms，
Dead 停住；逐帧仍由 `Interval/Count` 决定。证据与边界见
[`testing/native-monster/README.md`](testing/native-monster/README.md)。

覆盖范围按证据写：A2 只导入了 **118 个 v3 used 怪物库**（used 中 4 个是 v2、无 `frameSeek`，
不能按 `Frames.cs` 语义读；另有约 800 个 appearance 在源里缺对应 `.Lib`，保持 missing）。
逐帧真实渲染只覆盖 `005/022/039/023`，其余 used 库只有结构/计数证据；**不能**写成
“122 个 used 全 native”。Crystal 的 Blend（117）/Reverse/Effect/Sorting 语义尚未移植。
R49 当前 gate=PARTIAL，待队长最终全量门禁，不能沿用历史批次结论。

R49 C 起，**怪物走/跑**也按 Crystal `MonsterObject.cs:345-433` 插值：只有
`Walking/Running` 建立 `MonsterMotion`；`Movement = CurrentLocation` 身后 1/2 格，
`OffSetMove` 按 8 方向、`CellW=48/CellH=32`、`Frame.Count/Interval` 逐帧计算，
最后做 C# 同语义的 `x % 2 + x`（保留负数符号，不是 `rem_euclid`）；绘制/排序锚点用
`Movement` 与 `CurrentLocation` 的较大 Y。动作自然结束、受击/死亡/删除/切图/断线时
清状态；Push/Jump/DashAttack 本轮不扩散。帧数优先取 `frames.json` 的 Walking/Running
`Count`，缺表回退 6×100ms。证据见 [`testing/monster-motion/README.md`](testing/monster-motion/README.md)。
真实证据只覆盖 `005/022/184` 三只怪（不同方向/不同 Walk 帧数）；坐骑帧表、Push/Jump/DashAttack
仍未接入，窗口四边界与 FPS 未实测。

坐骑/翅膀的扁平语料与 Crystal 的 `Mount/*.Lib` / `CHumEffect/*.Lib` 也不是一一对应：

* `resource/data/horse_x.wzl` 是 3 个完整坐骑外观 × 640 帧（第 4 块只有 40 帧，丢弃），
  帧 0 是干净的坐骑（28×122），所以用它当 `Mount` 图库；`mount = 1..` 对 3 取模。
* `horse.wzl`（9×640）、`horse2.wzl`（6×640）是**骑乘人物**整身外观（帧 0 就带骑手），
  再叠身体会双重人物，因此本作不把它们当 `DrawMount` 的坐骑层；这是素材差异，不是漏做。
* `wingeffect_x.wzl` 有 1200 帧 = 2 个翅膀外观 × 600，动作布局与 `hum.wzl` 相同、
  正好对应 `Frames.cs:157-170` 的 effect 列；`wingserieseffect_x.wzl` 是 10 帧/方向
  的另一种布局，不能按 600 帧块索引，本作不用。

### 6.3 动作帧表与时长（`mir-render/src/animation.rs` / `player_layout.rs`）

Crystal 的 `Frame` 是 `Start / Count / Skip / Interval`（`Client/MirObjects/Frames.cs:203-244`），
绘制帧号 `DrawFrame = Frame.Start + (Frame.OffSet * Direction) + FrameIndex`
（`PlayerObject.cs:761`、`MonsterObject.cs:357`），其中 `Frame.OffSet = Count + Skip`
（`Frames.cs:209-212`）。**`Skip` 是每个方向段末尾的空槽数，不是播放前跳过的帧**。

`resource/data/hum.wzl` 是旧的扁平 600 帧布局，仅作为原生库缺失/骑乘时的
fallback；默认 auto→dev 路径下玩家走原生 `player-native/*` 库。原生布局的
`Start/Count/Skip/Interval` 来自 Crystal `Frames.cs:157-185`（Player 段）与
`PlayerObject.cs` 的 `Frames` 选择（`mir-render/src/player_layout.rs`）：

| 动作 | `NativeAction` | Body Start/Count/Skip | Body Interval | Effect Start/Count/Skip | Effect Interval |
|---|---|---|---|---|---|
| 站立 | `Standing` | 0/4/0 | 500 ms | 0/8/0 | 250 ms |
| 行走 | `Walking` | 32/6/0 | 100 ms | 64/6/0 | 100 ms |
| 跑步 | `Running` | 80/6/0 | 100 ms | 112/6/0 | 100 ms |
| 攻击 1 | `Attack1` | 136/6/0 | 100 ms | 168/6/0 | 100 ms |
| 攻击 2 | `Attack2` | 184/6/0 | 100 ms | 216/6/0 | 100 ms |
| 攻击 3 | `Attack3` | 232/8/0 | 100 ms | 264/8/0 | 100 ms |
| 攻击 4 | `Attack4` | 416/6/0 | 100 ms | 448/6/0 | 100 ms |
| 法术 | `Spell` | 296/6/0 | 100 ms | 328/6/0 | 100 ms |
| 受击 | `Struck` | 360/3/0 | 100 ms | 392/3/0 | 100 ms |
| 死亡 | `Die` | 384/4/0 | 100 ms | 416/4/0 | 100 ms |
| 尸体 | `Dead` | 387/1/3 | 1000 ms | 419/1/3 | 1000 ms |

`Start/Count/Skip` 是身体帧；`Effect Start/Count/Skip` 是独立的翅膀/光效帧表，不能
用身体帧号代替。站立身体 4 帧而 effect 8 帧、攻击 3 身体 232 起而 effect 264 起，
是最容易被混用的两处。

扁平 fallback 仍按 `resource/data/hum.wzl` 的 600 帧布局
（`mir-render/src/animation.rs::human_frames`），不能把原生表套到旧图上：

| 动作 | `ActorAction` | Start | Count | Skip | 步长 | Interval | 时长 |
|---|---|---|---|---|---|---|---|
| 站立 | `Standing` | 0 | 4 | 4 | 8 | 500 ms | 循环 |
| 行走 | `Walking` | 64 | 6 | 2 | 8 | 100 ms | **600 ms** |
| 跑步 | `Running` | 128 | 6 | 2 | 8 | 100 ms | **600 ms** |
| 攻击 1 | `Attacking` | 200 | 6 | 2 | 8 | **50 ms** | **300 ms** |
| 攻击 2（未选择） | — | 264 | 6 | 2 | 8 | 50 ms | 300 ms |
| 攻击 3（双手，未选择） | — | 328 | 8 | 0 | 8 | 50 ms | 400 ms |
| 受击 | `Struck` | 472 | 3 | 5 | 8 | 100 ms | **300 ms** |
| 死亡 | `Dying` | 536 | 4 | 4 | 8 | 100 ms | **400 ms** |
| 尸体 | `Dead` | 539 | 1 | 7 | 8 | 1000 ms | 保持 |

扁平攻击 1 的 50 ms 是第 7 轮校准，对齐服务端
`PLAYER_ATTACK_DAMAGE_DELAY_MS = 300`（`HumanObject.cs:3201` 的
`DelayedAction`）；Crystal 帧表写 100 ms，本作有意不同。原生表则严格用
Crystal 的 100 ms。扁平旧路径在 `variant=None` 时默认 Attack1（50 ms）；
一旦攻击事件带 `AttackVariant::Attack1/Attack3`，改用 `HUMAN_ATTACK1_R47`/
`HUMAN_ATTACK3_R47`（100 ms，600/800 ms），只有原生 `select_attack1`
才区分 A1–A4。

女性外观的偏移**不是统一 +512**：`NativeLibraryId` 按域/`altAnim` 分别给身体、
头发、武器、翅膀偏移：

| 身份 | 身体/头发 | 武器 | 翅膀 |
|---|---|---|---|
| C 系（战/道/法，及非 alt 刺客/弓手） | 808 | 416 | 840 |
| 刺客 alt | 512 | 512 | 544 |
| 弓手 alt | 352 | 352 | 352 |
| 男性（任意域） | 0 | 0 | 0 |

偏移加在 `Start + direction * (Count + Skip) + frame` 上，`Start` 本身不含偏移，
`index.json` 也不写偏移。来源是 Crystal `ArmourOffSet/HairOffSet/WeaponOffSet/
WingOffset`（`PlayerObject.cs:445-448,536-539,584-587`），实现在
`NativeLibraryId::{armour_offset,hair_offset,weapon_offset,wing_offset}`。
原生库域按 Crystal `PlayerObject.cs:368-412,457-503,584-587`：

* 战/道/法恒为 C 系（`carmour/chair/cweapon`）；
* 刺客 `altAnim` 才是 `aarmour/ahair`，否则 C 系；
* 弓手 `altAnim` 才是 `ararmour/arhair`，否则 C 系；
* 武器按 `Weapon / 100` 独立选域：普通 `CWeapons`、刺客 `AWeaponsR/L`、
  弓手 `ARWeapons/ARWeaponsS`；身体/头发域随 `altAnim`。翅膀库域也随
  `altAnim`，但帧偏移独立（女性 C 系 840、刺客 alt 544、弓手 alt 352，男性 0）。

`altAnim` 的生产近似：当前协议没有 AttackRange/Stance/Spell/Sneek/DashAttack
等独立动作或技能字段，`PlayerLayout::for_action` 只在刺客/弓手且未骑乘时判定：

* 刺客：有职业武器（`Weapon/100 == 1`）或无武器，且动作 ∈
  `{Standing,Walking,Running,Attacking,Struck,Dying}`；
* 弓手：有职业武器（`Weapon/100 == 2`），且动作 ∈
  `{WalkingBow,RunningBow,AttackRange1/2,Jump}`；`Walking/Running` 在
  `native_for_actor` 里先映射成 `WalkingBow/RunningBow`，所以这两个动作会走
  alt；普通 `Attacking` 映射成 `Attack1`，**反而不满足 alt 集合**，会用 C 系
  身体 + AR 武器库。`AttackRange*` 目前没有协议入口，只留在类型/测试里。
* 战/道/法恒 false；骑乘直接返回 `None`（整体走旧扁平路径）。

Crystal 的集合更大（刺客还含 Stance/Die/Attack1..4/Sneek/Spell/DashAttack，
弓手还含 AttackRange1..2/Jump）；本作没有这些独立动作，用 `Attacking` 统一
承载，所以这是**显式近似**，不是逐项等价。

攻击选招按 `PlayerObject.cs:1055-1075`：默认职业 Shift 且无目标时
`Next(100)>=20 ? Attack1 : Attack3`；刺客 Shift 惰性抽两次，
DoubleSlash 优先且 0 抽（生产恒 false，协议无技能字段），非 Shift 抽一次；
弓手 0 抽 Attack1。帧表和时长都由 `NativeFrameTable` 给出，不再用扁平
600/50/300 常量。`Attack4` 的 `TwinDrakeBlade/FlamingSword` 映射有单测，
但协议没有 Attack4/Spell，生产路径 **PARTIAL**。

播放规则：站立循环 4 帧；走/跑播完回 Standing 第 0 帧；攻击/受击/死亡播完保持
最后一帧；`Dead` 永远停尸体帧（`PlayerObject.cs:2317-2323,2500-2508`）。
原生路径要求身体库存在；**身体库键/库不存在时整个玩家回退旧扁平路径**
（旧路径再按身体层决定是否画占位）。身体库存在但当前动作/方向的身体帧缺失时，
在该层画 28×40 品红占位，**仍留在原生路径**，不跨库回退成 Standing/Attack1；
头发/武器/翅膀单层缺帧时才安静跳过该层。没有原生库或骑乘同样整体回退旧扁平路径。
原生层按 §6.1 的 8 方向层序绘制：普通职业/刺客的主、副手随方向在身体前后切换；
弓手 `WeaponLibrary1=null`，AR 弓只在 `WeaponLibrary2`（Off），所有方向末尾都按
`PlayerObject.cs:4921-4922` 再画一次 `DrawWeapon2`（左组末尾两次、右组身体前后各一次）。
没有盾牌层，扁平路径仍沿用 §6.1 的方向规则。Crystal 对本地 `User` 跳过翅膀，
本作未复刻这一分支，本地玩家也画翅膀。

#### 坐骑与翅膀帧表

坐骑帧表照 `Frames.cs:188-192` 的 `Count/Interval`，`Start` 换算成扁平
`horse_x.wzl` 的块内偏移（Crystal 的 `DrawMount` 会减掉 416，所以那边从 0 开始；
扁平布局把站立放在 0、行走放在 64……）：

| 动作 | Crystal 定义 | 扁平 Start | Count | Skip | Interval |
|---|---|---|---|---|---|
| `MountStanding` | `Frames.cs:188` `Frame(416,4,0,500)` | 0 | 4 | 4 | 500 ms |
| `MountWalking` | `Frames.cs:189` `Frame(448,8,0,100)` | 64 | 6 | 2 | 100 ms |
| `MountRunning` | `Frames.cs:190` `Frame(512,6,0,100)` | 128 | 6 | 2 | 100 ms |
| `MountStruck` | `Frames.cs:191` `Frame(560,3,0,100)` | 192 | 3 | 5 | 100 ms |
| `MountAttack` | `Frames.cs:192` `Frame(584,6,0,100)` | 384 | 6 | 2 | 100 ms |
| 坐骑死亡/尸体 | Crystal 没有这两个动作 | 512 / 576 | 3 / 1 | 5 / 7 | 100 / 1000 ms |

* 行走每方向只有 6 张有效图（Crystal 的 `MountWalking` 写的是 8），按素材实测用 6/2；
* 攻击段在 `horse*.wzl` 的 384 起（Crystal 的 `MountAttack` 相对偏移是 168），
  死亡/尸体段是扁平素材额外提供的，按实测取 512/576。

翅膀帧表取 `Frames.cs:157-185` 每行的 effect 列（`NativeEffectTable`），
和身体表相互独立：

| 动作 | effect Start | Count | Skip | Interval |
|---|---|---|---|---|
| 站立 | 0 | 8 | 0 | 250 ms |
| 行走 | 64 | 6 | 0 | 100 ms |
| 跑步 | 112 | 6 | 0 | 100 ms |
| 攻击 1 | 168 | 6 | 0 | 100 ms |
| 攻击 2 | 216 | 6 | 0 | 100 ms |
| 攻击 3 | 264 | 8 | 0 | 100 ms |
| 攻击 4 | 448 | 6 | 0 | 100 ms |
| 法术 | 328 | 6 | 0 | 100 ms |
| 受击 | 392 | 3 | 0 | 100 ms |
| 死亡 | 416 | 4 | 0 | 100 ms |
| 尸体 | 419 | 1 | 3 | 1000 ms |

注意站立 effect 是 8 帧（身体只有 4 帧），`EffectOffSet = Count + Skip` 也按 effect
自己的 Count 计算。翅膀帧号：
`DrawWingFrame = EffectStart + EffectOffSet × direction + EffectFrameIndex`
（`PlayerObject.cs:762`），再加 `WingOffset`（男性 0；女性 C 系 840、刺客 alt 544、
弓手 alt 352，`PlayerObject.cs:448,539,584-587,5080`）。它与身体 `DrawFrame` 是
两套帧号，不能复用身体帧号。`game-assets/gfx/sprites/player-native/chum-effect-*`
的 index 就是该 effect 表；缺 effect 库/帧时安静跳过该层。

怪物默认表来自 Crystal `Frames.cs:25-31`（站立 4×500、行走 6×100、攻击 6×100、
受击 2×200、死亡 10×100、尸体 1×1000）。扁平 `monN.wzl` 没有 Crystal `.Lib` 的
逐怪物 `Frames` 数据（`MonsterObject.cs:276`），所以按 `mon1.wzl` 图像 0 实测布局
保守映射为：站立 0、行走 64、攻击 128、受击 280、死亡 344、尸体 349；
`Count/Interval` 取 Crystal 默认表，**攻击与死亡段按素材/服务端节奏校准**：
攻击 6×50ms=**300ms**（对齐服务端 300ms 伤害延迟）、受击 2×200ms=400ms、
死亡 6×100ms=600ms。未收录怪物至少能正确播放站立 4 帧 + 走 6 帧；
`!Mon.dat` 的逐怪物精确帧表尚未解析。NPC 只有站立 4×450（`Frames.cs:18-19`），
其它动作回退到站立。

### 6.4 攻击/技能特效与音效（第 7 轮）

#### 攻击挥砍光效

`EntityRenderer::draw_attack_swing`（`mir-render/src/entities.rs`）：只对**玩家**、
动作 `Attacking` 时绘制；6 帧挥砍映射到 3 张光效（0~1 起手、2~3 挥出、4~5 收招），
取自 `magic2.wzl` 第 0~2 帧（`FxKind::Swing`），按方向偏移
（`swing_offset`：上 `(0,-20)`、右 `(24,0)`、左 `(-24,0)`、斜向 ±16/±14）。

#### 技能命中闪光

`ui::spellfx::draw_sprites` 在色块表现层（§5.18）之上再叠精灵：

* `magic.wzl` 前 **10 帧 × 60 ms = 600 ms**（`MAGIC_FX_FRAMES` / `MAGIC_FX_FRAME_MS`）；
* 雷电/雷电术改用 `magic2.wzl` 第 **20~29 帧**（`FxKind::Bolt`）；
* 特效按格居中；缺 `magic*.wzl` 或帧时**静默跳过**（`MapScene::effect_image` 返回 `None`）。

色块层的时长不变：瞬发 `CAST_MS = 700ms`、区域 `AREA_MS = 4000ms`、
自身增益/召唤 `AURA_MS = 1200ms`。

#### `S.ObjectRangeAttack`（143）

`net.rs` 的 `ObjectRangeAttack` 分支：更新实体坐标/朝向 + `Attacking` 动作 +
`SoundId::Swing`；若 `spell != None`，在命中格推一条技能表现事件。没有这个分支时
远程怪开火会被 wildcard 丢弃，玩家只会在 300ms 后看到伤害。

#### 音效映射

| `SoundId` | Crystal `SoundList` | 播放文件 |
|---|---|---|
| `Walk` | `WalkGroundL` | `1.wav` |
| `Run` | `RunGroundL` | `3.wav` |
| `Swing` | `SwingShort` | `50.wav` |
| `Hit` | `StruckBodySword` | `70.wav` |
| `Struck` | `MaleFlinch` / `FemaleFlinch` | `138.wav` / `139.wav` |
| `Die` | `MaleDie` / `FemaleDie` | `144.wav` / `145.wav` |
| `LevelUp` | `LevelUp` | `levelup.wav` |
| `Pickup` | `PetPickup` | `pet_pickup.wav` |
| `Drink` | `ClickDrug` | `108.wav` |
| `Cast` | 技能音 `20000 + Spell × 10` | `M<技能号>-0.wav`（缺省 `M31-0.wav`） |

**节流**：同一文件在 `SOUND_COOLDOWN_MS = 100ms` 内只播一次
（`SoundThrottle`，每文件一张表），避免走路音效糊成一片。

**缺失/无声回退**：音频设备打不开时只 warn 一次，之后所有播放是空操作，窗口与渲染
照常；单个音效文件缺失/解码失败时安静跳过（`warned` 集合保证同类只警告一次）。

#### BGM 切换规则

`app.rs::sync_music` 每帧检查：

1. 优先用服务端 `S.MapInformation.Music` / `S.MapChanged.Music` 的编号
   （`mir_render::music_for_index`）；
2. 编号为 0（示例数据库全部为 0）时按地图文件名/标题兜底
   （`music_for_map`，经典地图有主题曲映射）；
3. 同一个文件已经在播时**不重新起播**；换曲先 `stop_music` 再循环播放
   （Crystal `SoundManager.PlayMusic` 同款）；
4. 没有可用曲目 → 停止；设置面板的「背景音乐」开关关闭时立即停止，
   音量独立（默认 `0.5`，`--music-volume` / `settings.json`）。

### 6.5 渲染接口

* `EntityView` 新增 `action: ActorAction`（默认 `Standing`）、`action_started_ms: u64`、
  `mount: u16`（0 = 没有）与 `wings: u16`（0 = 没有）；
* `EntityRenderer::render_at(..., now_ms)` 用 `ActorAnimation::frame_at(now_ms)` 选帧，
  旧的 `render(..., animation_frame)` 把动画计数按 100 ms/拍换算成时间；
* `MapScene::render_with_entities_at(..., now_ms)` 是窗口用的入口，离线模式仍走
  `MapScene::render`，不受影响；
* 玩家身体/武器/头发/盾牌共用同一个动作帧号；怪物/NPC 的帧号加上图像首帧
  （怪物 `(image%10)*430`、NPC `(image%100)*60`），精确帧缺失时回退站立帧，
  再缺就回退库首帧/占位方块。

## 7. 无窗口端到端测试

`crates/mir-client/tests/live_world.rs`：真实服务端 + 登录 + 走路 + 渲染
`target/client-live.png`。第二条用例 `live_client_walk_run_capture_continuous_frames`
连续 **2 次走 + 1 次跑**，按 6×100ms 逐帧走生产渲染入口
`MapScene::render_with_entities_at_offset`，断言服务端权威坐标/方向、自机屏幕锚点
`(384, 256)`、相机中心 = 起点 `Movement`、地形逐帧位移 = `world_offset` 差，
并写 18 张 PNG + `evidence-manifest.json`。第 45 轮的逐帧对照表、证据批次
（归档 Down / 临时 DownRight / 冻结 Down）、单测动态覆盖边界与四类窗口未实测，
见 [`testing/self-motion/README.md`](testing/self-motion/README.md)。

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

`crates/mir-client/tests/bigmap_panel.rs`：**B 面板无头测试**（不连服务器）。

1. 注入 `S.UserInformation` + `S.MapInformation`（`big_map = 302`）+
   `S.NewMapInfo`（700×700），`UiState::toggle_bigmap()` 模拟 `B`；
2. 渲染整屏到 `target/client-bigmap.png`，断言面板居中（596×460）、面板区域变化
   > 5 万像素、整帧颜色 ≥ 20 种、视区里非黑像素 > 5 万、玩家点（黄）≥ 4 像素；
3. `UiState::escape()` 模拟 `Esc`，断言返回 `EscapeResult::Bigmap` 且面板关闭；
4. 点面板不会走路、也不会锁定目标；缺素材 / `big_map == 0` 时视区是纯色占位 + 地图名。

`crates/mir-client/tests/targeting.rs`：**鼠标目标 + 头顶血条**（注入式，不连服务器）。

1. 注入 `S.UserInformation` + 两只 `S.ObjectMonster`（相邻/隔一格）+ 两条
   `S.ObjectHealth`；鼠标移到怪物格上，`UiState::hover_object` 返回该 object id；
2. 渲染后怪物头顶有 32×4 血条（40% 的怪是黄色 `HEALTH_YELLOW`），悬停格画出
   1px 黄框；输出 `target/client-target.png`；
3. 单击怪物 → `target_object_id` 被设置；双击相邻怪物 → `UiAction::AttackTarget`，
   `net_command()` 等于 `NetCommand::Attack`；
4. 双击远处怪物 → 先 `WorldMove { step: true }`，每 600 ms 一步、共 3 步后放弃；
5. 点背包/小地图面板不会产生走路动作。

`crates/mir-client/tests/path_and_pickup.rs`：**A\* 点地走路 + 地面物品拾取**（注入式）。

1. 点到 4 格外的空地：`on_click_with_path` 返回 `WalkStep`，随后每 600 ms
   `tick_path` 依次发出 4 个 `NetCommand::Walk(Right)`，到达后路径清空；
2. Shift 跑步：连续两格同向时第二步发 `NetCommand::Run`；
3. 目标不可走：退化为 `WorldMove { step: true }`，不留半截路径；
4. 悬停相邻物品：`hover_item` 返回物品名，渲染帧与不悬停时不同；
   单击相邻物品 → `UiAction::PickUp` → `NetCommand::PickUp`；
5. 单击远处物品：先 `WorldMove` 靠近，相邻后 `tick_approach` 自动 `PickUp`；
6. 输出 `target/client-pickup.png`。

`crates/mir-client/tests/mount_wings.rs`：**坐骑/翅膀渲染**（真实素材，缺素材时跳过差异断言）。

1. `EntityView.mount = 2`、`wings = 1`，层计划断言坐骑第一、翅膀在头发之后；
2. 同一玩家「无坐骑/翅膀」与「有坐骑/翅膀」两帧差异 > 200 像素；
3. 输出 `target/client-mount.png`。

`crates/mir-render` 的 `pathfind` 单元测试覆盖：直线、绕墙、不可达、对角不切角、
节点上限/步数上限、启发式与确定性。

`crates/mir-client/tests/panels.rs`：**任务/仓库/邮件/行会四个面板的无头测试**（不连服务器）。

1. 用 `WorldSnapshot::apply_packet` 注入 `S.UserInformation`、`S.NewItemInfo`、
   `S.NewQuestInfo`/`S.ChangeQuest`、`S.UserStorage`/`S.NPCStorage`、`S.ReceiveMail`、
   `S.GuildStatus`/`S.GuildMemberChange`/`S.GuildNoticeChange`/`S.GuildStorageList`/
   `S.GuildBuffList`；
2. 分别渲染整屏到 `target/client-quests.png`、`target/client-storage.png`、
   `target/client-mail.png`、`target/client-guild.png`，断言面板区域相对「只有世界」那一帧
   的像素变化 > 1000、PNG 非空；
3. 命中测试与命令断言：
   * 任务「放弃」→ `NetCommand::AbandonQuest{quest_index}`；完成任务行是绿色；
   * 背包格 → 仓库格 → `NetCommand::StoreItem{from,to}`；仓库格 → 背包格 →
     `NetCommand::TakeBackItem{from,to}`；`S.StoreItem`/`S.TakeBackItem` 回执改动快照；
   * 邮件行 → `ReadMail`；「收取附件」→ `CollectParcel`；「锁定」→ `LockMail`；
     「删除」→ `DeleteMail`；写信页「发送」→ `SendMail{name,message,gold,items_idx,stamped}`、
     「询价」→ `MailCost`；
   * 行会「升职」→ `EditGuildMember{ChangeType=2}`、增益「购买」→
     `GuildBuffUpdate{Action=1}`、资金「存入」→ `GuildStorageGoldChange{Type=0}`、
     背包 → 行会仓库 → `GuildStorageItemChange{Type=0}`、公告「保存」→ `EditGuildNotice`；
4. 小窗口（320×240）下四个布局函数仍然可用（面板宽度/命中矩形收边）。

```powershell
cargo test -p mir-client --test panels -- --nocapture
```

`crates/mir-client/tests/sound.rs`：**音效事件 + 真实 WAV 冒烟**（不连服务器、不开窗口）。

1. 注入 `ObjectWalk`/`ObjectRun`/`ObjectAttack`（带技能）/`ObjectStruck`/`S.Struck`/
   `ObjectDied`/`LevelChanged`/`GainedItem`/`GainedGold`/`NPCResponse`，断言事件顺序、
   `GainedGold(0)` 与空 NPC 页不产生事件、编号严格递增且不重复消费；
2. 本地事件（`push_local_sound`）与队列上限；
3. 100 ms 频控（`SoundThrottle`）与无设备播放器的空操作行为；
4. 真实素材：`resource/wav` 里 12 个映射文件逐个用 `rodio::Decoder` 解码并打印
   采样率/声道（至少 5 个必须成功，缺素材时打印跳过），并确认 12 个事件的文件都能定位。

```powershell
cargo test -p mir-client --test panels -- --nocapture
cargo test -p mir-client --test sound -- --nocapture
cargo test -p mir-client --test minimap_panel -- --nocapture
cargo test -p mir-client --test bigmap_panel -- --nocapture
cargo test -p mir-client --test targeting -- --nocapture
cargo test -p mir-client --test path_and_pickup -- --nocapture
cargo test -p mir-client --test mount_wings -- --nocapture
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

## 8. 未实现与 PARTIAL（R52 H 当前口径）

**R51 / R52 分界（2026-10-26）**：R51 已 **Done / PASS-WITH-PARTIAL**，Blink/StormEscape/FrostCrunch/ThunderStorm/ShoulderDash 五技能已交付。R52/C 已以真实服务端产包完成进程内 World→encode→decode→WorldSnapshot 链路；DeleteItem 按 UUID 消费装备计数/耗尽摘槽，客户端消费 149（ObjectSpell）、125（ObjectProjectile）、26（ObjectRemove）、152/153，并维护 `spell_fields` 视图。152/153 错误预测格 + 已有 motion/队列的敏感回纠已有证据。该 wire 验证不是网络栈/真实 socket 验收；125 无服务端生产者、原生 dash 帧/视觉/声音/时序、Frost 玩家目标及其它既有 PARTIAL 仍保留。

**R52 已交付但有界（不代表完整）**：护身符耗材、PoisonCloud、TrapHexagon 及客户端装备扣数与技能场包消费已有 B1–B4/C 证据。PoisonCloud 的服务端场生成/撤销使用 149/26；125 是 ObjectProjectile，当前没有服务端生产者，不能描述成毒雾撤销链或完整生命周期覆盖。G6 wire 使用进程内真实产包链，不是生产 socket；根装备覆盖不代表嵌套槽等价。Frost 玩家目标 PvP、guild Freezing、原生 dash 视觉/音频/时序、Walking/Walk 借帧、火墙逐对象 ProcessSpell/InTrapRock 仍为 PARTIAL/顺延；详细范围见 [路线图 §五](roadmap.md#五下一步优先级r52-a2026-10-25) 和 [R52 索引](testing/round52/README.md)。本 H 批仅做文档与静态检查，不重跑全量功能门禁。

* **移动插值的边界实测**：自机走/跑插值公式、单测与单方向 live 回归已通过；但
  **四类真实窗口边界（中央/触边/沿边/反向离边）未实测**，≥600ms/跳变无窗口录屏；
  他人玩家与怪物走/跑已做逐帧插值（R46/R49 C），但坐骑帧表仍未按 Crystal
  `MountWalking/MountRunning` 插值；Push/Jump/DashAttack 尚未接入。
  相机中心 clamp 只对越界的异常/回退中心生效，正常落点仍在屏幕中心。**本轮未测 FPS/帧耗时（帧率）**；
  文中性能数字（如 §6.0）仅第 13 轮历史数据，不适用于本轮移动插值/相机跟随改动。证据与冻结批次见
  [`testing/self-motion/README.md`](testing/self-motion/README.md)，后续清单见
  [`client-experience-backlog.md`](client-experience-backlog.md)。
* 武器发光（`WeaponEffect`）、中毒染色与等级特效；坐骑/翅膀已支持基础绘制，
  但坐骑/翅膀的逐外观精确素材映射（`Mount/*.Lib`、`CHumEffect/*.Lib`）尚未接入；
* 原生选招：本地攻击命令按 Crystal `PlayerObject.cs:1055-1075` 走 `select_attack1`，
  可选 A1..A4（刺客 Shift 分支可达 A2/A4）；但旧扁平 `variant` 只写 A1/A3，
  `native_action` 也不进入协议，服务端 `S.ObjectAttack` 没有原生动作字段。
  远端玩家 `net::entity_views` 只把已有 `AttackVariant::Attack1/Attack3` 映射成
  A1/A3，协议没有 Attack4/Spell，`TwinDrakeBlade/FlamingSword` 映射只在单测可达
  （生产 PARTIAL），DoubleSlash 生产恒 false。因此**网络上可见的他人攻击只有
  A1/A3**，不能把本地 A1–A4 选择说成网络全量覆盖。
* 弓手 alt 的 `WalkingBow/RunningBow` 已接入；`AttackRange1–3` 因协议没有独立
  远程动作，仍以 `Attacking` → `Attack1` 近似（PARTIAL）。刺客 `Sneek` 尚未接入；
  双持（`WeaponLibrary2`）与弓手全方向第二遍 `DrawWeapon2` 已按 §6.1 接入。
* **怪物逐图像帧表（R49 B/C，PARTIAL）**：已按 Crystal `.Lib` v3 原生帧表导入 118 个 used
  库，并接入默认 `game-assets` 客户端路径（§6.2）；但 4 个 v2 库（无 `frameSeek`）与约 800 个
  源缺 `.Lib` 的 appearance 未导入，逐帧真实渲染只覆盖 `005/022/039/023`；Blend（117）/
  Reverse/Effect/Sorting 未移植；`!Mon.dat` 精确外观映射仍未解析（§6.2）。缺表/坏表/单动作
  缺表时才回退旧的 `monN` 保守默认（如 `mon1.wzl` 图像 1/2 的受击/死亡段会回退站立帧）。
  R49 当前 gate=PARTIAL，待队长最终全量门禁；
* 装备面板里的角色小预览（面板目前只有 14 个槽的图标 + 名称 + 属性汇总）；
* 鼠标悬停地面物品的拾取提示框（现在只画物品名；`P` 键与点击拾取已可用）；
  大地图点击寻路（`BigMapDialog.cs:612-628`）仍只吃掉点击；
* 大地图面板的 NPC 列表/滚动条/世界地图按钮/「传送到 NPC」/搜索框（`BigMapDialog.cs:101-255,320-432`），
  以及坐标行跟随鼠标（本作显示自己的坐标）；玩家蓝条（`MapObject.cs:500`）暂未画；
* **音效/音乐**：循环音（`LoopProvider`）、怪物各自的 `BaseSound`、按地块选择脚步音、
  按武器选择受击音、按距离衰减、Crystal 的滑杆式音量条 UI（`MainDialogs.cs:2864-2887`；
  本作设置面板改用 ±10% 按钮）、解码缓存与回收
  （Crystal 的 `CachedSound` / `SoundManager.cs:195-223`；本作每次播放现解码，
  短 WAV 够快，但没有缓存与 30 秒回收）；音乐目前只按地图编号/标题选曲，
  没有登录/选角/战斗音乐切换（`LoginScene.cs:29`、`SelectScene.cs:25`）；
* **按键自定义**：设置面板目前只支持 **6 个动作改键**（`client_config::KEY_ACTIONS`：
  攻击/技能/背包/组队/好友/设置；冲突拒绝、持久化到 `client.toml`），其余键固定；
  Crystal `KeyBindSettings.cs` 的完整键表与 `RequireAlt/Shift/Ctrl/Tilde` 修饰键语义未接
  （后续清单 P4）；`F12`/`O` 是固定的面板快捷键；
* **小地图**：NPC 任务感叹号（`MainDialogs.cs:1963-2010`）、两档尺寸/底部按钮
  （邮件/大地图/亮度）、滚轮缩放的「大地图搜索」；大地图面板本体见 §5.7（已实现）；
* **光照/时间**：`S.TimeOfDay{lights}`（协议 index 61）客户端已接，但**服务端未下发**，
  因此不画昼夜光照；`MapDarkLight` 的明暗渲染也未做。天气已接（随 `S.MapChanged.weather`
  显示在 HUD，见 §5.19），但天气粒子特效（雨滴/雪花）没有画；聊天频道切换与物品链接未做；
* **摆摊/拍卖/商城的剩余项**：NPC 市场（寄售/拍卖/搜索/翻页/买断/出价/提前成交/取回）面板已在
  §5.14 实现，商城页签在 §5.16 实现；**独立商城面板 R50-C 已实现**（§5.16.1：
  `S.GameShopInfo` 目录 + `S.GameShopStock` 库存刷新、`Class`/`Category` 筛选、
  `K` 热键、缺图标占位，证据 `target/round50/r50-c/`）。仍未做：
  ①`Category` 的**服务端**过滤（现在是客户端本地分节/下拉，服务端只按类型/名字/职业过滤）；
  ②`K` 改键与 `RequireAlt` 修饰键（固定键）；③面板像素级回归基线（只有单次截图）；
  ④release 包真机往返。挂单页的按类型过滤树与滚动条（本作固定 10 行 + 上下页）也未做；
* **任务**：可接任务列表（目前只能从 NPC 对话页接取）、任务详情/奖励选择/共享/追踪控件、
  已完成任务列表的展示（`S.CompleteQuest` 已进快照，面板只列进行中任务）；
* **仓库**：扩充仓库（`HasExpandedStorage`）、仓库密码、仓库内的金币存取
  （Crystal 的个人仓库本来就没有金币功能）；
* **邮件**：回复预填、黑名单/举报按钮、多行正文输入、邮件过期提示（服务端 30 天清理，
  客户端不显示剩余天数）、主题字段（协议没有）；
* **行会**：新建职位/职位权限编辑/邀请入会（服务端 `C.EditGuildMember{ChangeType=0/3/4/5}`
  已实现，界面未接）、行会投票、攻城/行会战、成员表「地图」列（协议 `GuildMember` 没有地图）、
  行会仓库的滚动分页（112 格一屏画完）；
* **社交**：好友面板没有 Email/Whisper 快捷按钮（用邮件/聊天面板代替）；联盟页签的
  盟友名单是客户端本地记录（服务端无权威盟友包），缺「申请中」状态的服务端回执；
* 装备升级/镶嵌/觉醒等数值系统；
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
   Crystal 的 `.Lib` 在生成时已经做过这一步（`MLibraryV2.cs:419-426`）。坐骑/翅膀是
   RGB565（`.wzl` 没有 alpha），所以 `EntityRenderer` 对这两层走 `remove_black_all`，
   否则会在角色周围留下不透明黑框。
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
    但都取自同一个 `mmap.wzl`（`MLibrary.cs:23`）。本项目的 `ui::minimap` 与
    `ui::bigmap` 共用同一个 `MinimapScene`，图像号分别来自 `MapSnapshot::mini_map` 与
    `MapSnapshot::big_map`。
20. **`mmap.wzl` 的 0 号帧是真实地图图**（600×399），不是「空图」；Crystal 用
    `MapInfo.MiniMap <= 0` 判断「这张图没有小地图」（`MainDialogs.cs:1898-1907`），
    所以客户端把 0 当哨兵值、不去查图库，否则会画出别的地图。
21. **`BigMapDialog` 的点位公式与整图缩放不一致**：地图是整图缩放绘制的
    （`BigMapDialog.cs:676`、`MLibrary.cs:756-776`），对象点却减去 `startPointX/Y`
    （`:678-705`），而自己的点按 `格 × 比例` 画（`:707-710`）——玩家不在图像边缘时
    点位会错位。本作统一按 `格 × 比例` 投影，见 §5.7。
22. **大地图对话框的背景图 `Title.wzl` 不在本仓库素材里**：Crystal 用
    `Libraries.Title` 的 820 号图当对话框底（`BigMapDialog.cs:95-98`），
    `resource/data` 只有 `mmap.wzl`/`prguse2.wzl` 等；本作改用自绘面板
    （尺寸按控件坐标反推 596×460，见 §5.7）。
23. **血条只在 `ShouldDrawHealth()` 为真或刚掉过血时画**：Crystal 的
    `PlayerObject.ShouldDrawHealth`（`PlayerObject.cs:182-190`）只对自己/队友为真，
    `MonsterObject.ShouldDrawHealth`（`MonsterObject.cs:335-341`）只对自己的宠物/队友为真，
    外加 `MapObject.DrawHealth` 的 `HealthTime` 窗口（`MapObject.cs:461-476`）。
    本作以「收到过 `ObjectHealth`/`HealthChanged`」当作画条条件（需求），
    所以怪物掉血后就一直显示血条。
24. **背景音乐编号不在客户端代码里**：Crystal 客户端只接收 `MapInfo.Music`，没有
    地图文件名 → 音乐的静态表；示例数据库 389～463 张图的 `Music` 全为 0。
    本作的 `music_for_map` 兜底（`0`/比奇、`3`/盟重、其它地图）是项目数据，
    正式行为仍以服务器编号 → `SoundList.lst:583-615` 为准。
25. **Crystal 的坐骑 `.Lib` 只有 216 帧**（`Mount/00.Lib`；`DrawMount` 把
    `MountStanding` 的 416 偏移减掉，`PlayerObject.cs:5089`），而扁平
    `horse_x.wzl` 每个外观 640 帧、动作起点也不同（站立 0、走 64、跑 128、
    攻击 384）。`mir-render` 的 `MOUNT_*` 常量按扁平布局填写，`Count/Interval`
    尽量照 `Frames.cs:188-192`，行走帧数按素材实测 6 帧。
26. **`wingeffect_x.wzl` 与 `Frames.cs` 的 effect 列能对上**：站立 8 帧/250 ms、
    行走 64、跑步 112、攻击 168……；翅膀的 `DrawWingFrame` 不是身体的
    `DrawFrame`（`PlayerObject.cs:761-762`），不能拿身体帧号直接索引翅膀。
27. **Crystal 的 A\* 允许斜穿墙角**（`GetNeighbours` 只检查目标格，
    `PathFinder.cs:274-294`）；本作要求对角两侧正交格都可走，宁可绕路也不穿墙。
    另外 Crystal 的 `MaxNodes` 是路径点数上限（`:210-214`），本作拆成
    「搜索节点上限」与「路径步数上限」两个参数。
