# 客户端：联机世界与实体渲染

本文记录 `mir-client` 在「离线看地图」之外的联机能力：登录链路、世界快照、输入映射、
实体绘制与无窗口端到端测试。协议细节见 [`protocol/login-flow.md`](protocol/login-flow.md)，
渲染几何见 `crates/mir-render/src/lib.rs` 与 `crates/mir-render/src/geometry.rs` 的模块文档。

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
  ← S.UserInformation                // 自己的对象 ID / 坐标 / HP/MP
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
* `entities`：`ObjectPlayer`、`ObjectMonster`、`ObjectNPC`、`ObjectItem`、`ObjectGold`、
  `NewMapInfo` 的 NPC 列表建立的 `BTreeMap<u32, WorldEntity>`；
* `ObjectWalk/ObjectRun/ObjectTurn/ObjectAttack/ObjectStruck/ObjectDied/ObjectRevived` 增量更新；
* `HealthChanged` 更新自己，`ObjectHealth` 更新怪物血量百分比；
* `Chat`、`ObjectChat` 进聊天记录（最多 100 条）；`NPCResponse` 存对话页，
  `NPCGoods` 记商品数量；`GainedItem/GainedGold` 写一条系统消息并更新金币；
* `ObjectRemove`/`MapChanged` 清理实体；
* `packet_counts` 记录包种类次数，供测试断言。

客户端每批包处理完把快照整体克隆给 UI；UI 侧 `poll_network` 用最新快照重绘。
聊天内容会打印到控制台（没有字体，UI 暂不画聊天面板）。

## 4. 键盘映射

| 按键 | 离线模式 | 联机模式 |
|---|---|---|
| 方向键 / WASD | 平移相机 | 朝该方向移动；方向与当前朝向相同 → `C.Walk`，否则 → `C.Turn` |
| Shift + 方向 | — | 与朝向相同 → `C.Run`（服务端仍校验跑步计数） |
| Space | — | `C.Attack { direction: 当前朝向 }` |
| P | — | `C.PickUp` |
| T | — | 打开聊天输入；回车 `C.Chat`，Esc 取消 |
| Esc | 退出 | 关闭聊天输入 / NPC 对话 |
| S | 截图 | （被「向下」占用） |
| F1 | 调试叠层 | 调试叠层（坐标、实体数、HP/MP） |
| F2 | 截图 | 截图 |
| Q | 退出 | 退出 |

Crystal 客户端的键盘移动不是逐键发 `C.Walk`：`GameScene.CheckInput` 用
`MouseDirection()` 得到方向并解析成 `MirAction.Walking/Running/Standing`
（`Client/MirScenes/GameScene.cs:11522-11553`），随后 `PlayerObject` 在动作开始时发
`C.Walk`、`C.Run` 或 `C.Turn`（`Client/MirObjects/PlayerObject.cs:1313-1334`）。
本客户端没有鼠标寻路和动作缓冲，因此把「方向是否等于当前朝向」直接映射成
Turn/Walk 两种动作；Shift 强制跑。服务端对动作时间与跑步计数的校验保持不变。

## 5. 实体图库映射

扁平素材（`resource/data/*.wzl`）与 Crystal 的「一个外观一个 `.Lib`」不同，因此用文件名
+ 固定步长定位；映射表如下：

| 实体 | 文件 | 图像号 → 帧号 | Crystal 出处 |
|---|---|---|---|
| 玩家 | `hum.wzl` | `appearance*600 + direction*8 + n`（站立 4 帧，步长 8） | 外观选择 `PlayerObject.cs:553`；帧号 `PlayerObject.cs:761`；玩家帧集合 `Frames.cs:157-171`；身体绘制 `PlayerObject.cs:5045` |
| 怪物 | `mon{image/10+1}.wzl` | `(image%10)*430 + direction*8 + n`（站立 4 帧） | 图像 → 图库 `MonsterObject.cs:208`；默认怪物帧集合 `Frames.cs:25-31`；绘制 `MonsterObject.cs:4307-4309` |
| NPC | `npc.wzl`、`npc2.wzl`…（每 100 个图像换库） | `(image%100)*60`，取站立首帧 | 图像 → 图库 `NPCObject.cs:77-80`；帧号 `NPCObject.cs:101`；默认 NPC 帧集合 `Frames.cs:18-19` |
| 物品 | `items.wzl`（缺失时 `dnitems.wzl`） | 帧号 = `ObjectItem.image`；金币按数量用 112..116 | `ItemObject.cs:32,37`（物品）、`:49,55-64`（金币）、`:72-73`（绘制） |

