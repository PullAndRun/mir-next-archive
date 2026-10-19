# 协议级 playtest（`tools/playtest/playtest.py`，第 8/9 轮）

用途：不启动图形客户端，用 Python 直接连真实服务端，按 Crystal 协议把「登录 → 建角 →
进世界 → 走一步 → 看包」跑一遍，用于冒烟与排障。详细说明与覆盖表见
[`tools/playtest/README.md`](../../tools/playtest/README.md)（与本文件保持同一口径）。

## 1. 用法

```powershell
python tools/playtest/playtest.py --gm playtest
python tools/playtest/playtest.py --server 127.0.0.1:7006 --gm playtest --timeout 8
```

* `--server host:port` 直连已有服务端（不自己起服务）；
* `--timeout` 单步等待秒数（默认 8）；
* `--gm <账号>` 用 GM 账号跑完整链路（买药/打怪/跨图都依赖 GM 定位与 `@map` 兜底）；
  **验收口径以 `--gm playtest` 为准**；
* `--start-map`（默认 `0,1` 依次尝试，兼容 map_index 与文件名不一致的数据集）、
  `--start-point`（默认 `330,270`）。

**结果文案**：

* `结果：全部通过（0 跳过）`
* `结果：全部通过（含 N 个跳过项）` + 逐条 `  - [SKIP] 步骤 —— 原因`
* `结果：存在失败项（OK x / SKIP y / FAIL z）` + 逐条 `  - [FAIL] 步骤 —— 原因`

退出码：`0` = 全 OK/合理 SKIP，`1` = 有 FAIL。

**环境（重要，仅限本机测试）**：本机可能接着多台显示器，**不要前台启动 `target\debug\mir-server.exe`**
（会弹控制台窗口到别的显示器，打断使用者）。本地验证一律用隐藏启动器
（用法/产物/`-Stop` 详见 [`tools/dev/README.md`](../../tools/dev/README.md)）：

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File tools/dev/serve.ps1 -Port 7006 -GmAccounts playtest
# 停：
powershell -NoProfile -ExecutionPolicy Bypass -File tools/dev/serve.ps1 -Stop -Port 7006
```

> 这条「窗口放显示器 2」的规则**只约束开发与测试**：`mir-client` 本身不控制窗口位置
> （用 `Window::default_attributes()` 交给操作系统），正式产品在玩家自己的机器上按系统默认
> 摆放；搬运逻辑只存在于 `tools/dev/*.ps1`。

## 2. 覆盖（`--gm playtest`，真实 `Server.MirDB`，15 项全 OK、0 SKIP）

| # | 步骤 |
|---|---|
| 1 | 连接 + `C.ClientVersion` |
| 2 | 注册 / 角色列表 |
| 3 | 登录 |
| 4 | 建角 |
| 5 | 进游戏（`S.UserInformation`） |
| 6 | 进入世界 |
| 7 | 回到起始图（见 §3） |
| 8 | 走到 NPC + 对话 |
| 9 | 买药 |
| 10 | 打怪 |
| 11 | 用技能 |
| 12 | 开背包 |
| 13 | 跨图传送 |
| 14 | 下线 |
| 15 | （GM 回执/包序校验等） |

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

## 4. 当前状态（t76/t88）

- 新角色满血（`World::enter` 兜底，`docs/world/combat.md` §10.4）与 TownArcher 红名过滤
  （`docs/world/monster-ai.md` §1）均已修复；
- `--gm playtest` 在真实 DB 上 15 项全 OK、0 SKIP；连续 3 次 exit 0、输出一致；
- **NPC 商店价格缺口**：`S.NPCGoods` 没有价格字段，脚本只能验证「货架能出 Book/物品」，
  买药的价格校验依赖客户端 `[Trade]` 页（完整说明见 `tools/playtest/README.md`）；
- 真实数据里没有 `ObjectMagic` AI，魔法链路靠合成用例（`tests/skills.rs`）或预留的 AI 215/216/220。

## 5. 与其它测试的关系

playtest 是「真服务端 + 真 socket + Python 客户端」的冒烟，不替代 Rust 集成测试；
它补的是「换个语言实现的客户端也能按协议玩」这一层。
