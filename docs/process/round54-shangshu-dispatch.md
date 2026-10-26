# R54 尚书省派发单

## §1 派发依据与强制契约

主单 `JJC-20261026-002`；门下子单 `JJC-20261026-002-sub-12e9178b`；HEAD `08572ae8ca2420c95a8ecf02977b52c64241f235`。
依据计划 `docs/process/round54-plan.md`（124行）与评审 `docs/process/round54-menxia-review.md`（60行）；修改后放行，D1/D4批准、D2/D3修改后批准，以评审原文为准。本单先落盘再CLI；本派发者不改crates、不跑cargo、不commit。
M1–M5原样纳入，各子单以本单全文为强制契约附件：

1. **M1 high，B/T**：拒绝例明确断言PK拒绝原因，施法前确认存活/同图/落点/非安全区/法力与冷却合法、关系真实形成；落地重检例先证明cast成功且pending存在，再经invite/join改变关系，再跨延迟tick断言HP/毒/伤害包不变。清空建关系杂包，设置同条件异队/异会正对照；禁止直接写group_id/guild或注毒造因果。保持概率样本间HP/毒/冷却重置，不能借上次状态命中。
2. **M2 med，A/T**：计划:44“对发现用例的目标”改为“对所有发现用例的目标逐个运行”；各前缀列两次--list、完整例名/目标/实际执行数，合计>=1且所有发现例均执行，0 tests不得计通过。world.rs:5906确认确有既有cfg(test)模块。
3. **M3 high，A/T**：明确NPC首帧使用现有生产 `spawn_npcs`（`crates/mir-server/src/npc.rs:687-748`）并核实返回ID/网格/seen；:759-762的spawn_npc标为测试/GM入口，不能冒充生产加载。测试fixture可提供最小GameData，但动作必须走World；任何NPC回收不得手删容器；无路径则该子项PARTIAL。
4. **M4 med，A/C/T**：补上表第6项ItemObject到期触发锚点与严格>边界；到期前保持、到期后World tick撤销、再tick零重复remove。将广播默认直接send与 `crates/mir-server/src/world.rs:5577-5582` batching路径区分：覆盖真实tick批处理后逐接收者完整解码序列，或明确batching未测子项PARTIAL；不得以跨channel取包时刻证明全局发送序。
5. **M5 med，尚书/C/T**：计划:103的“A1/A2/A4/A5/A6”改为明确的路由名/前缀与矩阵子项，避免与本轮A/B/C/T或历史A2混淆；冻结19项承接映射，每个结果写资源/动作/断言/有效数/原exit/HEAD/候选哈希。首红保留独立attempt；不把资源提前return、未执行和断言FAIL混为PASS。

M4“上表第6项”即评审普通物品到期项：`github/Crystal/Server/MirObjects/ItemObject.cs:129-135`，严格Time>ExpireTime后RemoveObject+Despawn；Rust `crates/mir-server/src/items.rs:1629-1638`同为严格>。
计划§四门禁口径原样承接（只由Lead执行）：

1. 提交一：A/B测试+C文档候选，先由独立gpt评审并由Lead运行唯一全量门禁；失败交原执行者返工，新attempt从头保全；本规划者不提交。
2. 提交二：仅T审计/清单、尚书验收/门下确认等最终文档；Lead在Done确认后显式逐文件纳入。若第二批触及代码/测试/CI/配置/生成输入，原门禁失效，必须对新树再跑；不借“文档例外”改变受测行为。
3. 每次全量命令 `pwsh -NoProfile -File tools/dev/ci.ps1 -KillStale`，当前默认12项；以实际脚本输出总数、分类与原始总exit为准。执行前检查 `target/soak.lock`，有锁则等待合法释放，不删锁、不KillStale。
4. 原件路径 `target/round54/gate/attempt-N/`，保留完整stdout/stderr原始字节、原进程exit_code、起止UTC/本地时区、命令/cwd、HEAD、受测diff/文件sha；逐项OK不是12个独立进程exit。
5. 每条定向命令同样保留原始exit/有效用例数/filtered/提前return说明，TMP/TEMP统一指向仓库target/tmp；attempt不得复用，首红也不覆盖，反事实重建另标。
6. 绑定提交前受测树与提交一sha：保存受测HEAD+候选文件哈希；提交后核对树内容一致；提交一到最终仅允许上述文档差异且经T核实。新日志不替代R52/R53丢失原件。
7. `git diff --check`、链接校验、12项实际门禁均满足，仍只证明本轮受测范围；失败不得写全绿。>1MiB只登记本地路径/B/sha，不入库；target、两历史JSON、.env不纳入。

