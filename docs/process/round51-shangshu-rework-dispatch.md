# R51 尚书省 BLOCK 返工派发契约（B3 / T3）

- 主单 `JJC-20261025-003`，保持 `Doing`；尚书 `shangshu / gpt-6.1-sol`；2026-10-25（Asia/Shanghai）。
- 核验基线：`main`，`HEAD=origin/main=f500269974c92d32b3510aef902e1d8fd2e7c2c6`；既有 A/B/B2/P09/T 单均 Done，协调单 `sub-91431c94` 仍 Doing、无结果；终态不重开。
- 输入：[方案](round51-plan.md)、[门下评审](round51-menxia-review.md)、[原派发](round51-shangshu-dispatch.md)、[T2 v2](../testing/round51/round51-final-audit-v2.md)、[v2 manifest](../testing/round51/final-manifest-v2.json)。本契约只派发本次两项阻断返工。

## BLOCK 依据与证据边界

- T2 v2 为 **BLOCK**：报告实测 19230 B，SHA256 `d7e7ca18001061fa75b3c05fcdcf3ff0ccaffd0d5546055d6529fa7e1050b15d`；manifest 实测 **11274 B**（交接所述10990 B已过时），SHA256 `4d8757d13bdd9c71b65f5aca00307f524da643743a009083ef2d7ac7b9c34be5`。
- attempt5 合法 PASS：`target/round51/final-gate5.log`，189824 B，SHA256 `091e096410bdb4788487da7966517a0de5d73e0a5801591c3475a86818367d11`；meta head 为上述提交，UTC 03:07:21–03:11:33，原始 exit0，12/12 OK。已核原始字节哈希及 GB18030 摘要，不能用门禁绿替代语义验收。
- meta 实际命令为 `pwsh -NoProfile -File tools/dev/ci.ps1 -KillStale`；摘要每项数字0源于OK分类，不能冒充另存的逐进程退出观察。失败attempt1/4及其余历史原件保留。
- **P03 open**：`crates/mir-server/src/magic.rs:3115–3123` 的 `caster_freezing` 只加装备 `added_stats`，漏模板与 buff；Crystal `github/Crystal/Server/MirObjects/HumanObject.cs:1858/1965/2332` 汇总后由 `:5987` 读取总 Freezing。原测试只设附加17，不能证明总来源。
- **P10 open**：`target/round51/b2/07-seven-row-matrix.md` 夸大非零 StepCounter 清零、TemporalFlux 包序、真实施法非空上毒覆盖；`target/round51/b2/00-summary.md` 的“P03–P07/P10全部关闭、未决无”须明确撤回。

## B3：兵部唯一写者与验收

- SUB_TASK_ID：`JJC-20261025-003-sub-3b310e70`；owner `bingbu / deepseek`；实际成员 `bingbu-r51-b` 已由 Lead 直接启动，登记后交其接续，禁止再启第二功能写者；保留真实登记时间。
- 依赖：T2 BLOCK 已确认；生产写范围仅 `crates/mir-server/src/magic.rs` 与该文件内嵌测试；其他生产文件如确有必要，先向 Lead/尚书报精确路径与理由，未裁定不扩围。
- P03：按既有装备/属性刷新语义汇总模板、附加、有效 buff 的 Freezing，避免重复或漏计；修正“仅附加属性即语义一致”的注释；保持 `freezing<=0` 不抽随机、正值区间及 Frozen `5+roll` 的 Crystal 规则。
- P03 定向测试必须分别覆盖模板独有、附加独有、buff独有、混合求和、零值；明确断言总值，并用固定种子或可靠非退化断言证明正值额外时长，不能只用始终允许0的范围断言。验证buff生效/失效来源边界；不借此实现新的buff或PvP系统。
- P10：仅新建 `target/round51/b3/**` 更正版矩阵/摘要/命令证据，逐条指明撤回B2的错误陈述、替代关系和原证据边界；B/B2原件只读保留。七行必须区分生产静态核验、真实运行断言、手工注入消费测试及未测项。
- 矩阵不得再称零初值断言证明清零、生命周期/耗蓝断言证明包序、手工注毒证明施法上毒；P07仍partial、P04/P08仍partial，P03阻断关闭也不抹除玩家目标PARTIAL；不得写“全部关闭/未决无”。
- B3 禁止顺手补 R52 的 StepCounter、真实上毒、失败回纠、TemporalFlux严格包序等测试；仅新增本次 Freezing 缺口所需代码/测试及证据纠偏。
- B3 命令（仓库根；由兵部执行，本尚书不执行cargo）：
  1. `cargo test -p mir-server --lib magic -- --nocapture`（exit0，实际运行数>0，列新增测试名和逐来源断言）。
  2. `cargo fmt --all --check`；`cargo clippy --workspace --all-targets -- -D warnings`（各原始exit0）。
  3. `git diff --check`；`git diff --name-only`，核写范围；人工逐项对照更正版七行矩阵与源码/测试，P03/P10和R52清单自洽。