几何锚点：

* 玩家/怪物/NPC：格原点 + 图像头里的 `offset_x/offset_y`（`MLibrary.cs:658-665`，
  `PlayerObject.cs:5045` 的 `offSet: true`）；
* 物品：在格子里居中（`ItemObject.cs:78-79`），不叠加图像偏移（`:72-73` 的默认 `offSet: false`）；
* 绘制顺序：地图 Y 升序、同一行 X 升序（`GameScene.cs:10798-11008`）。
* 精灵缺失：画 12×18 的彩色方块（玩家蓝、怪物红、NPC 绿、物品黄；死亡为灰色），
  物件有 `ObjectHealth` 时额外画 3px 血条。无 `resource/` 的 CI 也能得到非空画面。

### 已知的素材/枚举偏差

本作服务端协议里的 `Monster` 枚举是 Crystal 的**紧凑枚举**（例如 `Scarecrow = 5`、
`Skeleton = 22`），而 `resource/data/mon1..mon53.wzl` 是经典 Mir2/盛大布局（怪物图像按
0..=9、10..=19 分组，对应 `Mon1`、`Mon2`…）。两者不是同一套编号，所以演示怪物可能显示成
同一 `monN` 文件里的另一个怪物外观。`resource/data/!Mon.dat` 里有每只怪的图库与动作帧表，
本模块尚未解析；解析后可以做到逐个怪物精确映射。怪物帧号还做了逐级回退：
精确帧 → 该图像站立帧 → 库首帧 → 占位方块，因为并非每个 `monN.wzx` 都恰好有
`10*430` 帧（例如 `mon1.wzx` 只有 1086 帧）。

## 6. 无窗口端到端测试

`crates/mir-client/tests/live_world.rs`：

1. 用 `MemoryStore` + `ServerConfig { load_game_data: false, demo_monsters: true, map_root: <workspace>/resource/map }`
   在 `127.0.0.1:0` 起真实 `mir_server::serve_forever`；
2. `NetClient` 连接、自动注册、登录、建角、`StartGame`；
3. 断言收到自己的 `UserInformation`、`MapInformation`、至少一个 `ObjectMonster`；
4. 依次尝试八个方向走路，断言至少一次产生 `UserLocation` 且自己坐标变化；
5. 用快照渲染 `800×600` 一帧到 `target/client-live.png`，断言颜色数 ≥ 3、非黑像素 > 100、
   PNG 文件 > 1 KiB。

```powershell
cargo test -p mir-client --test live_world -- --nocapture
cargo test -p mir-client -p mir-render
cargo clippy -p mir-client -p mir-render --all-targets -- -D warnings
cargo fmt --all -- --check
```

## 7. 尚未实现

* 字体、聊天面板、状态栏、背包/装备/技能面板、NPC 对话与商店 UI（当前只打印到控制台/日志）；
* 玩家头发、武器、坐骑、翅膀、光效、中毒/等级特效（`EntityView` 只带一个外观号）；
* 行走/跑步/攻击帧动画（只按动画计数播放站立帧），以及移动插值；
* 鼠标点击寻路、拾取、NPC 对话交互；
* 地图切换后的素材重载（服务端 `MapChanged` 会更新快照地图名，窗口仍用启动时加载的地形）；
* 声音、天气、光照、小地图/大地图；
* `!Mon.dat` 精确怪物外观映射（见上一节）。

## 8. 对拍 Crystal 时发现的怪癖

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