## §2 四子单表

| key | 角色/模型 | 写范围 | 验收命令 | 证据路径 | 依赖 |
|---|---|---|---|---|---|
| A | bingbu兵部，Lead现有deepseek/deepseek-flash路由，启动前核实 | `crates/mir-server/tests/aoi.rs`及`crates/mir-server/src/world.rs`既有`#[cfg(test)]`块 | 下列A命令组 | `target/round54/aoi/attempt-N/**` | 放行、登记 |
| B | bingbu兵部，Lead现有deepseek/deepseek-flash路由，启动前核实 | 新`crates/mir-server/tests/round54_frost_edges.rs` | 下列B命令组 | `target/round54/frost/attempt-N/**` | 放行、登记 |
| C | libu礼部，sol-relay/gpt-6.1-sol | `docs/world/aoi.md`、`README.md`、`docs/roadmap.md`、新`docs/process/round54-carryover.md`、新`docs/process/round54-aoi-acceptance.md` | `py -3 tools/docs/check_links.py`；`git diff --check`；19项人工映射 | `target/round54/docs/attempt-N/**` | A+B完整结果含PARTIAL/FAIL |
| T | xingbu刑部，独立sol-relay/gpt-6.1-sol，只读 | `docs/testing/round54/round54-final-audit.md`、`docs/testing/round54/final-manifest.json`；第三件summary=`target/round54/audit/summary.md` | 下列T只读验收组 | `target/round54/audit/**` | C、Lead新门禁及候选哈希 |

A命令组：

- 五固定前缀：`aoi_acceptance_enter_snapshot`、`aoi_acceptance_leave_cleanup`、`aoi_acceptance_change_map_sets`、`aoi_acceptance_recycle_all_kinds`、`aoi_acceptance_broadcast_identity_order`。
- 每前缀各运行`cargo test -p mir-server --test aoi <前缀> -- --list`、`cargo test -p mir-server --lib <前缀> -- --list`；列完整例名/目标、合计>=1。
- 所有发现例目标逐一运行`cargo test -p mir-server --test aoi <前缀> -- --nocapture`和/或`cargo test -p mir-server --lib <前缀> -- --nocapture`；两目标都有例则都执行，不能替代或漏跑。
- 回归`cargo test -p mir-server --test aoi -- --nocapture`、`cargo test -p mir-server --lib aoi::tests`、`cargo test -p mir-server --test maps -- --nocapture`，原exit0且有效数>0。
- 进图：真实enter四类ID精确集、自身排除、无重复、完整帧类型分组；NPC必须生产spawn_npcs。离图：双向seen/网格撤销、指定ID一次remove、后续不可达、重复leave无新增remove。
- 切图：mover/旧观察者/新观察者三方精确集、旧NPC隔离、本channel MapChanged先于新首帧、旧广播负例。回收：Some(UserItem)普通物品严格>到期、金币对照、真实死亡→尸体tick；NPC无真实入口PARTIAL。
- 广播：近远/异图同坐标/actor/exclude精确身份，各接收者完整包向量；空间序独立验证，batching依M4覆盖或PARTIAL。maps四资源例先查Server.MirDB，缺样本PARTIAL，不下载/改DB；reported20/有效16历史基线分开。

B命令组：

