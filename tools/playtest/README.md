# 可玩性自检（playtest）

真实网络客户端的最小化脚本：连接 `127.0.0.1:7000`，按「新玩家 10 分钟能玩到的东西」
逐项打印 `[OK]/[FAIL]/[SKIP]`。

```text
python tools/playtest/playtest.py
python tools/playtest/playtest.py --server 127.0.0.1:7000 --timeout 8
python tools/playtest/playtest.py --server 127.0.0.1:7000 --gm playtest --timeout 8
```

* 每步都有超时（`--timeout`，默认 8s）；超时会打印**期望收到的服务端包号/包名**。
* 退出码：0 = 没有失败项（允许 SKIP，例如服务端没下发 NPC/鸡鹿）；1 = 有 `[FAIL]`。
* 末行按**实际计数**输出：`结果：全部通过（0 跳过）` 或 `结果：全部通过（含 N 个跳过项）`
  （后者会把跳过项逐条列出来）；失败时输出 `结果：存在失败项（OK x / SKIP y / FAIL z）`
  并列出失败项。
* **幂等**：同一服务端可以连续跑多次。跨图传送那一步会把角色留在目标图（本数据集是
  `D011`），所以 GM 模式下进世界后先 `@map` 回起始图 → **下线再进**（`@map` 走的是
  `World::place_player`，只改坐标不发世界包，必须重新 `StartGame` 才能拿到新图的
  `MapInformation`/`NewMapInfo`/`S.ObjectNPC`）→ 校验落点后继续。非 GM 模式每次都注册
  新账号 + 建新角色，天然幂等。起始图/坐标可用 `--start-map`（默认 `0,1`，逗号分隔依次
  尝试，兼容「map_index 与文件名不一致」的数据集）与 `--start-point`（默认 `330,270`）
  覆盖；回不去就报清楚并退出，不静默继续。
* 只依赖 Python 标准库；协议实现见 `crates/mir-protocol`（帧 = u16 总长 + i16 包号 + 负载，
  Compressed 包为 gzip）。

## 当前覆盖

| 步骤 | 协议 |
|---|---|
| 连接 / 版本校验 | `S.Connected`(0) → `C.ClientVersion`(0) → `S.ClientVersion`(1) |
| 注册 / 登录 | `C.NewAccount`(3) → `S.NewAccount`(4)，`C.Login`(5) → `S.LoginSuccess`(9) |
| 建角 / 进游戏 | `C.NewCharacter`(6) → `S.NewCharacterSuccess`(11)，`C.StartGame`(8) → `S.StartGame`(14) |
| 进入世界 | `S.MapInformation`(17)、`S.UserInformation`(21)、`S.ObjectNPC`(92)、`S.ObjectMonster`(71) |
| 走到 NPC / 对话 | `C.Walk`(11) → `S.ObjectWalk`(28)；`C.CallNPC`(50) → `S.NPCResponse`(93) |
| 回到起始图（幂等） | `C.Chat`(`@map`) → GM 回执 `Teleported to …` → `C.LogOut`(9) → `S.LogOutSuccess`(58) → `C.StartGame`(8) → `S.StartGame`(14) |
| 打鸡/鹿 | `C.Attack`(47) → `S.ObjectDied`(81)/`S.GainExperience`(85) |
| 买药（NPC 商店） | `C.CallNPC`(50) 逐页 BFS → `S.NPCGoods`(102) → `C.BuyItem`(51) → `S.GainedItem`(66) + `S.LoseGold`(68) |
| 使用技能 | `@learn` → `S.NewMagic`(117) → `C.Magic`(58) → `S.ObjectMagic`(120) + `C.HealthChanged`(77) |
| 跨图传送（自然） | GM `@mapinfo` 的「附近传送点」→ BFS 走过去踩上传送点 → `S.MapChanged`(98)；失败才降级「传送 NPC」→「GM `@map`」 |
| 下线 | `C.LogOut`(9) → `S.LogOutSuccess`(58) |

当前真服务端（真实 `Server.MirDB` + `--gm`）跑满 **14 项全 `[OK]`、0 `SKIP`**；不带 `--gm`
时会退回普通玩家链路（打怪/技能按原因 SKIP）。

### 自然传送点（t96）

跨图传送的**第一选择是「真走进去」**：GM `@mapinfo` 现在会追加一段「附近传送点」——
`Movements (72 total, 6 on big map)` + 按距离升序的最多 10 条
`(326,288) -> 0108(#4) (8,12) dist=18`（`[bigmap]`/`[hole]`/`[move]`/`[conquest]` 标记）。

