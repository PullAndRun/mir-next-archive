# 第50轮实施方案（中书省）

- 旨意：`JJC-20261025-002`（太子建单，六部尚书办）；规划：中书省 `gpt-6.1-sol`。
- 范围：独立商城（Crystal `GameshopDialog`）服务端真实包链 + 客户端独立面板 + 素材 manifest；R49 P01–P08 遗留；README/证据索引门禁行刷新。
- 基准：`github/Crystal` 只读；本作与 Crystal 的语义差异必须在此列明并留证。
- 原则：先证据后实现；无证据写 `PARTIAL`；全量门禁与 git 只由队长执行；六部不自签、不改历史日志。

## 一、事实依据与取舍

### 1.1 Crystal 侧（本轮逐处实读）

1. **登录即下发**：`Server/MirObjects/PlayerObject.cs:1203-1206` 登录链在 `CheckConquest()` 后调 `GetGameShop()`（`:1205`）。
2. **目录包** `GetGameShop`（`PlayerObject.cs:13913-13944`）：逐条遍历 `Envir.GameShopList`；`Stock!=0` 时按 `iStock` 从 `Info.GSpurchases`（否则 `Envir.GameshopLog`）取 `purchased`，**仅当 `Stock-purchased>=0`** 才发 `S.GameShopInfo{Item, StockLevel=Stock-purchased}`（`:13933-13937`，为负则整条不发）；`Stock==0` 也发一条，`StockLevel=item.Stock`（即 0，`:13941`）。
3. **库存包** `GameShopStock`（`PlayerObject.cs:13703-13723`）：`Stock-purchased>=0` 才发 `S.GameShopStock{GIndex, StockLevel}`（`:13720`）。**Crystal 此处把 `item.Info.Index` 当 `GIndex` 发**，与自身读取键不一致——本作按 GIndex 统一（`docs/world/gameshop.md` §8.1 已裁决）。
4. **购买** `GameshopBuy`（`PlayerObject.cs:13725-13911`）：数量 1..99 裸返回；找不到商品回 `YouBuyItemNotInShop`；`Quantity*Count/StackSize>5` 裸返回；限量不足回 `YouBuyMoreThanAvailable` **并刷新库存包**（`:13776-13777`）；`PType` 0=元宝 1=金币，其他回 `YouDontHaveEnoughCurrency`（`:13791-13814`）；扣款后 `cost!=0` 才发 `S.LoseCredit/Gold`（`:13826-13836`）；个人计数只在 `iStock&&Stock!=0` 推进、全服计数**无条件**推进（`:13838-13859`）；`Stock!=0` 才刷库存（`:13861`）；发货一律商城邮件 `Sender="Gameshop"`（`:13900-13910`）+ 聊天提示。
5. **包格式**：`Shared/ServerPackets.cs:5348-5369` `GameShopInfo{GameShopItem, i32 StockLevel}`、`:5371-5394` `GameShopStock{i32 GIndex, i32 StockLevel}`、`Shared/ClientPackets.cs:2434-2454` `GameshopBuy{i32 GIndex, u8 Quantity, i32 PType}`。（简报给的 `:1564` 实为 `MarketSearch`，以实测 `:2434` 为准。）
6. **客户端目录**：`Client/MirScenes/GameScene.cs:153` 静态列表；`:6678-6684` 收到目录包时 `Stock=StockLevel` 入列、7 天内点亮 `New`；`:6686-6697` 收到库存包时**`StockLevel==0` 直接移除条目**、否则改库存并刷新面板。
7. **面板**：`Client/MirScenes/Dialogs/GameshopDialog.cs`（763 行）——职业筛选 `ClassFilter`（`:26`，打开默认=玩家本职业 `:508`）、分区 `SectionFilter`（`:28`，全部/热卖/促销/新品）、搜索按 `Info.FriendlyName` 小写包含（`:655`/`:704`）、分类去重（`:648-680`）、过滤分页 `:700-760`（8 格 2×4、按名字排序、页码）、支付单选 `:474-497`。
8. **格子**：`Client/MirControls/MirGameShopCell.cs:264-283` 名字截 17 字/按品质着色/单价×数量/库存 `>=99→"99+"`、`==0→"∞"`、否则数字；`:189-238` 购买前校验币种与余额、确认框后发 `C.GameshopBuy`；`:285-299` 画图标。
9. **入口**：`Client/MirScenes/Dialogs/MainDialogs.cs:186-197` 主界面商城按钮，提示绑定 `KeybindOptions.GameShop` 键位。
10. **素材授权**：`docs/assets/licensing.md` §1（2026-10-20 所有者裁决：`resource/` 内资源全部可用，仅覆盖 `resource/`；源码/文档仍 GPL-3.0-or-later）。