- `cargo test -p mir-server --test round54_frost_edges -- --list`，必须发现`round54_frost_exact_plus_two_probability`、`round54_frost_party_real_cast_guard`、`round54_frost_guild_real_cast_guard`。
- `cargo test -p mir-server --test round54_frost_edges -- --nocapture`，三例全部执行且exit0。
- 回归`cargo test -p mir-server --test round53_frost_pvp -- --nocapture`、`cargo test -p mir-server --test round52_regression`、`cargo test -p mir-server --lib magic`；7/8/43为历史有效基线，本轮实数报告。
- 60→62技能3，种子3/5/7/11各300次真实cast→延迟tick共1200，每次伤害>0/HP因果；Slow/Frozen各命中/未中合计1200，命中24..72，4/2跳且1000ms；60→63仍伤害且无毒对照。
- Group与Guild分别真实invite/join建关系，PK拒绝原因/合法前置/成功cast+pending→关系改变→落地重检/正对照均按M1；样本间HP/毒/冷却重置，不改公式/RNG。

C须勘误aoi.md:155/175/226，冻结19项来源/动作/证据/结论；不宣称Crystal运行对拍，保留完整PK矩阵/G1/G6等缺口。T读取全部日志、meta/evidence和19项映射，逐项M1–M5复核，核验门禁原字节B/sha、原exit、起止、HEAD/diff/候选哈希。T可用只读`git rev-parse HEAD`、`git diff --name-only`及Node fs/crypto校验；不得跑cargo/回归/门禁。

## §3 波次

门下放行 → 尚书登记A/B/C/T → Lead启动A ∥ B → C → 独立gpt候选评审 → Lead唯一全量门禁 → T → 尚书验收 → 门下确认。
A/B写范围不重叠可并行编辑；同树cargo/ignored/全量门禁由Lead串行，不抢target锁。C依赖A+B完整结果，T依赖C和Lead新门禁，T不兼任作者。
Lead命令`pwsh -NoProfile -File tools/dev/ci.ps1 -KillStale`；`target/round54/gate/attempt-N/`保留原始字节/exit/起止/HEAD/候选哈希。有soak.lock等待合法释放。失败新attempt，首红不覆盖。
尚书核实父/子show、todo、delegate-result及实物后进Review，验收后请求Done交门下confirm，不跳状态；提交仅由Lead依获授权范围执行，本派发者不commit。

## §4 禁捷径与PARTIAL诚实

禁止0 tests/提前return/空结果算通过，禁止手删容器/直接写关系/注毒/GM冒充生产，禁止改种子阈值挑绿。真实断言失败FAIL并保全，缺证/缺资源/未执行PARTIAL，不互换。
不改生产逻辑/公式/RNG/网格/协议/DB/CI/配置，不增getter/feature，不改R53冻结例、不下载素材、不运行Crystal对拍。生产缺陷或无法合法观测回报Lead，不扩范围修绿。
矩阵按五路由名/完整前缀与子项命名，纠正计划“A1/A2/A4/A5/A6”。19项逐条承接，R52十二项/R49 P05/P08/R51遗留及125生产者、真socket/窗口、原生dash、完整PK矩阵/G1/G6/EnemyGuild/门边界仍开放。
历史原件、两历史JSON与.env不动，>1MiB只登记路径/B/sha不入库；每次attempt唯一，首红保留。

## §5 回执要求

每子单先recall/progress，维护todo；返回SUB_TASK_ID↔agent ID、实物路径/行数/B/sha、完整命令/cwd/原exit/stdout/stderr/起止、有效/filtered/提前return数、HEAD/候选哈希及PASS/PARTIAL/FAIL矩阵。
A报五路由子项，B报每种子/总双侧计数与HP/包/毒断言，C报19项映射和检查exit，T报三件审计及门禁绑定；承接方核实非空实物后delegate-result。
尚书向Lead交派发单路径/行数/B/sha、四真实SUB_TASK_ID/部门、验收命令、M1–M5落点、CLI原stdout/exit与异常；Lead负责启动代理及ID映射。
