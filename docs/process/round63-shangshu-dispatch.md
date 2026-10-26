# R63 尚书派发：570 严格时钟与首批缓存契约

结论：门下 PASS-WITH-CHANGES 的 M1–M3 已并入实际 A/B/C/D 子单提示词；完成登记，主单 Doing。此为派发准入，不代表实现、真红、绿或门禁已通过。

## 基线与看板

- 主单：`JJC-20261026-010`；接手 Assigned，派发后 Doing；四子单登记成功。
- 仓库：`C:/Users/diana/Documents/github/mir-next`；实查 `HEAD=f80724ee3e0d4242e7be1af60ece09ad8b48c432`。
- [中书计划](round63-plan.md)：SHA256 `cbd122e19df4fc1673904c9d84ca84a098b8718a43f9c53a2094405dfa4a1181`。
- [门下评审](round63-menxia-review.md)：SHA256 `5b12715e31ede349f077d4fe5f638ebd7d82c6ede598b29ba1aed7f241bcd386`；必改3项，静态评审不代替真红。
- 接手未结主单1；新增四子单后 validate：191条符合 schema v3.1，exit 0。主单 show 确认 Doing，todos=[]，未代填完成结果。

## 子单契约

所有原始日志置于各自 target 白名单；仅按依赖启动，写范围互斥。B 的 tests 文件均位于 `crates/mir-client/tests/`。

| 单 | 实际 sub ID / owner | 依赖 | 写白名单 | 交付 | 验收 |
|---|---|---|---|---|---|
| A | `JJC-20261026-010-sub-823eaf5f` / bingbu | 派发准入 | `crates/mir-client/src/ui/projectile.rs`；`target/round63/a/**` | G1严格时钟、G2复位后缓存本帧再spawn窄修复；单测红绿/偏离/日志 | unit过滤器非0；clippy/fmt exit0；不扩wire/total/游标/ActionFeed/LRU/audio/net |
| B | `JJC-20261026-010-sub-1f51f226` / gongbu | A交付 | `crates/mir-client/tests/{round63_projectile_wire.rs,round59_projectile_render.rs,round61_projectile_queue.rs,round62_projectile_wire.rs}`；`target/round63/b/**` | 全wire G1/G2；隔离基线assert红→功能树同测试绿；历史elapsed supersede | 四integration targets；同测试SHA红绿；不得改实现凑绿；保留旧名与覆盖 |
| C | `JJC-20261026-010-sub-da9402eb` / xingbu | A、B交付 | `target/round63/c/**`；其余只读 | 独立复跑/断言与exit摘要、Crystal锚点表；`target/round63/c/round63-c-evidence.md` | unit+四integration；核实真红、skip=false、M3；旧ignored 7与新增数/位置；无新增ignore/allow；clippy/fmt/diff |
| D | `JJC-20261026-010-sub-5d0de186` / libu | C通过及Lead功能提交、唯一门禁 | `docs/testing/round63/**`、`docs/process/round63-*.md`、`docs/roadmap.md`、`docs/client.md`、`target/round63/d/**` | README、final-manifest、run-checks.sh、gen-tree-hashes.sh、commit-tree-hashes.txt；G3/P01–P12/supersede映射；C证据转录；client/roadmap | check_links、diff --check、JSON解析、hash可复核、每受控新增文件≤1MiB；按事实结论 |

D的文档白名单不授权覆盖既有中书计划、门下裁决或本派发文书。新manifest须按JSON解析，不自引用hash；区分工作树hash与提交blob。

## M1：既有文件归属及显式暂存

接手 `git status --short` 实为4项未跟踪：门下时点三项前件加后来新增的评审文书；受控文件无修改，不能称工作树全干净。

| 既有未跟踪文件 | 归属 / 入库规则 |
|---|---|
| `docs/process/round63-plan.md` | 中书规划前件，Lead文档批显式审阅暂存 |
| `docs/testing/monster-native/player-resource-freeze-before.json` | 历史冻结前件，永不入本轮提交；保留原状 |
| `docs/testing/monster-native/player-resource-freeze.json` | 历史冻结件，永不入本轮提交；保留原状 |
| `docs/process/round63-menxia-review.md` | 后增门下独立裁决，Lead文档批显式审阅暂存 |

- 已映射到 A/B/C 提示词：只显式暂存白名单、冻结JSON禁入；C复核暂存清单，D再次承接。
- 功能批暂存仅A的实现文件及B的四个具名测试；文档批仅经审阅的计划/评审/本文、D文档白名单具体文件。
- 使用 `git add -- <逐个审阅的具体文件>`；禁止 `git add .`、`git add -A` 或将目录整批纳入。提交前核对 `git diff --cached --name-only`。
- target仅作本地证据，不因执行白名单而成为入库白名单；不删除或改写上述既有文件。

## M2：串行执行与隔离基线真红

- A→B不变。A先修实现；B须在 `target/round63/b` 内从 `f80724ee3e0d4242e7be1af60ece09ad8b48c432` 建隔离worktree或只读checkout，禁止回改功能树取红。
- 隔离基线只叠加新公开行为测试，Monster/Remove/125均走编解码→snapshot→UiState::sync，不直接填私有缓存、不叠加A修复。
- 对新G2测试过滤取得可定位的真实assert失败；编译失败、旧测试失败、0 tests均不算真红。
- 保存基线完整SHA、测试文件SHA256、过滤器、完整命令、UTC起止、stdout/stderr、exit、失败断言及实际/期望值；功能树使用同一测试内容/SHA再取绿。
- B提示词已明确以上责任与路径；A已知B须隔离重建红，C提示词要求独立复核同测试SHA及基线assert红/功能绿。
- C核验不成立则退A/B；缓存无公开wire真红即移出功能批，保留静态风险/PARTIAL，不拿旧elapsed断言失败代替G2红。