### 1.2 Rust 侧（当前有什么 / 缺什么）

| 层 | 已有（证据） | 缺 |
|---|---|---|
| 协议 | `GameShopInfo`（`crates/mir-protocol/src/manual/server.rs:1089-1130`，INDEX 250）、`GameShopStock`（`crates/mir-protocol/src/packets/server.rs:5963-5985`，INDEX 251）、round-trip `manual/server.rs:2225-2229`、包路径编解码 `data/game_shop_item.rs` | 无服务端发送、无客户端接收 |
| 世界层 | `World::gameshop_buy` `crates/mir-server/src/market.rs:2433`、`rollback_gameshop_purchase` `:2657`、`lock_game_shop` `:2759`、`flush_gameshop_update` `:2847`、`GameShopPurchases::remaining` `:262-307`（None=不限量） | **不发 `S.GameShopInfo`/`S.GameShopStock`**（`market.rs:96-98` 自述）；无登录目录下发、无成功/失败后库存刷新 |
| 会话层 | `on_gameshop_buy` `crates/mir-server/src/session.rs:3047`、`flush_gameshop` `:3170`、计数读取 `:3157`、登录链 `:1179-1290`（`UserInformation` 在 `:1211`） | 目录下发锚点未插；`GameShopReply`（`:91-95`）无「库存刷新」语义 |
| 客户端 | `NetCommand::GameShopBuy` `crates/mir-client/src/net.rs:660-669`→`ClientPacket::GameshopBuy` `:4249-4260`；市场面板 `ui/market.rs`（`stock_text` `:549-559` 对商城只显示「在售」占位） | **`GameShopInfo`/`GameShopStock` 在 `crates/mir-client/src/` 命中 0**：无接收、无目录缓存、无独立面板 |
| i18n | CI 双棘轮 `tools/dev/ci.ps1:135`（70）/`:136`（15）；TextKey `crates/mir-core/src/i18n.rs:90` | 15 处未归零：`market.rs:559-563`（5）、`monster.rs:695,956,970,980,995,1006,1021,1032,1049`（9）、`server.rs:114`（1） |
| 素材 | `resource/` 全量可用政策已落 `docs/assets/licensing.md` §1 | 商城无 `GIndex→图标/名称/价格/分组` 可复核 manifest |
| R49 证据 | 四证据目录 + `docs/testing/round49/round49-xingbu-final-audit.md`（P01–P08） | P02 三哈希错配、P03 两张历史 PNG 不存在、P04 位级不确定记录方式、P07 全库无产物 |

### 1.3 取舍

协议与线格式已与 Crystal 一致，**不重写协议**；只补「谁在何时发、发什么值、失败时不发」的服务端行为与客户端消费。独立面板与市场页签共用 `MarketItemType::GameShop` 的过滤代码但**不共用状态**，避免页签回归。素材只用 `resource/` 现有图库与 `mir2_dev` 索引，不新造图标。**库存零的可见状态**：Crystal 收到 `StockLevel==0` 直接从列表移除（`GameScene.cs:6692`）；本作保留条目显示「售罄」并禁用购买（让玩家能解释为何买不到），该差异记入 `docs/world/gameshop.md` §7.3，且服务端只对限量商品发 0，不限量商品的 0 只出现在登录目录包里。

## 二、本轮可判定目标（每条可验收、可证伪）

