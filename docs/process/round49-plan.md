# 第49轮实施方案（中书省）

> 旨意：`JJC-20261025-001`；角色：`zhongshu`；范围只覆盖本轮 WIP 收口与商城独立面板。
> 原则：Crystal 是唯一行为基准；先证据、后实现；失败写 `PARTIAL`，不得假绿。

## 一、事实依据与取舍

1. `git status --porcelain`：工作区含 `crates/mir-asset/src/{lib.rs,lib_container.rs,library.rs}`、
   `crates/mir-render/src/{animation.rs,entities.rs}`、`crates/mir-client/src/{app.rs,camera.rs,main.rs,net.rs}`、
   `crates/mir-tools/src/main.rs` 等修改，并有 `crates/mir-asset/src/frames.rs`、5 个新测试及
   `docs/testing/{monster-motion,monster-native,native-monster}/` 未跟踪项；另有 R47 攻击变招证据。
2. `git diff --stat`：`45 files changed, 1924 insertions(+), 230 deletions(-)`（未跟踪文件另计）。
   这不是干净基线，必须先冻结 WIP 事实，不能用重置命令覆盖他人变更。
3. `git log -3 --oneline`：`f0be364 feat(render): 原生玩家库选库与 Crystal 选招（R48，第48轮体验优化）`、
   `bfa9a09 feat(client): 战士空挥 Attack1/Attack3 选招（照 Crystal Frames.cs，第47轮体验优化）`、
   `fdef540 docs(process): DSH 插件与技能选型（LSP/质量门禁/DB/会话检索 + 三个技能）`。
4. `docs/testing/monster-native/README.md:81-116` 已记录原生怪物帧表与动作生命期；
   `docs/testing/monster-motion/README.md:1-46` 已记录怪物走/跑插值公式、真实素材证据与未测边界；
   `docs/testing/native-monster/README.md:25-35,81-116` 记录 118 个 v3 库导入及缺源降级。
5. `docs/client.md:1324-1325,1763-1784` 明确本轮 WIP 已接怪物移动/原生帧，但坐骑、Push/Jump/Dash、
   `!Mon.dat` 精确映射仍未完成；`docs/client-experience-backlog.md:12-15` 将其列为 P1/P2。
6. `docs/roadmap.md:47` 明确商城当前只有市场页签，独立面板和 `S.GameShopInfo`/`S.GameShopStock` 未做。
   `crates/mir-server/src/market.rs:63-101` 也明确现实现不发这两个包。
7. Rust 协议结构其实已在 `crates/mir-protocol/src/manual/server.rs:1089-1127` 与
   `crates/mir-protocol/src/packets/server.rs:5959-5986`，线格式分别是 `GameShopItem+i32` 与 `i32+i32`；
   因此本轮重点是下发、消费、刷新和证据，不重写已通过的 432 包协议。
8. Crystal 依据：独立包线格式为 `github/Crystal/Shared/ServerPackets.cs:5348-5391`；登录调用
   `GetGameShop` 为 `github/Crystal/Server/MirObjects/PlayerObject.cs:1203-1206`，实现为
   `:13913-13944`；库存刷新为 `:13703-13721`、购买后 `:13821-13862`。默认怪物帧与插值分别见
   `github/Crystal/Client/MirObjects/Frames.cs:22-32`、`github/Crystal/Client/MirObjects/MonsterObject.cs:345-433`。
9. i18n 基线为 `docs/i18n.md:267-281` 的 `189/22/69/15`，参数化 API 已在
   `docs/i18n.md:681-746` 落地；本轮只为新增商城文案走现有 API，不把 170+ 文案大扫除混入本轮。

**选择理由**：怪物 WIP 已占用未提交工作区，若不先闭环会污染后续素材与渲染判断；商城独立面板
是路线图明确的最小玩家可见增量，协议结构已有、依赖集中在登录包序和既有购买事务，能在一轮内
给出可回归的服务端→客户端因果链。技能、AOI、攻城和全量 i18n 留到后续，避免假绿。

## 二、本轮目标与明确不做

### 可判定目标

- 怪物原生帧/走跑 WIP 的单测、真实素材 ignored 证据、缺素材降级和文档互相一致；未知动作、缺动作、
  缺库都不借用其他动作伪造；现有 WIP 变更可单独提交。
- 登录进世界后，服务端按 Crystal 顺序为每个可用商城商品发送一次 `S.GameShopInfo`；库存按
  角色库存/全服库存和现有持久化计数计算。购买事务成功后发送对应 `S.GameShopStock`，库存不足也刷新该商品；
  事务失败回滚内存副作用且不发送虚假库存。
- 客户端能打开独立商城窗口，消费 `S.GameShopInfo` 建目录，消费 `S.GameShopStock` 更新/移除商品，
  支持 Crystal 已有的分类/职业/新品/促销筛选、搜索、元宝/金币支付和 `C.GameshopBuy`；无商品、断线、
  缺图标、库存为零均有可见状态或稳定占位。
- 新增玩家可见文字全部经现有 `TextKey`/`Label` 参数化 API；专门 i18n 扫描与全量 CI 通过后才可提交。

