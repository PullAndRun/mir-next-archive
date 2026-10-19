# 可玩性自检（playtest）

真实网络客户端的最小化脚本：连接 `127.0.0.1:7000`，按「新玩家 10 分钟能玩到的东西」
逐项打印 `[OK]/[FAIL]/[SKIP]`。

```text
python tools/playtest/playtest.py
python tools/playtest/playtest.py --server 127.0.0.1:7000 --timeout 8
```

* 每步都有超时（`--timeout`，默认 8s）；超时会打印**期望收到的服务端包号/包名**。
* 退出码：0 = 没有失败项（允许 SKIP，例如服务端没下发 NPC/鸡鹿）；1 = 有 `[FAIL]`。
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
| 打鸡/鹿 | `C.Attack`(47) → `S.ObjectDied`(81)/`S.GainExperience`(85) |
| 下线 | `C.LogOut`(9) |

买药（NPC 商店页 `NPCGoods` 102 的完整解析）与「使用技能」（新建角色未学技能）当前标为 SKIP，
下一步可补。

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

## 已知问题：新角色 dead（已修/在修）

在真实服务端（`target/debug/mir-server.exe` + 真实 DB/map 资源）上首次运行时发现：
注册/登录/建角/进游戏/收实体列表均 `[OK]`，但**第一步移动就被服务端拒绝**：
`MoveError::Dead { object_id }`，而 `S.UserInformation` 里 `hp=15`。原因是新角色进入世界时
`Player.dead` 被 tick 判死（`world.rs` 的移动门禁判 `player.dead`），并非脚本协议问题
（同样的 `C.Walk` 包在 `tests/` 的世界内用例可通过）。

**状态（2026-10-19 13:52，最终）：** 两处服务端 bug 已由兵部修复（新角色满血、TownArcher 只锁红名）；
脚本已加真寻路（读 `resource/map/<map>.map` v0 → BFS + 斜向不穿墙角 + 每步重算 + 服务器回拉拉黑）、
`S.UserLocation`(23) 本人坐标、`S.MapChanged`(98) 换图、`S.ObjectWalk`(28) 怪物走位、超时/最近包/HP 诊断。
本轮真服务端复跑：

```
[OK] 连接 / 版本校验 / 注册 / 登录 / 建角 / 进游戏 / 进入世界
[OK] 走到 NPC / 对话
[SKIP] 买药（NPCGoods 商店页解析未做）
[SKIP] 打死一只鸡/鹿（起始视野内只有守卫，不跨图寻路，避免踩传送点换图）
[SKIP] 使用技能（新建角色未学技能）
[OK] 开背包/关闭
[OK] 下线
结果：全部通过（含跳过项），exit 0
```

「打怪」步骤在更早的实验里已能接近并交战（HP 15↔16 波动），但鸡/鹿会走位且远在城外；
跨图追击会踩传送点，脚本选择显式 SKIP。服务端战斗链路由 `tests/skills.rs`（66 条）与
`duo_smoke` 覆盖，本脚本的这一步属锦上添花。