- **R50-A 门禁行刷新**：`README.md` 第 49 轮行（刑部定位 `:74`）与 `docs/testing/round49/README.md`（`:5`、`:57`）由 `current_gate=PARTIAL/待队长门禁` 改为 `PASS + 门禁日志 sha256 + commit d5707c6`，数字全部引用 R49 实测值（`5d475f4f…`、`d5707c6`、182,941 B、exit 0、12 项）。验收 A1/A2。
- **R50-B 服务端真实包链**：登录按 Crystal 顺序每商品一条 `S.GameShopInfo`（`Stock!=0` 发 `Stock-purchased`，为负不发；`Stock==0` 发 `StockLevel=0`）；购买成功按 `GameShopStock` 刷新本人视图；个人/全服计数与 Crystal 同键（GIndex）；事务失败回滚内存且**不发任何新库存包**；客户端 GIndex 幂等。验收 B1–B4。
- **R50-C 客户端独立面板**：分类/职业/新品/促销筛选、搜索、元宝/金币双币、`C.GameshopBuy`、空态/断线/缺图标/库存零可见状态；与市场页签共存不冲突。验收 C1–C4。
- **R50-D 素材授权与哈希 manifest**：`GIndex→图标/名称/价格/页签分组` + `source_path/bytes/sha256/license`；未核验素材只许 `PARTIAL` 占位并明文标注。验收 D1–D2。
- **R50-E i18n no-test 15→0**：上述 15 行改走 TextKey/Label 参数化 API；CI 阈值 `15→0`；`docs/i18n.md` §7 同步，**不写 0 除非实测 0**。验收 E1–E3。
- **R50-F R49 P02/P03/P04 证据元数据收口**：同一快照重建 monster-native `batch/client_binary/source_blobs` 或写降级结论；`self_moving/landed` 降级结论；attack PNG 位级非确定的记录方式（语义摘要为准）；中间 manifest 入库体积策略。验收 F1–F3。
- **R50-G 看板清理（低优先，可裁）**：2026-10-20~22 停滞旧任务（口径称 25 条，**待 R50-01 用 `list --all` 实测**）按实测清单归档/关闭，只改状态与备注，不改标题与历史事实。验收 G1。
- **明确不做**：不改协议线格式；不把市场页签当独立面板；不做商城邮件「一键领取」新交互（已有领取链路）；不动 R47/R48 历史日志与 PNG。

## 三、子任务 DAG

| ID | 承接（部门/模型） | 依赖 | Crystal 出处 | 写范围（互不重叠） | 验收 | 证据 |
|---|---|---|---|---|---|---|
| R50-01 实证复核 | 礼部 `libu` / `gpt-6.1-sol` | 无 | `PlayerObject.cs:1203-1206`、`:13703-13944`；`GameScene.cs:6678-6697` | `target/round50/facts/**` | F0 | `target/round50/facts/facts.json` |
| R50-A 门禁行刷新 | 礼部 `libu` / `gpt-6.1-sol` | 01 | 无（R49 门禁原始日志） | `README.md`（仅第 49 轮行）、`docs/testing/round49/README.md`（仅追加「门禁结果」节）、`docs/roadmap.md` §六 | A1、A2 | `target/round50/r50-a.log` |
| R50-B 服务端包链 | 兵部 `bingbu` / `deepseek-flash` | 01 | `PlayerObject.cs:13913-13944`、`:13703-13723`、`:13725-13911` | `crates/mir-server/src/{session.rs,market.rs}`、`crates/mir-server/tests/{gameshop,gameshop_wire,gameshop_failure}.rs` | B1–B4 | `target/round50/r50-b/**` |
| R50-C 客户端面板 | 工部 `gongbu` / `deepseek-flash` | B | `GameshopDialog.cs:26-28,474-497,508,648-760`；`MirGameShopCell.cs:189-299`；`GameScene.cs:153,6678-6697`；`MainDialogs.cs:186-197` | `crates/mir-client/src/{net.rs,app.rs,ui/gameshop.rs,ui/mod.rs}`、`crates/mir-client/tests/{gameshop_panel,round2_panels}.rs` | C1–C4 | `target/round50/r50-c/**` |
| R50-D 素材 manifest | 户部 `hubu` / `deepseek-flash` | 01 | `GameshopDialog.cs:700-760`（分组/排序）；`MirGameShopCell.cs:294-299`（图标） | `tools/assets/gameshop_manifest.py`、`docs/assets/gameshop-manifest.json` | D1、D2 | `target/round50/r50-d/**` |
| R50-E i18n 归零 | 兵部 `bingbu` / `deepseek-flash` | B（串行：同改 `market.rs`） | 无（本作口径，见 `docs/i18n.md` §7） | `crates/mir-core/src/i18n.rs`、`crates/mir-server/src/{market.rs,monster.rs,server.rs}`、`tools/dev/ci.ps1` | E1–E3 | `target/round50/r50-e/**` |
| R50-F 证据重跑 | 刑部 `xingbu` / `deepseek-flash` | 01 | 无（R49 P02/P03/P04） | `target/round50/monster-native/**`（只写新目录） | F1–F3 | `target/round50/monster-native/**` |
| R50-H 文档终稿 | 礼部 `libu` / `gpt-6.1-sol` | B、C、E、F | 全部已核出处 | `docs/world/gameshop.md`、`docs/client.md`、`docs/i18n.md`、`docs/assets/licensing.md`、`docs/testing/round50/**` | H1、H2 | `target/round50/r50-h.log` |
| R50-G 看板清理（可裁） | 尚书省 `shangshu` / `gpt-6.1-sol` | 无 | 无 | 仅看板（无文件） | G1 | 看板 `show` 原文 |
| R50-T 只读终审 | 刑部 `xingbu` / `gpt-6.1-sol` | 队长全量门禁 | 无 | `docs/testing/round50/round50-final-audit.md` | T1、T2 | 该报告 + 门禁日志 |

