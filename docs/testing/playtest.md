# 协议级 playtest（`tools/playtest/playtest.py`，第 8/9 轮）

用途：不启动图形客户端，用 Python 直接连真实服务端，按 Crystal 协议把「登录 → 建角 →
进世界 → 走一步 → 看包」跑一遍，用于冒烟与排障。详细说明与覆盖表见
[`tools/playtest/README.md`](../../tools/playtest/README.md)（与本文件保持同一口径）。

## 1. 用法

```powershell
python tools/playtest/playtest.py --gm playtest
python tools/playtest/playtest.py --server 127.0.0.1:7006 --gm playtest --timeout 8
py tools/playtest/playtest.py --self-check
```

* `--server host:port` 直连已有服务端（不自己起服务）；
* `--timeout` 单步等待秒数（默认 8）；
* `--gm <账号>` 用 GM 账号跑完整链路（买药/打怪/跨图都依赖 GM 定位与 `@map` 兜底）；
  **验收口径以 `--gm playtest` 为准**；
* `--start-map`（默认 `0,1` 依次尝试，兼容 map_index 与文件名不一致的数据集）、
  `--start-point`（默认 `330,270`）；
* `--self-check` 不连服务端，只验 `@mapinfo` 传送点行解析（含四种标记）与三种结果文案，
  可放进 CI。

**结果文案**：

* `结果：全部通过（0 跳过）`
* `结果：全部通过（含 N 个跳过项）` + 逐条 `  - [SKIP] 步骤 —— 原因`
* `结果：存在失败项（OK x / SKIP y / FAIL z）` + 逐条 `  - [FAIL] 步骤 —— 原因`

退出码：`0` = 全 OK/合理 SKIP，`1` = 有 FAIL。

**环境（重要，仅限本机测试）**：本机可能接着多台显示器，**不要前台启动 `target\debug\mir-server.exe`**
（会弹控制台窗口到别的显示器，打断使用者）。本地验证一律用隐藏启动器
（用法/产物/`-Stop` 详见 [`tools/dev/README.md`](../../tools/dev/README.md)）：

```powershell
pwsh -NoProfile -File tools/dev/serve.ps1 -Port 7006 -GmAccounts playtest
# 停：
pwsh -NoProfile -File tools/dev/serve.ps1 -Stop -Port 7006
```

> 这条「窗口放显示器 2」的规则**只约束开发与测试**：`mir-client` 本身不控制窗口位置
> （用 `Window::default_attributes()` 交给操作系统），正式产品在玩家自己的机器上按系统默认
> 摆放；搬运逻辑只存在于 `tools/dev/*.ps1`。

## 2. 覆盖（`--gm playtest`，真实 `Server.MirDB`；14 项全 OK、0 SKIP）

真机顺序（3 连跑一致）：**连接/注册/登录 → 建角/进游戏/进世界 → 回到起始图 → 走到 NPC →
买药 → 跨图（自然）→ 打怪 → 使用技能 → 开背包 → 下线**。

| # | 步骤 | 协议/说明 |
|---|---|---|
| 1 | 连接 / 版本校验 | `S.Connected` → `C.ClientVersion` → `S.ClientVersion` |
| 2 | 注册 / 登录 | `C.NewAccount` → `S.NewAccount`；`C.Login` → `S.LoginSuccess` |
| 3 | 建角 / 进游戏 | `C.NewCharacter` → `S.NewCharacterSuccess`；`C.StartGame` → `S.StartGame` |
| 4 | 进入世界 | `S.MapInformation` / `S.UserInformation` / `S.ObjectNPC` / `S.ObjectMonster` |
| 5 | 回到起始图（幂等） | `@map` → GM 回执 `Teleported to …` → `C.LogOut` → `S.LogOutSuccess` → `C.StartGame` |
| 6 | 走到 NPC / 对话 | `C.Walk` → `S.ObjectWalk`；`C.CallNPC` → `S.NPCResponse` |
| 7 | 买药（NPC 商店） | `C.CallNPC` 逐页 BFS → `S.NPCGoods` → `C.BuyItem` → `S.GainedItem` + `S.LoseGold` |
| 8 | **跨图传送（自然）** | `@mapinfo` 附近传送点 → BFS 走过去踩点 → `S.MapChanged`；失败才降级传送 NPC → GM `@map` |
| 9 | 打怪（GM `@mob`） | `C.Attack` → `S.ObjectDied` / `S.GainExperience` |
| 10 | 使用技能 | `@learn` → `S.NewMagic` → `C.Magic` → `S.ObjectMagic` + `C.HealthChanged` |
| 11 | 开背包 / 关闭 | 客户端本地 UI（协议无背包开关包） |
| 12 | 下线 | `C.LogOut` → `S.LogOutSuccess` |
| 13–14 | 其它包序/回执校验 | GM 回执、视野对象、包序检查等 |

