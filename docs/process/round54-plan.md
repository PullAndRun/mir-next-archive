# 第54轮实施方案（中书省）

> 主任务：`JJC-20261026-002`；承接终态 `JJC-20261026-001`；2026-10-26；角色太子→中书。
> 目标：用真实 World 生命周期测试补 AOI 五路由与 Frost 精确正侧，证据不足即 PARTIAL。
> 架构：复用现有世界、编解码和固定种子 harness；只补测试与文档，不改生产玩法、AOI 网格、公式或 RNG。
> 技术：Rust 现有测试、PowerShell 门禁、Node edict；原始需求为本轮任务包，基准为 R53 验收/终审/AOI v2。
> 执行者按 subagent-driven-development 逐任务执行；本文件只作规划，编码与测试交 deepseek，判断与文档交 gpt。
> 本次规划不改 crates、不跑 cargo、不做 commit/git 写；下列命令是获批执行阶段验收契约，尚未执行。

## 一、事实锚点（均已只读核实存在）

- 基线：`git rev-parse HEAD origin/main` 两行均为 `08572ae8ca2420c95a8ecf02977b52c64241f235`。
- `git status --porcelain`：仅本计划及历史 `docs/testing/monster-native/player-resource-freeze-before.json`、`docs/testing/monster-native/player-resource-freeze.json` 未跟踪；tracked 干净，历史 JSON 不读不写不纳入。
- `docs/process/round53-shangshu-acceptance.md:26-47` 列 19 条承接；`docs/process/round53-menxia-confirm.md:3-35` 确认；看板 show 实证前单 Done。
- `docs/testing/round53/round53-final-audit.md:26-33,69-75`：Frost 7/AOI集成8/库7；随机1是库内复跑；精确+2/同会覆盖不足，不能外推。
- `docs/process/round53-aoi-acceptance-v2.md:13-20,53-65`：3 PASS/5 PARTIAL/0 BLOCK；maps 20 reported 中4资源例提前return，仅16合成例有效；AOI全量仍不可关闭。

| # | Crystal 文件:行号 | Rust 文件:行号 | 已核实事实与本轮用途 |
|---|---|---|---|
| 1 | `github/Crystal/Server/MirObjects/PlayerObject.cs:1741-1768` | `crates/mir-server/src/world.rs:1881-1893` | Crystal y/x/cell插入序；Rust 玩家→怪物→NPC→物品分组；进图不声称逐包等价 |
| 2 | `github/Crystal/Server/MirEnvir/Map.cs:2361-2382` | `crates/mir-server/src/world.rs:1934-1941` | Add/Remove与网格、双向seen登记相对应；四类首帧需精确ID |
| 3 | `github/Crystal/Server/MirObjects/PlayerObject.cs:400-407` | `crates/mir-server/src/world.rs:1961-1997` | 离图remove、退队、清索引；补重复leave与后续广播负例 |
| 4 | `github/Crystal/Server/MirObjects/HumanObject.cs:2596-2602` | `crates/mir-server/src/world.rs:2237-2244` | 跑步摘旧/加新；R53已修sync_player_players，不重写、不回退 |
| 5 | `github/Crystal/Server/MirObjects/PlayerObject.cs:4506-4509,4558-4583` | `crates/mir-server/src/maps.rs:487-520,528-574` | 切图旧remove、清集合、新图通知/刷新；补三方精确集与旧NPC隔离 |
| 6 | `github/Crystal/Server/MirObjects/MapObject.cs:358-375` | `crates/mir-server/src/items.rs:1593-1638` | 撤销/到期后清seen和AOI；普通物品必须Some(UserItem)，None+金币不充数 |
| 7 | `github/Crystal/Server/MirObjects/MapObject.cs:339-375` | `crates/mir-server/src/world.rs:5121-5137` | 怪物回收撤销seen与网格；应经真实死亡与tick，不能直删容器 |
| 8 | `github/Crystal/Server/MirEnvir/Map.cs:2437-2448` | `crates/mir-server/src/aoi.rs:267,273-279`；`crates/mir-server/src/world.rs:5557-5590` | Crystal Players逆序；Rust查询(y,x,id)，排exclude/actor；身份与序列分别证 |
| 9 | `github/Crystal/Server/MirObjects/HumanObject.cs:2596-2602` | `crates/mir-server/src/world.rs:5281-5333` | Rust BTreeSet差集，先remove后add、批内ID序；不同于广播空间序 |
| 10 | `github/Crystal/Server/MirObjects/HumanObject.cs:5967-5979` | `crates/mir-server/src/magic.rs:4098-4145` | 同图/落点2/有效MAC伤害前置；玩家+2，Slow分母100、4跳/1000ms |
| 11 | `github/Crystal/Server/MirObjects/HumanObject.cs:5982-5994` | `crates/mir-server/src/magic.rs:4147-4166` | 玩家Frozen分母100、2跳/1000ms；不改概率、时长、随机次序 |
| 12 | `github/Crystal/Server/MirObjects/PlayerObject.cs:4650-4674` | `crates/mir-server/src/world.rs:2503-2528` | Group=同队，Guild=同会，两谓词不同；EnemyGuild恒false继续PARTIAL |