**验收命令原文（每项都必须保存命令、cwd、起止、原始退出码、实际计数、工作树指纹、SHA256）**

```text
F0  py target/round50/facts/verify.py docs/process/round50-plan.md
    期望：facts ok: <N>（N=本文件内 `路径:行` 引用数），退出 0；任一行号越界/文件不存在则退出 1
A1  git show --stat --oneline d5707c6 ; grep -c "current_gate=PARTIAL" README.md docs/testing/round49/README.md
    期望：第一行含 d5707c6 与 119 files；两条 grep 计数均为 0；再 grep -c 5d475f4f ≥1、d5707c6 ≥1
A2  git diff --stat ; git diff --check
    期望：仅 3 个文件、无空白错误，退出 0
B1  cargo test -p mir-server --test gameshop
    期望：`test result: ok`，0 failed，退出 0（既有 15 例不回归）
B2  cargo test -p mir-server --test gameshop_wire
    期望：test result: ok；覆盖「Stock!=0 发剩余/为负不发/Stock==0 发 0/成功刷新/失败零包/回滚不刷」六断言，退出 0
B3  cargo test -p mir-server --test gameshop_failure ; cargo test -p mir-protocol game_shop
    期望：均 test result: ok，退出 0（协议线格式未变）
C1  cargo test -p mir-client --lib ui::gameshop
    期望：test result: ok，退出 0
C2  cargo test -p mir-client --test gameshop_panel ; cargo test -p mir-client --test round2_panels
    期望：两条均 ok，市场页签用例不回归，退出 0
C3  cargo test -p mir-client --lib ui::labels
    期望：ok，退出 0（新增标签中英齐全）
D1  py tools/assets/gameshop_manifest.py --check
    期望：`manifest ok: <N>`，每条含 icon/name/credit/gold/category/source_path/bytes/sha256/license；缺项打印 `partial: <k>` 且退出 1（除非显式 --allow-partial）
D2  py tools/assets/gameshop_manifest.py --verify-icons
    期望：`icons ok: <N>`，退出 0；缺图标列入 PARTIAL 清单，不伪造
E1  py tools/i18n/scan_hardcoded.py --player-visible --no-test --strict --fail-above 0
    期望：`0`，退出 0（阈值改动前的实测证据必须与改后一致）
E2  py tools/i18n/scan_hardcoded.py --player-visible --strict --fail-above 70
    期望：≤70，退出 0
E3  py tools/i18n/verify_text_keys.py ; cargo test -p mir-core ; cargo test -p mir-core embedded_pack_matches_builtin_text
    期望：均退出 0
F1  cargo test -p mir-render --test monster_native_evidence -- --ignored --nocapture
    期望：退出 0；raw-manifest/client_binary/source_blobs 三哈希同一快照，写入 target/round50/monster-native/
F2  cargo test -p mir-client --test monster_motion -- --ignored --nocapture
    期望：退出 0；只写 target/round50/，不改 docs/testing/monster-motion/ 历史产物
F3  Get-FileHash 4 张实存 PNG + 两次 attack 生成的语义摘要比对
    期望：4 文件存在且哈希落盘；语义摘要一致、像素差按帧登记（不写“0 diff”）
G1  node .../edict.mjs list --all ; node .../edict.mjs show <ID> ; node .../edict.mjs validate
    期望：清单条数与实测一致；每次 state 有 confirm；validate 退出 0
H1  py tools/docs/check_links.py ; git diff --check
    期望：链接数 ≥ 670 且无断链，退出 0
H2  grep 每个「已实现」断言必须指向本文件 §三 的证据路径
    期望：零条无证据断言（由 T1 复核）
T1  git status --porcelain ; git diff --stat ; 复核门禁日志 SHA256 与 12 项
    期望：差异全部在允许清单内；日志 sha256 = 本次值；不得固定 12/12
```