- 每条证据记录命令、cwd、起止/时区、原始exit、实际通过/总数、HEAD+porcelain、日志路径/字节/SHA256；不沿用B2数值冒充B3实测。交diff给Lead，兵部不commit、不跑全量。

## Lead attempt6 与 T3 独立复审

- DAG：`B3交付 → Lead提交新树 → Lead唯一attempt6 → T3 → 尚书验收 → 按门下指定确认流程处理`；本次只登记与派发，不推进Review/Done。
- attempt6 仅Lead执行 `pwsh -NoProfile -File tools/dev/ci.ps1`，不得SkipTests/SkipPostgres；有 `target/soak.lock` 禁用KillStale，不并发全量/ignored/soak。重跑用新文件名，不覆盖attempt1–5。
- Lead交 `target/round51/final-gate6.log` 与 `.meta.txt`：新提交完整HEAD、branch、命令/cwd、起止/时区、原始进程exit、字节/SHA256、实际项数与末尾汇总；实际exit0且所有项OK才具备T3前提，不预写12/12。
- T3 SUB_TASK_ID：`JJC-20261025-003-sub-d0ea04bd`；owner `xingbu / gpt-6.1-sol`；严格依赖B3交付、新提交及attempt6完成，由Lead启动独立复审者，不能复用B3作者自审。
- T3写范围仅新建 `docs/testing/round51/round51-final-audit-v3.md`、`docs/testing/round51/final-manifest-v3.json`，各≤1MiB；必要新建 `target/round51/final-gate6-summary.log`，实现与全部历史审计/日志只读。
- T3先 `git rev-parse HEAD origin/main`、`git status --porcelain`、`git diff --check`，锁定所审新提交树；重新计算B3、attempt6原件字节/SHA，核源命令/起止/原始exit/项数；不得沿用f500269的attempt5替代新树。
- T3须独立确认 **P03 Freezing总来源缺口 closed、P10不实声明纠正 closed**；逐项核新增测试可失败性、矩阵/摘要与R52残余一致，不能把手工状态或静态阅读写成真实施法运行覆盖。
- 任一阻断仍开或新树证据不符则BLOCK退回原owner；全部闭环后仅提出 **PASS-WITH-PARTIAL** 范围建议，由有权验收角色接受残余，禁止直接宣布五技能完整/主单Done。
- T3不得代修、不跑cargo/全量/ignored、不commit；manifest列新树、父子ID、证据路径/字节/SHA/状态及报告hash，manifest自身hash外部回报，避免自引用。

## R52 必补与允许保留的残余

- 按T2 §H保持：P01/P02毒云/困魔必需耗材与语义未实现，Unsupported真拒绝、不扣蓝；Frost玩家目标待PvP批；P04火墙逐对象与InTrapRock缺系统；P08 Walking/Walk借用、原生dash视觉/时序未验。
- R52必补测试：真实Frost施法非空上毒及概率两侧、玩家Frozen实际attack拒绝；零步非零StepCounter清零；StormEscape专门跨图运行例；默认MagicInfo/2500ms冷却与严格TemporalFlux包序；152/153在错误预测格、已有motion下回纠/Standing/清队列。
- 继续保留原派发排除项：PetSave/PetExp、AOI全量、攻城控制点/城门/领地、插值余项、寻路/天气/光照、坐骑翅膀、小地图NPC/CallDefaultNPC、行会、R49像素证据/K重绑；不在B3建立或虚构R52任务ID。
- 原始target证据本地ignored，manifest不能替代原件；异机复核由Lead保全原始字节。两份monster-native冻结JSON保持不动。

## 看板回填与禁捷径

- CLI固定 `node C:/Users/diana/.dsh/skills/edict/scripts/edict.mjs`；新建B3/T3执行子单，既有Done单不重开。T3的前置依赖写入验收契约，由Lead执行调度；CLI无通用depends_on参数，不手改看板JSON。
- 两承接角色先recall/progress，按真实阶段todo（各最多一项in-progress）；自身todo与实际证据齐备后由对应角色delegate-result。T3登记Doing表示委派已建，不等于依赖满足或复审已启动。
- 尚书只写本流程文档、主单memo及合法看板操作；主单保持Doing，最终读取show并执行 `validate --as shangshu`。现有协调单91431c94的shangshu delegate-result历史exit4权限缺口保留，交Lead按实际权限处理，不冒名重试。
- 禁止免耗施法、改包号/Crystal规则、删拒绝断言、allow/skip/ignored凑绿、GM/DB造状态冒充链路、覆盖历史/冻结证据、旧树日志套新树、硬编码成功、假运行/假时间及越界扩张。
- 本次回执：B3/T3 delegate均exit0，主单memo exit0；重复 `state Doing` 被拒exit3（`Doing → Doing` 非法，允许Review/Blocked/Cancelled），随即show确认仍Doing，未重试；合法progress exit0。`validate --as shangshu` exit0，113条任务符合schema v3.1。CLI仅拒绝冗余同态更新，主单未离开Doing；未执行done/Done。文档链接检查exit0（1109条相对链接、2篇README），`git diff --check` exit0；本尚书未跑cargo、未改实现或证据原件、未commit。