### 明确不做

- 不做 Blink/ShoulderDash/StormEscape、PoisonCloud/FrostCrunch、地图级技能、护身符、PetSave/PetExp。
- 不做 AOI 全量验收、攻城旗子/控制点/修理、CallDefaultNPC、行会战/行会补全。
- 不做 `!Mon.dat` 全量精确外观映射、坐骑帧表/坐骑插值、Push/Jump/DashAttack、天气/昼夜/音效细分、
  寻路和 UI 缩放；攻击变招只冻结并验收既有 WIP，不新增动作语义。
- 不做全量 170+ i18n 收口；不改无关协议包、数据库迁移、市场页签既有购买事务和 R47/R48 冻结证据。

## 三、子任务 DAG

### R49-01 冻结事实与 WIP 分界（`libu`；前置）

- 路由：`gpt-6.1-sol` 完成 Crystal 对照、清单和证据审计；不写代码。
- write scope：`docs/testing/round49/**`、`docs/process/round49-plan.md`；不得改 `crates/**`。
- 依赖：无。输出 `wip-freeze.json`，列出 tracked/untracked、哈希、既有证据和本轮新增边界。
- 验收：`git status --porcelain`、`git diff --stat`、`git diff --check`；证据
  `docs/testing/round49/wip-freeze.json`，不得声称未运行的测试通过。
- Crystal：`Frames.cs:22-32`、`MonsterObject.cs:345-433`；商城 `ServerPackets.cs:5348-5391`。

### R49-02 怪物原生帧与移动 WIP 收口（`gongbu`；编码）

- 路由：`deepseek` 编码并补测试；write scope：`crates/mir-asset/**`、`crates/mir-render/**`、
  `crates/mir-client/src/{net.rs,camera.rs,app.rs,main.rs}`、对应 `crates/*/tests/**`；不得改服务端玩法。
- 依赖：R49-01。必须保留 `frames.json` 有表但缺动作时安静跳过、无表时旧默认回退、负 stride/stride=0
  的既有语义；缓存解析不能每帧读盘。
- 验收命令：
  `cargo test -p mir-asset --test real_framesets`；
  `cargo test -p mir-render --test monster_native -- --ignored --nocapture`；
  `cargo test -p mir-render --test monster_native_evidence -- --ignored --nocapture`；
  `cargo test -p mir-client --lib camera::monster_motion_tests`；
  `cargo test -p mir-client --test monster_motion -- --ignored --nocapture`；
  `cargo test -p mir-tools --test monster_native_import -- --ignored --nocapture`。
- 证据：更新 `docs/testing/monster-native/`、`docs/testing/monster-motion/`、`docs/testing/native-monster/`，
  汇总到 `docs/testing/round49/monster-wip.json`；真实素材缺失必须记录 `PARTIAL` 与 fallback 原因。
- Crystal：`github/Crystal/Client/MirObjects/Frames.cs:22-32`、`MonsterObject.cs:345-433`。

### R49-03 商城服务端独立包下发与库存刷新（`bingbu`；编码）

- 路由：`deepseek` 编码并补服务端/协议测试；write scope：`crates/mir-server/src/**`、
  `crates/mir-server/tests/**`；仅在线格式不一致时才改 `crates/mir-protocol/**`，不得改迁移。
- 依赖：R49-01；可与 R49-02 并行。登录下发应从 `GameData::game_shop` 和既有 DB 计数读取，不能生成 fixture 商品；
  购买继续复用 `lock_game_shop`、`gameshop_buy`、`flush_gameshop_update`，发送库存包的时机必须与事务结果一致。
- 验收命令：
  `cargo test -p mir-protocol game_shop -- --nocapture`；
  `cargo test -p mir-server gameshop_login -- --nocapture`；
  `cargo test -p mir-server gameshop_stock -- --nocapture`；
  `cargo test -p mir-server gameshop_transaction_failure -- --nocapture`。
  测试必须断言包序、GIndex/StockLevel、个人/全服库存、库存为零、重复登录不重复累加及 DB 失败回滚。
- 证据：`docs/testing/round49/gameshop-server.json`，含线格式 round-trip、登录包序、购买成功/不足/事务失败三条因果链。
- Crystal：`ServerPackets.cs:5348-5391`、`PlayerObject.cs:1203-1206,13703-13721,13821-13862,13913-13944`。

### R49-04 客户端独立商城窗口（`gongbu`；编码）

- 路由：`deepseek` 编码并补 UI/网络测试；write scope：`crates/mir-client/src/**`、
  `crates/mir-client/tests/**`；不得改变 `mir-server` 购买校验。
- 依赖：R49-03。沿现有 `WorldSnapshot`/`UiState`/面板事件模型接包；按 GIndex 去重；StockLevel=0 移除或禁购；
  缺图标走统一占位；支付类型与 `C.GameshopBuy` 字段必须保持现有协议语义；所有新文案使用 `Label`/TextKey。
- 验收命令：`cargo test -p mir-client --test gameshop -- --nocapture`、
  `cargo test -p mir-client --lib gameshop -- --nocapture`。测试覆盖打开/关闭、筛选/搜索、购买、库存刷新、
  零商品、迟到包、断线重连和中英文。