**统一契约与黑名单**

- 先 `recall <父ID>`、`recall <自身子ID>`、`progress`、`todo`；`--as` 用真实角色；同一子单最多一个 in-progress；六部不 `state`/不验收。
- 禁止：`@revive/@repair/@give/@teleport/@map/@mapinfo` 造状态、改 DB、伪造库存、把静态 fixture 当真实链路、`SKIP`、缺素材 `return Ok`、硬编码坐标、抓包改包、用旧日志/旧截图冒充本次绿、固定“12/12”。
- 证据写 `target/round50/**`；单文件 ≤1 MiB；`docs/testing/round49/**` 除 A 的显式单文件追加外只读。
- B 与 E 都动 `market.rs` → 强制串行（E 在 B 交付并释放后启动）；C 与 B 不同 crate 可并行；D 与 C 只经 manifest 文件解耦。

## 四、门禁与提交策略

1. **第一次提交（A）**：只含 `README.md`、`docs/testing/round49/README.md`、`docs/roadmap.md` 三处门禁行，先落地，避免与功能批互相拖累。
2. **第二次提交（功能批）**：B、C、D、E、F、H 的成果合并提交；`docs/testing/round50/` 证据索引随批入库（>1 MiB 仅登记路径/字节/SHA256）。
3. **队长唯一全量门禁**：`pwsh -NoProfile -File tools/dev/ci.ps1 -KillStale`，保留原始进程退出码、起止时间、日志路径与 SHA256，末行 `==> 全部通过（12 项）` 才是绿；不以管道尾部成功码代门禁码。
4. 门禁**之后**写入的审计产物（T1 报告、final-manifest）按 R49 §10 口径**不使门禁失效**，但必须在报告里单列。
5. 回滚：任一门禁项红 → 只回退本批文件，不删证据、不改历史日志；重跑必须换新日志名。

## 五、风险与 PARTIAL 预案

| 风险 | 预案 |
|---|---|
| R50-A 写 R49 冻结目录越界 | 只追加「门禁结果」一节、不改既有行/哈希/PNG；`git diff` 全文留证；越界即 `PARTIAL` |
| E 的语义风险：monster 9 条是「模板名/输入匹配」 | 玩家可见的模板名走 TextKey 双语；匹配用别名表仍留在 `config/`，不塞进 UI 词表；实测不到 0 就维持阈值 15 并写 `PARTIAL`，**不写 0** |
| B 的包序：失败也发了库存 | 事务成功后才发；失败零包由 `gameshop_failure` 断言；违例即 PARTIAL |
| C 依赖 B 的包；B 若 PARTIAL | C 只交付离线面板 + 空态/断线态，真实链路验收挂 B |
| 素材缺图标/双路径（dev 散装 vs release packs） | 缺项只写占位 + PARTIAL，不伪造素材、不改授权口径 |
| G 误触历史事实 | 只改状态/备注；有争议的单不关，列清单交门下 |
| 门禁只跑一次（~176s）且不可并发 | 提交前先跑 A2/E2/C2 子集；`-KillStale` 期间不跑 ignored/soak |

## 六、不在本轮范围

技能补全（`Blink`/`PoisonCloud`/`ThunderStorm` 等）、AOI 验收收尾、攻城与领地、坐骑/翅膀精确素材、天气/光照、大地图寻路、任务 `CallDefaultNPC`、行会投票、Push/Jump/Dash 与 Blend117/Reverse/Effect/Sorting（仍记 R49 的 PARTIAL，不在 R50 假装收口）。

## 七、校验声明

1. 本文件所有 `文件:行号` 均已在本轮逐处读取核对（Crystal 侧 10 条、Rust 侧按 §1.2 表格）。
2. **待实证并已列入 R50-01（F0）**：`README.md` 第 49 轮行的精确行号（刑部报告称 `:74`，本文件按此引用）；`docs/testing/round49/manifest.json` 是否需同步；R50-G 的「25 条」实测清单；B 的登录下发锚点精确行。
3. **未给行号、由 R50-01/R50-C 首步实证后补记**：客户端独立面板键位与 `UiState` 目录缓存字段落点（Crystal 侧是 `KeybindOptions.GameShop`，本作建议固定 `K` 并复用 `app.rs:3060` 起的 `Key::Character` 热键表；是否进设置面板可重绑请队长裁决）。
4. 本方案未运行任何 cargo/门禁/git 写操作；门禁结论一律以队长原始日志为准。