为什么要走 GM 拿这份数据：Crystal 的 `S.NewMapInfo` 只带 `ShowOnBigMap` 的 6 条
（`Server/MirObjects/PlayerObject.cs:1009-1056`，客户端画大地图图标用），脚本按客户端协议
根本看不到「走进城门就换图」的格子。`@mapinfo` 是**开发者旁路**：只观察数据，换图仍然由
玩家自己走进去触发（服务端 `maps.rs::check_movement`），不改变客户端语义。

本步现在输出：
```
[OK]   跨图传送 —— 自然传送点 (326, 288) → 0108 (8, 12)（4 格）
```
（打怪会 `@mob` 刷出一群跟屁虫把周围格子占满，所以跨图这一步排在**买药之后、打怪之前**；
买药结束时人正好在药店旁，离 (326,288) 的城门 4 格。）

### 脚本自检

```text
py tools/playtest/playtest.py --self-check
# [OK] 自检通过：@mapinfo 传送点行解析 4/4、结果文案 3/3
```
不连服务端，只验 `@mapinfo` 传送点行的解析（含 `[bigmap]`/`[hole]`/`[move]` 标记）与三种结果文案。

## GM 闭环：打怪 + 用技能（t78）

服务端启动时把账号加入 GM 名单，并用 `--gm` 让脚本走 GM 闭环：

```powershell
$env:MIR2_GM_ACCOUNTS = "gmtest"          # 逗号分隔，可多个
target\debug\mir-server.exe
python tools/playtest/playtest.py --gm gmtest --timeout 15
```

脚本会：注册/登录 `gmtest`（密码固定 `gm12345`，不存在会自动注册）→ `@level 30` → `@heal`
→ `@mob Hen 5`，用 `C.Attack` 打相邻 Hen 直到 `S.ObjectDied`(81) + `S.GainExperience`(85)
→ `@learn FireBall`（`S.NewMagic` 117，spell=31）→ `@mob Hen 1` 取聊天回复里的
`object ids: [...]` → `C.Magic{FireBall, target_id, location=0}` → 断言 `S.ObjectMagic`(120)
且 MP 下降（`S.HealthChanged` 77 的 mp 字段）→ `[OK] 打死一只鸡/鹿` + `[OK] 使用技能`。

不带 `--gm` 时仍只跑普通玩家链路（打怪/技能按原因 SKIP）。

## 历史问题（已修复，留档）

**新角色进世界即被判死**（`MoveError::Dead`，而 `S.UserInformation` 里 `hp=15`）：原因是
新角色进入世界时 `Player.dead` 被 tick 判死（`world.rs` 的移动门禁判 `player.dead`），
不是脚本的协议问题。已由兵部修复（新角色满血、TownArcher 只锁红名）。

脚本随后补齐了真寻路（读 `resource/map/<map>.map` v0 → BFS + 斜向不穿墙角 + 每步重算 +
服务器回拉拉黑）、`S.UserLocation`(23) 本人坐标、`S.MapChanged`(98) 换图、`S.ObjectWalk`(28)
怪物走位、超时/最近包/HP 诊断，以及买药、使用技能、跨图传送三步。

## 本轮真服务端复跑（`--gm`，同一服务端连跑 3 次，每次 exit 0）

```
[OK]   连接服务端 / 版本校验 / 注册账号 / 登录 / 创建角色 / 进入游戏 / 进入世界
[OK]   回到起始图 —— 0 @ (330, 270)，视野 NPC 7 个（本图 NPC 40 个）
[OK]   走到 NPC / 对话 —— Premium_Elijah
[OK]   买药 —— Alchemist_Samuel（@BuySell 页）买模板 #658 x1，花费 40 金币，货架倍率 1.00
[OK]   打死一只鸡/鹿 —— GM @mob Hen（ObjectDied 81 + GainExperience 85 x1）
[OK]   使用技能 —— FireBall 命中（MP 260 -> 251）
[OK]   开背包/关闭 —— 客户端本地 UI（协议无背包开关包）
[OK]   跨图传送 —— 0 (371, 315) → map#34 D011 @ (100, 100)（GM @map 兜底：最近的自然传送点 (324, 41) 距起点 274 格（超过 60 格预算））
[OK]   下线
结果：全部通过（0 跳过）
```

第 2/3 次进世界时角色还停在上一次的 `D011`（`进入世界 pos=(100, 100)`），
`[OK] 回到起始图` 会把它拉回比奇再继续，所以三次输出完全一致。

## 已知缺口（不影响自检）

* `S.NPCGoods` 只有 `Vec<UserItem>`，**没有价格字段**（Crystal 的 `ClientGoodsInfo.Price`
  未建模），脚本只能按已知最便宜药水模板 `#658` 选、再用 `S.LoseGold` 反查成交价。
* GM `@map` 走 `World::place_player`，只改坐标、不发 `S.MapChanged`/视野对象；
  脚本因此用「下线再进」重建视野（见上文幂等说明）。