- 证据：`docs/testing/round49/gameshop-client/{open.png,stock-update.png,empty.png}`、
  `docs/testing/round49/gameshop-client.json`；截图必须来自真实 `S.GameShopInfo`/`S.GameShopStock` 流程，不能静态注入目录。
- Crystal：`Client/MirScenes/GameScene.cs:128,153,2042-2046,6679-6695`、
  `Client/MirScenes/Dialogs/GameshopDialog.cs:1-758`、`Client/MirControls/MirGameShopCell.cs:195-227`。

### R49-05 文档、i18n 与跨模块证据（`libu`；非编码）

- 路由：`gpt-6.1-sol` 负责文档、证据一致性和 i18n 审计；不得替编码任务修代码。
- write scope：`docs/world/gameshop.md`、`docs/client.md`、`docs/testing/round49/**`、必要的 `docs/i18n.md`；
  不得擅改 `crates/**`。
- 依赖：R49-02、R49-04。更新“已做/未做/降级/回滚”与 Crystal 行号，保留旧市场页签和 native WIP 的 PARTIAL 边界。
- 验收命令：
  `py tools/i18n/scan_hardcoded.py --player-visible --strict --fail-above 70`；
  `py tools/i18n/scan_hardcoded.py --player-visible --no-test --strict`；
  `py tools/docs/check_links.py`；`git diff --check`。
- 证据：`docs/testing/round49/i18n.log`、`docs/testing/round49/manifest.json`；基线必须仍能解释为
  `189/22/69/15`，新增商城文字不得增加 CI 计数。

### R49-06 只读终审与提交门禁（`xingbu`；终态）

- 路由：`gpt-6.1-sol` 只读审计；队长按流程执行唯一 git 提交，本规划者不提交。
- write scope：代码无；只允许把命令输出保存到 `target/round49/**` 和最终证据清单。
- 依赖：R49-05。验收：`pwsh -NoProfile -File tools/dev/ci.ps1 -KillStale`（一次只一人跑全量门禁）、
  `git diff --check`、`git status --porcelain`、`git diff --stat`，并核对所有证据存在、哈希和
  `PARTIAL` 声明一致。输出 `target/round49/final-gate.log`、`docs/testing/round49/final-manifest.json`。
- 只有 12/12 门禁、协议 round-trip、服务端事务失败恢复、客户端真实包链和 i18n 门禁均有证据时，才允许
  由队长提交；否则保持 `PARTIAL`，不得用 `SKIP` 改成通过。

## 四、风险、回滚与禁止捷径

- WIP 横跨 45 个已跟踪文件且含二进制/未跟踪资产：先保留 R49-01 冻结清单，按子任务生成 scoped patch；
  回滚只反应用该 patch 的反向变更，禁止 `git reset --hard`、`git checkout --` 覆盖既有 WIP。
- 原生素材只有 118 个 v3 库；4 个 v2、约 800 个缺源 appearance 必须降级并明示。缺库回旧扁平路径，
  有表缺动作不跨动作借帧；任何“全量 native”表述都视为门下封驳点。
- 怪物插值可能与相机/动作清理竞态：保留 600ms 落点、切图/受击/死亡/断线清理，并用静止相机与移动相机
  双场景证据；性能异常先关闭真实 ignored 证据，不改生产回退语义。
- 商城包序/重复包风险：服务端登录只发一次，客户端按 GIndex 幂等；协议线格式先 round-trip；
  事务失败不得发 `GameShopStock`，客户端缺少完整目录时显示空态而不是伪造商品。
- 商城库存与角色/全服计数不一致时，以现有 DB 事务和锁顺序为事实；先回滚内存发货/扣款，再保留旧 UI 状态，
  不通过改 DB 或 GM 命令制造成功。
- 验收统一照抄 `docs/process/team-and-roles.md:60-69`：不许假绿；失败必须反馈；验收要证因果链；随机允许、
  作弊不允许；禁止 `@revive`/`@repair`/`@give`/`@teleport`/`@map`/`@mapinfo`/改 DB/强杀冒充正常退出/
  `SKIP`/硬编码坐标；测试辅助不得进生产；全量门禁一次只一人。本轮再加：禁止静态商城 fixture、抓包改包、
  客户端伪造库存、跨动作借帧、吞掉 DB 失败、以旧市场页签冒充独立商城包链。

## 五、模型路由与交付物

- `deepseek`：R49-02、R49-03、R49-04 的实现、测试与必要的编码门禁修复；不自定验收标准。
- `gpt-6.1-sol`：R49-01/R49-05/R49-06 的规划、Crystal 对照、文档、证据审计、风险裁决与最终是否可提交判定；不写实现代码。
- 交付物：本文件；怪物帧/移动代码与测试及三份既有证据目录更新；商城服务端登录/库存包链、客户端独立窗口与测试；
  `docs/world/gameshop.md`、`docs/client.md`、`docs/testing/round49/` 的 manifest/log/截图；i18n/链接/CI 门禁记录；
  最终由队长在所有证据通过后完成一个可追溯提交。