- 覆盖实查：`crates/mir-server/tests/round53_frost_pvp.rs:555-588` 只测60→63拒毒及60→60概率，没有60→62；1200样本阈值24..72可复用。
- 文档勘误直接依据：`docs/world/aoi.md:155` 写广播ID序、`:175` 写差分空间序，均与上表8/9不符；`:226`差异节需补首帧分组。
- `crates/mir-server/src/npc.rs:658-762` 有查询/生成入口；本次搜索未见NPC回收API，不能承诺凭空闭合生命周期。
- `tools/dev/ci.ps1:107-146,164-177` 默认12项；`docs/process/team-and-roles.md:119-157` 要原始exit/时间/sha与非假绿，soak锁存在禁KillStale。

## 二、可证伪目标（A/B/C 三个交付子任务）

### A：AOI 五路由补证（只补现有路径测试；路由各自判定）

- [ ] 先在 `crates/mir-server/tests/aoi.rs` 写具名测试；需访问私有seen/网格时只在 `crates/mir-server/src/world.rs` 既有 `#[cfg(test)]` 模块追加测试，不新增生产getter/测试开关。
- [ ] 命名固定为下表前缀；分别运行 `cargo test -p mir-server --test aoi <前缀> -- --list` 与 `cargo test -p mir-server --lib <前缀> -- --list`；两处合计必须发现≥1例。
- [ ] 对**所列每个**发现用例的目标**逐个**运行下表命令；若仅库中存在，将 `--test aoi` 换 `--lib`，清单记录实际完整命令和全部匹配例名，不把另一目标0 tests计通过。 > 原文（R54）：对发现用例的目标运行下表命令；若仅库中存在，将 `--test aoi` 换 `--lib`，清单记录实际完整命令和全部匹配例名，不把另一目标0 tests计通过。

| 路由/前缀 | 验收命令（默认集成目标） | 必须能失败的断言/期望 |
|---|---|---|
| 进图 `aoi_acceptance_enter_snapshot` | `cargo test -p mir-server --test aoi aoi_acceptance_enter_snapshot -- --nocapture` | 真实enter：四类对象ID精确集、自身排除、无重复；完整相关帧向量按玩家/怪物/NPC/物品分组，对已核实组内顺序逐项比对 |
| 离图 `aoi_acceptance_leave_cleanup` | `cargo test -p mir-server --test aoi aoi_acceptance_leave_cleanup -- --nocapture` | 真实leave后双方seen/网格精确撤销，移除指定ID恰好一次；再广播/移动不得触达；第二次leave返回None且无新增remove |
| 切图 `aoi_acceptance_change_map_sets` | `cargo test -p mir-server --test aoi aoi_acceptance_change_map_sets -- --nocapture` | mover/旧图观察者/新图观察者visible_players精确集，旧NPC清除；MapChanged先于新首帧；旧图广播负例 |
| 回收 `aoi_acceptance_recycle_all_kinds` | `cargo test -p mir-server --test aoi aoi_acceptance_recycle_all_kinds -- --nocapture` | 普通物品真实到期、金币对照、怪物死亡后尸体tick回收：实体/seen/索引清理与remove ID/次数；NPC若无真实入口则该子项PARTIAL，禁止手删冒充 |
| 广播 `aoi_acceptance_broadcast_identity_order` | `cargo test -p mir-server --test aoi aoi_acceptance_broadcast_identity_order -- --nocapture` | 同图近/远、异图同坐标、actor/exclude身份精确集；各接收者完整包向量与先后位置；网格返回顺序单独验证，不把跨channel消费时刻当全局发送序 |

- [ ] 回归 `cargo test -p mir-server --test aoi -- --nocapture`、`cargo test -p mir-server --lib aoi::tests`、`cargo test -p mir-server --test maps -- --nocapture`；均须exit0且有效用例>0。
- maps四真实资源例：先核实Server.MirDB样本；有则记录例名/素材sha并实跑，无则资源子项PARTIAL，提前return不计PASS；不为本轮临时下载或改DB。
- 交付 `target/round54/aoi/attempt-N/{原始日志,meta.json,evidence.json}`；矩阵逐路由列PASS/PARTIAL/FAIL及断言、例数、资源差异。五路由不要求全部PASS才允许诚实交付，真实失败必须报告而非改生产修绿。