## M3：对象9与负例99精确口径

- 首批新代快照含源7、目标8、无关可见9，首条125打8；9从未被早先投射物命中过，不能进入spawn/advance的源目标回填而掩盖刷新缺陷。
- 下一同代快照移除9，确认9在WorldSnapshot缺席，新125打9：相对该条125前新增投射物恰1、出生cue恰1、落点等于首批9位置。
- 该投射物完成后，属于它的570效果与命中cue均0；其它目标8的既有结果须隔离计数，不能污染断言。
- 9随后重现不补播旧125，不新增旧事件投射物/出生cue/命中效果。
- 负例：旧代曾见99，新代从未出现99，新125打99必须零生成、零出生cue，旧位置不可跨代复活。
- 映射：A提示词约束复位→本帧位置→spawn与9不被回填；B提示词含全部精确计数与wire路径；C提示词逐项独立核对。

## DAG与门禁顺序

`A → B → C → Lead功能提交 → Lead唯一ci.ps1 → D → 文档批 → 独立终审/指定角色确认`。

1. A先单测红绿；B隔离基线红/功能树绿并supersede历史断言；C只读独立复跑及审查，不执行ci.ps1。
2. C交Lead后按显式白名单提交功能批；仅Lead在该功能HEAD运行 `pwsh -NoProfile -File tools/dev/ci.ps1 -KillStale`。
3. Lead保存 `target/round63/gate/attempt-1.log`、功能HEAD、exit、实际项数及SHA256；失败保留原日志，另编号attempt-N，退A/B→C→新提交→新门禁。
4. 门禁通过D才按事实完成文档；Lead验证links/diff/hash并提交文档批。门禁后功能/测试变化必须重新功能提交与门禁。
5. 独立门下终审，按真实 pending_confirm.confirm_by 确认；本阶段不申请Done、不代终审、不commit/push。

## 入库禁区、风险与禁称

- 禁入 `.env`、resource、target原始日志、历史大冻结JSON及密钥；不覆盖R59/R61/R62冻结证据，新manifest记录旧测试/语义→新断言/提交。
- A严格start+60ms；now等号不进，到期skip先+1再普通+1，frame>=10删除；elapsed不决定生命周期，绘制只读；生产skip=false仍PARTIAL。
- G1验证1060/1061、1120/1121、1600/1601及大跳11000后11001完整状态；稀疏tick可延长时长；同刻/回退守卫为客户端附加约束。
- 0 tests不可PASS；SKIP/缺素材early-return列PARTIAL、总结果非全PASS；不得新增ignore/allow掩盖失败，旧7 ignored须实测核对。
- G3仅契约：保护活跃引用、miss少生成风险、容量/LRU/ID复用/换图待证；裁剪需绝对序号与所有消费者最低水位，含慢消费者/clone/重连/饱和；不造协议或任意上限。
- D逐项承接R61 P01–P12，不能缩写P03而声称全局TargetPoint闭合；ActionFeed FIFO、物理回收、设备Live等保留PARTIAL。
- 禁称：实施前R63已PASS、125全链已证/完整、生产SkipFrames已接、缓存已限容/无内存增长、已物理回收、严格时钟=600ms必删、真实可听/Live已证、全场景无丢重、十二PARTIAL全闭合。

## 实际执行记录与代笔声明

- 派发日志：`target/round63/shangshu/01-delegate.log`，逐条保存命令/输出及EXIT；四次delegate、state Doing、validate均EXIT=0；追加show亦EXIT=0。
- 偏离已修正：请求把A单测指向计划50–53，该处实际为B wire；A按37–40四个时钟测试，B保留50–53 wire测试名与覆盖。
- C的docs交付与只写target冲突：C在target产出原稿，由D转录至 `docs/testing/round63/round63-c-evidence.md`；未扩大C写范围。
- 工具留痕：首次写文书因参数校验 `invalid justification: expected a non-empty sentence` 未写入；补齐说明后创建成功，不影响六条CLI命令及结果。
- 本文由 **gpt 尚书（gpt-6.1-sol）** 完成。只新增本文及用户指定本地派发日志，不建脚本、不改crates、不运行cargo/ci.ps1、不commit/push。
- 当前会话子代理深度已满，仅完成看板登记；执行agent尚未启动、无agent ID映射，由Lead按DAG启动并记录，未把登记冒称四部执行通过。

> 资源映射（Lead 回填，2026-10-27T00:05+08:00）：main→`JJC-20261026-010`（Lead 编排、提交 1fcf4fc、门禁 attempt-1 EXIT=0）；A→deepseek 成员 `bingbu-hit-a`（sub-823eaf5f，15:19Z Done）；B→deepseek 成员 `gongbu-wire-b`（sub-1f51f226，15:29Z Done）；C→deepseek 成员 `xingbu-r63-c`（sub-da9402eb，15:41Z Done，PASS-WITH-PARTIAL）；D→gpt 成员 `libu-r63-d`（sub-5d0de186，进行中）。映射明细 `target/round63/lead-mapping.md`。