**跨图必须在打怪之前**：`@mob` 会刷一群随从把周围格子占满，服务端会拒绝移动
（`target cell is occupied`），之后再想自然走进城门就无路；买药结束时角色正好在药店旁，
离城门 4 格，所以顺序是「买药 → 跨图 → 打怪 → 技能 → 开背包 → 下线」。

不带 `--gm` 时买药、打怪、跨图会 `[FAIL]`（没有 GM 定位与 `@map` 兜底），所以文档/验收
以 `--gm playtest` 为准。

## 3. 幂等与「回到起始图」

同一服务端可连续跑多次，输出稳定。GM 模式下多一行：

```text
[OK] 回到起始图 —— 0 @ (330, 270)，视野 NPC 7 个（本图 NPC 40 个）
```

机制：进世界后 `@map <起始图> <x> <y>` → 校验 GM 回执 `Teleported to …` → **下线再进**
（`C.LogOut` → `S.LogOutSuccess` → `C.StartGame`）重建视野。为什么要重进：GM `@map` 走
`World::place_player`，只改坐标、**不发** `S.MapChanged`/视野对象（服务端既有实现，不是 bug）。

## 4. 自然传送点（t96）

跨图传送的**第一选择是「真走进去」**：GM `@mapinfo` 会追加「附近传送点」段——
`movements: 72 total, 6 on big map (nearest 10)` + 按切比雪夫距离升序最多 10 行
`(326,288) -> 0108(#4) (8,12) dist=18`，带 `[bigmap]`/`[hole]`/`[move]`/`[conquest]` 标记。
为什么要走 GM 拿这份数据：Crystal 的 `S.NewMapInfo` 只带 `ShowOnBigMap` 的 6 条
（`PlayerObject.cs:1009-1056`），脚本按客户端协议看不到「走进城门就换图」的格子；
`@mapinfo` 是**开发者旁路**，只观察数据，换图仍由玩家走进去触发（`maps.rs::check_movement`），
不改变客户端语义。

本步真机输出（3 次连跑一致）：

```text
[OK]   跨图传送 —— 自然传送点 (326, 288) → 0108 (8, 12)（4 格）
```

三级降级：自然传送点 → 传送 NPC（`Teleport_*` 的 `@tele → @move*`）→ GM `@map`（打印降级原因）。
服务端数据与测试见 [`docs/world/maps.md`](../world/maps.md) §3.5。

## 5. 当前状态（t76/t88/t96）

- 新角色满血（`World::enter` 兜底，`docs/world/combat.md` §10.4）与 TownArcher 红名过滤
  （`docs/world/monster-ai.md` §1）均已修复；
- `--gm playtest` 在真实 DB 上 14 项全 OK、0 SKIP；连续 3 次 exit 0、输出一致；
- **三件套全绿（2026-10-19 实测）**：`cargo test -p mir-server` **39 目标全绿**
  （含 `tests/maps` 20/20、`tests/duo_smoke` 7/7）；`cargo clippy -p mir-server --all-targets
  -- -D warnings` 0；`rustfmt --check` 0；
- `--self-check`：`[OK] 自检通过：@mapinfo 传送点行解析 4/4、结果文案 3/3`（不连服务端）；
- **NPC 商店价格缺口**：`S.NPCGoods` 没有价格字段，脚本只能验证「货架能出 Book/物品」，
  买药的价格校验依赖客户端 `[Trade]` 页（完整说明见 `tools/playtest/README.md`）；
- 真实数据里没有 `ObjectMagic` AI，魔法链路靠合成用例（`tests/skills.rs`）或预留的 AI 215/216/220。

## 6. 与其它测试的关系

playtest 是「真服务端 + 真 socket + Python 客户端」的冒烟，不替代 Rust 集成测试；
它补的是「换个语言实现的客户端也能按协议玩」这一层。

## 7. 第 40 轮审计与「审计后追加」证据

- 御史台第 40 轮独立审计：**verdict=partial**（3 findings），全文见
  [`core-loop-audit-round40.md`](core-loop-audit-round40.md)。结论以审计原文为准，本页不重述/不改写。
- 审计截止 18:11；之后新增的两份 live 证据标注为**审计后追加**，不改变审计结论：
  - **PS-1 strict**：重登前后 equipment 2 件、inventory 3 件槽位 `uid/index` 全等，
    `gold=0/completed=[]/spells=[]` → PASS；见
    [`evidence/round26/ps1-strict.log`](evidence/round26/ps1-strict.log)。
  - **DR-1 cjlaaa**：自然死亡 `hp=0` → 12s 无自动复活 → `C.TownRevive(68)` 0.011s 收
    `S.Revived(136)` → 可移动/攻击；**已知差异**：复活落点用配置出生点，Crystal
    `BindMap`/`PKTown` 分支未实现；见
    [`evidence/round26/dr1-cjlaaa.log`](evidence/round26/dr1-cjlaaa.log)。
- 审计后核心闭环**整体仍写部分成立**：升级 live SKIP、EQ 未覆盖 0 耐久、TP 目标选择待改。