### B：Frost 精确+2及同队/同会真实施法（同一专项测试子任务）

- 写范围：只新建 `crates/mir-server/tests/round54_frost_edges.rs`，复用R53 harness模式和现有资源；不改R53冻结用例/生产函数。
- [ ] `round54_frost_exact_plus_two_probability`：施法者60、目标62、技能3；固定种子3/5/7/11各300次真实cast→延迟tick，共1200次；每次伤害>0、HP因果成立，Slow/Frozen各有命中与未中、合计1200，命中各在24..72，时长4/2跳、tick1000ms；+3目标63对照仍受伤且无两毒。
- [ ] `round54_frost_party_real_cast_guard`：经现有invite/join形成真实同队，Group模式真实cast拒绝且延迟后HP/毒不变、无伤害/毒包；非队员对照可命中；再覆盖施法后入队的落地重检。
- [ ] `round54_frost_guild_real_cast_guard`：经现有行会harness建立同会，Guild模式重复上述拒绝与异会对照、落地重检；不把party当Guild，不碰EnemyGuild。
- 验收先 `cargo test -p mir-server --test round54_frost_edges -- --list`：必须发现上述3个具名例；再 `cargo test -p mir-server --test round54_frost_edges -- --nocapture`：3例全部执行exit0；不放宽阈值、不换种子挑结果。
- 回归 `cargo test -p mir-server --test round53_frost_pvp -- --nocapture`、`cargo test -p mir-server --test round52_regression`、`cargo test -p mir-server --lib magic`；保留现有7/8/43的有效基线，按本次实数报告。
- 交付 `target/round54/frost/attempt-N/{原始日志,meta.json,evidence.json}`，含每种子和总双侧计数、HP/包/毒断言；缺资源/未执行即PARTIAL，发现真实缺陷则返中书裁范围，不改公式/RNG。

### C：文档勘误与承接索引（gpt 礼部，等 A/B 证据交接）

- [ ] 修改 `docs/world/aoi.md` 的155/175及差异节：分别写查询空间序、差分ID序、首帧类型分组；不宣称运行Crystal对拍或逐包等价。
- [ ] 同步 `README.md`、`docs/roadmap.md` 及新增 `docs/process/round54-carryover.md`、`docs/process/round54-aoi-acceptance.md`；保持既有PARTIAL表与本轮逐路由矩阵一致。
- [ ] carryover逐条列R53验收19项的来源、R54动作、证据与结论：R52十二项原编号、R49 P05/P08、R51遗留均保留；+2/同队/同会仅按实际新增证据缩窄缺口，不关闭完整PK矩阵/G1/G6。
- 验收 `py -3 tools/docs/check_links.py`、`git diff --check`：exit0；人工核对19项映射不漏，R53实际frost-pvp/final-manifest/final-gate-summary路径不漂移。
- 交付 `target/round54/docs/attempt-N/` 链接日志与meta；旧R52/R53报告、冻结目录不改，C是文档唯一写者。

## 三、子任务 DAG 与模型/写范围

| 稳定key | 承接/路由 | depends_on | 写范围/交付 |
|---|---|---|---|
| A | 兵部，deepseek-flash测试编码 subagent，由Lead启动 | 门下approve/尚书登记 | 仅tests/aoi.rs与world.rs既有cfg(test)块；target/round54/aoi/** |
| B | 兵部，deepseek-flash测试编码 subagent，由Lead启动 | 门下approve/尚书登记 | 仅tests/round54_frost_edges.rs；target/round54/frost/** |
| C | 礼部，sol-relay/gpt-6.1-sol | A、B完整结果（含PARTIAL） | §二C文档白名单；target/round54/docs/** |
| T | 刑部，独立sol-relay/gpt-6.1-sol，只读评审 | C、Lead新门禁 | docs/testing/round54/round54-final-audit.md、final-manifest.json；target/round54/audit/** |

- DAG：独立门下 → 尚书delegate登记 → A ∥ B → C → Lead门禁 → T → 尚书验收 → 门下confirm → Lead收口提交。
- A/B写文件不重叠，可并行编辑；同树cargo/ignored/全量门禁由Lead排串行，不抢target锁；manifest唯一T写者。
- 表中 `tests/` 均相对 `crates/mir-server/`；编码provider沿Lead现有deepseek路由，禁止误填sol-relay；gpt需显式sol-relay/gpt-6.1-sol。
- 启动用当前可用 subagent/团队工具，先检查实际路由；本规划者深度已到上限，不再派代理，不执行尚书delegate。
- 尚书先取得每个SUB_TASK_ID，再由Lead启动并记录ID↔agent映射；子结果先核对非空实物，不能空完成；门下与T不得兼任相应作者。

## 四、两次提交与门禁口径（只由 Lead 执行）

1. 提交一：A/B测试+C文档候选，先由独立gpt评审并由Lead运行唯一全量门禁；失败交原执行者返工，新attempt从头保全；本规划者不提交。
2. 提交二：仅T审计/清单、尚书验收/门下确认等最终文档；Lead在Done确认后显式逐文件纳入。若第二批触及代码/测试/CI/配置/生成输入，原门禁失效，必须对新树再跑；不借“文档例外”改变受测行为。
3. 每次全量命令 `pwsh -NoProfile -File tools/dev/ci.ps1 -KillStale`，当前默认12项；以实际脚本输出总数、分类与原始总exit为准。执行前检查 `target/soak.lock`，有锁则等待合法释放，不删锁、不KillStale。
4. 原件路径 `target/round54/gate/attempt-N/`，保留完整stdout/stderr原始字节、原进程exit_code、起止UTC/本地时区、命令/cwd、HEAD、受测diff/文件sha；逐项OK不是12个独立进程exit。
5. 每条定向命令同样保留原始exit/有效用例数/filtered/提前return说明，TMP/TEMP统一指向仓库target/tmp；attempt不得复用，首红也不覆盖，反事实重建另标。
6. 绑定提交前受测树与提交一sha：保存受测HEAD+候选文件哈希；提交后核对树内容一致；提交一到最终仅允许上述文档差异且经T核实。新日志不替代R52/R53丢失原件。
7. `git diff --check`、链接校验、12项实际门禁均满足，仍只证明本轮受测范围；失败不得写全绿。>1MiB只登记本地路径/B/sha，不入库；target、两历史JSON、.env不纳入。

## 五、风险与 PARTIAL 边界

- 审查重点：自身/异图误接收、重复leave/remove、切图旧NPC残留、金币冒充普通物品、+2被错误收紧或Group/Guild混淆；分别由 aoi_acceptance_{enter_snapshot,leave_cleanup,change_map_sets,recycle_all_kinds,broadcast_identity_order} + B 具名断言覆盖。 > 原文（R54）：…分别由 A1/A2/A4/A5/A6 与 B 具名断言覆盖。
- NPC若无生产回收入口，仅登记缺口与源码依据，不发明接口/手删数据造绿；资源四例无样本继续PARTIAL。A为五路由“补证交付”，不是全量关闭承诺。
- 测试需私有状态时只写既有cfg(test)块；不能新增生产debug入口；无法合法观测即PARTIAL，先报尚书/门下，不擅扩写范围。
- 测试发现生产失败，保留失败原件、定位与影响说明，交gpt裁决是否另轮；本轮不改生产逻辑修绿。预算不足优先保全已经实现的证据，不追加D研究。
- R52十二项、R49 P05/P08、R51遗留不因本轮门禁通过清零；125生产者、原生dash、完整PK矩阵、门边界仍开放。
- 回滚只撤销本轮明确文件的scoped patch，由Lead处理；禁止reset/checkout覆盖他人变更，历史原件与大JSON不动。

## 六、不做清单

- 不做125真生产者/真socket/真实客户端窗口联调、原生dash表现；本轮不启用可选D研究承诺。
- 不做EnemyGuild/行会战、旧技能耗材、guild嵌套槽、攻城/天气、全量PK矩阵、AOI网格重写、Crystal公式或RNG修改。
- 不改生产crates逻辑、协议、DB/迁移、配置/工具；不下载素材、不改历史冻结证据、不运行Crystal程序对拍。
- 禁止0 tests/提前return/空结果算通过，禁止改种子阈值挑绿、GM/改DB/手造生命周期、旧日志冒充本轮原件。

## 七、待门下裁决项（四项）

1. 是否接受A“逐路由补证、NPC无真实回收入口及资源四例可PARTIAL”，AOI全量默认仍开放？
2. 是否接受将“同会（party）”明确拆成Group同队与Guild同会两个真实施法小例，EnemyGuild继续排除？
3. 是否批准A仅在world.rs既有cfg(test)块增加私有状态断言、禁止生产getter，并允许库/集成双目标发现与记录？
4. 是否认可“两次提交：测试与文档候选→最终纯审计文档”的绑定规则、任何受测行为变更必须重跑门禁？

> 治理留痕：计划骨架先于首条CLI落盘；create/state Zhongshu均exit0；完成后仅转Menxia，不派发、不审批、不执行实施验收。
