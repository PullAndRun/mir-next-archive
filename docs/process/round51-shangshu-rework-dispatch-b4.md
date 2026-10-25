# R51 尚书省 T3 BLOCK 续派契约（B4 / T4）

- 主单 `JJC-20261025-003` 保持 `Doing`；尚书 `shangshu / gpt-6.1-sol`；2026-10-25（Asia/Shanghai）。
- 续接 [B3/T3 派发](round51-shangshu-rework-dispatch.md)、[原派发](round51-shangshu-dispatch.md)、[门下评审](round51-menxia-review.md)；仅本次 P03/P10 两项返工。
- 核验基线：`main`，`HEAD=origin/main=42a49e0879be4d00bccd43bd7ebce8c9e4276058`（含 B3 `1fe36dd` 与 T2 v2 文档）；当前兵部 B4 工作树修改不属于该提交树的已验收内容。

## T3 缺口、原件与登记事实

- 初核时 T3 报告尚未落盘，先按 Lead 描述及源码/diff核验；本轮收尾已实读 [T3 正式报告](../testing/round51/round51-final-audit-v3.md) 及 [v3 manifest](../testing/round51/final-manifest-v3.json) 的 verdict/阻断记录，正式结论 **BLOCK，P03/P10 open**。
- T3 的两项阻断与下列独立核验一致；attempt6 合法 PASS 不能代替语义验收，B4 在途修改也不能视为 closed。T3 §B/§F列出的来源/失效边界、实际延长及声明纠偏一并纳入本次验收。
- T3 §F另记 B3 提前补 StepCounter/真实 Frozen/包序与旧契约不符；本次保留范围偏差记录交 Lead 裁决，不倒填历史授权。
- **P03**：基线 `crates/mir-server/src/magic.rs:3125-3131 caster_freezing` 遍历装备时无耐久过滤；Crystal `github/Crystal/Server/MirObjects/HumanObject.cs:1836` 在模板/附加累加前跳过 `CurrentDura == 0 && Info.Durability > 0`；Rust `crates/mir-server/src/items.rs:912-921 apply_equipment_stats` 同规则。两种装备属性都应排除，buff 独立计算。
- **P10**：基线 Blink 测试以 `Option<usize>` 比较包序；`None < Some(_)` 为真，缺 `ObjectTeleportOut` 仍可通过。B3 矩阵的严格包序声明证据不足；必须每包存在断言先执行，再比顺序。
- attempt6 原件实测：`target/round51/final-gate6.log`，188643 B，SHA256 `1e1ffa2f7a9cee641b4a1d9729722bd99158030c68b8630cfc062709ef4dacdb`；GB18030 解码末尾 12/12 OK、全部通过。
- `target/round51/final-gate6.meta.txt`：start `2026-10-25T03:37:05Z`，end `2026-10-25T03:40:52Z`，227 s，原始 exit0、head 同基线；meta 本身未记命令/cwd，不由尚书补写，T4须据新门禁原件独立核。
- 初核时 CLI show 实际 B3=`JJC-20261025-003-sub-3b310e70`、T3=`JJC-20261025-003-sub-d0ea04bd` 均仍 Doing、无回填结果，与交接所述 Done 不一致；已报 Lead，由原承接角色回填真实结果，不代造成功。
- B3 `target/round51/b3/00-summary.md` 和 `07-seven-row-matrix.md` 误标 `sub-28585983`（B2），B4 新证据须更正关联映射；旧件保留。协调单 `sub-91431c94` 继续按实际 CLI 权限处理。

## B4 范围、验收与交接

- SUB_TASK_ID：`JJC-20261025-003-sub-e2cb39cb`；owner `bingbu / deepseek`；唯一成员 `bingbu-r51-b` 已由 Lead 直接下达在途，本次真实登记后接续，不启动第二功能写者、不倒填登记时间。
- 唯一允许写：`crates/mir-server/src/magic.rs` 及内嵌测试，新增 `target/round51/b4/**`；其他 crates、协议、配置、文档与 B/B2/B3 历史证据只读。必要扩围先报精确路径与理由，由 Lead 裁定后实施。
- P03：沿用 `apply_equipment_stats` 耐久语义，先过滤再汇总模板与附加；buff 来源保持独立，不改 Crystal 数值/概率/时序。`freezing<=0` 不抽 RNG，正值 `0..freezing-1`，Frozen `5+roll`。
- 必验模板独有、附加独有、buff独有、混合总值与全零；不能以单一附加来源代替三来源。按现有 add/remove/tick 路径验证 buff 生效、移除及到期后失效，不创建新 buff/PvP 系统。
- 耐久正反：`durability>0,current_dura=0` 的同件装备同时带非零模板与附加，二者均不贡献；恢复 `current_dura>0` 后二者恢复；`durability=0,current_dura=0` 的模板与附加仍贡献；损坏装备并存的有效 buff 仍贡献。
- 每例须可失败：明确断言汇总数值；零来源真实 Frozen 固定5，以相同种子对照下一次 RNG 输出，运行证明不消耗；正来源固定种子真实施法 Frozen 必须有 duration>5 或精确非5值断言，不能只验证单独 freeze_roll>0 或永远包含5的时长范围。
- 主装备模板/附加正确引用 Crystal `HumanObject.cs:1858-1859`；`:1965-1966` 属于插槽刷新，不再混称主装备附加；插槽新系统仍排除。
- P10：本人 `ObjectEffect(Teleport)`、`AddBuff(TemporalFlux)` 与观察者 `ObjectTeleportOut`、`ObjectTeleportIn`、`ObjectEffect(Teleport)` 五个位置各先 assert/expect 存在，再比较本人 Effect<Buff、观察者 Out<In<Effect；不改生产发包顺序凑绿。
- 包序断言须保证任一相关包缺失必失败；静态核对存在性断言支配比较语句，并交实际非空包流测试名。不得再把 Option 排序当存在性证明。
- 新建 B4 七行矩阵与摘要：更正 B3 对严格 Out 包序、RNG不消耗及实际延长的过度声明；B3 已撤回“全部关闭/未决无”且标待复审，不把已撤回结论说成仍存在。分列静态核验、真实运行、手工注入消费与未测项，纠正 B3/B2 子单映射。
- 按 T3 §F保留并更正历史：B3 magic原日志起始11:32:54+08（快照误写11:31:19）；B2为29例，B3新增2例+改名1例，29+2=31；B3 clippy实际只server/client，不能冒称workspace。B4以自身原件记录新时间、命令范围和计数，不覆盖旧件。
- B4 命令（根目录，由兵部执行，尚书本次不跑 cargo）：
  1. `cargo test -p mir-server --lib magic -- --nocapture`（原始 exit0，实际例数>0，列三来源、耐久正反与五包存在/包序用例）。
  2. `cargo fmt --all --check`；`cargo clippy --workspace --all-targets -- -D warnings`（各原始 exit0）。
  3. `git diff --check`；`git diff --name-only`，核唯一写范围；人工对照新矩阵、源码/测试与 R52 清单。
- 各命令交 cwd、起止/时区、完整 HEAD+porcelain、原始进程 exit、实际通过/总数、日志路径/字节/SHA256；失败也保留，不套用 B3 的31例或旧门禁数字。兵部交 diff 不 commit、不跑全量。

## Lead attempt7 与 T4 依赖契约

- DAG：`B4真实交付 → Lead提交新HEAD → Lead唯一attempt7完成 → 独立T4 → 尚书验收 → 合法Review/门下确认`；当前只登记，不启动复审、不推进主单验收状态。
- attempt7 唯一执行者 Lead：根目录 `pwsh -NoProfile -File tools/dev/ci.ps1`；禁 SkipTests/SkipPostgres；有 `target/soak.lock` 禁 KillStale；不并发全量/ignored/soak。
- 新建 `target/round51/final-gate7.log` 与 `target/round51/final-gate7.meta.txt`，记录新完整 HEAD/branch、命令/cwd、起止/时区、原始进程 exit、字节/SHA256、实际项数及末尾摘要。只有原始 exit0 且全部实际项 OK 才能交 T4；不预填12项。
- 重跑用新 attempt 名并同步契约/依赖，不覆盖 attempt1–6 或失败原件；attempt6 只证明 42a49e0，不能套用到 B4 新树。
- T4 SUB_TASK_ID：`JJC-20261025-003-sub-1c3b4c52`；owner `xingbu / gpt-6.1-sol`；由 Lead 在上述依赖满足后启动，不能由 B4 作者兼任独立复审。
- T4 唯一允许写：新增 `docs/testing/round51/round51-final-audit-v4.md`、`docs/testing/round51/final-manifest-v4.json`，各≤1MiB；实现、target 原件及 v1/v2/v3 审计只读，不 cargo、不代修、不 commit。
- T4 先核 `git rev-parse HEAD origin/main`、`git status --porcelain`、`git diff --check`，锁定 Lead 新提交功能树；重算 B4/attempt7 原件字节/SHA，核命令/时间/原始 exit/实际项数与 HEAD。
- T4 独立确认 **P03 耐久与三来源缺口 closed、P10 五包存在且包序断言可失败 closed**；核矩阵纠正、子单映射、原件保全及所有 PARTIAL/R52，不能把静态阅读写成运行覆盖。
- 任一缺口仍开、证据跨树/缺原始 exit/哈希不符/零例假绿，结论 **BLOCK** 交 Lead 退原 owner；全闭环才提出 **PASS-WITH-PARTIAL** 建议，由有权角色接受残余，不直接宣布五技能完整或主单 Done。
- manifest 登记新树、父子 ID、原件路径/字节/SHA/状态及报告 hash；manifest 自身 hash 由外部回报，避免自引用；不得预写审计 verdict 或未执行的验证。

## PARTIAL / R52 与禁捷径

- 固定 PARTIAL：P01 PoisonCloud/P02 TrapHexagon 必需耗材排除，Unsupported 真拒绝且不扣蓝；Frost 玩家目标留 PvP；P04 火墙逐对象/InTrapRock；P07 152/153 敏感回纠；P08 Walking/Walk 借用与原生 dash 视觉/时序。
- R52 保留：真实 Slow 非空与概率两侧、玩家 Frozen 拒攻、默认 MagicInfo/2500ms冷却、StormEscape 专门跨图和 buff→LevelMagic 严格包序、152/153 错误预测格+已有 motion、火墙完整语义、原生 dash、guild Freezing 来源与真 socket。
- 非零 StepCounter 已由 B3 增加前值3→0断言，T4按实际代码/运行证据核；不机械复制旧 R52 清单，也不借 B4 顺手扩大其他未测覆盖。
- 继续排除 PetSave/PetExp、AOI全量、攻城/领地、插值余项、寻路/天气/光照、坐骑翅膀、小地图NPC/CallDefaultNPC、行会、R49像素/K重绑；不得虚构 R52 子单ID。
- 禁免耗施法、改包号/Crystal规则、删拒绝断言、allow/skip/ignored凑绿、GM/DB状态冒充链路、硬编码成功、假运行/假时间、旧树日志套新树、覆盖历史/冻结JSON或审计原件。

## 看板操作与本次回执

- CLI `node C:/Users/diana/.dsh/skills/edict/scripts/edict.mjs`，全部真实尚书写命令带 `--as shangshu`；只经 CLI 写，不手改全局 JSON，不冒名换角色绕门禁。
- B4/T4 delegate 均 exit0；实际 SUB_TASK_ID 如上。先 recall/progress/todo 后执行、每单最多一项 in-progress；承接角色按真实结果 delegate-result，BLOCK结果也不得冒写PASS。
- CLI 无通用 depends_on 参数，依赖以本契约+delegate验收文本+Lead串行调度落实；T4登记Doing表示建单，不表示前提满足。
- 主单初核已Doing，沿用该状态；历史 `Doing → Doing` 被拒 exit3，本次不重复同态命令、不 done/Done。旧 B3/T3 结果及协调单按真实权限处理；`shangshu delegate-result` 历史 exit4限制保留，不重试、不冒名。
- 主单 memo、progress 均 exit0，最终 show 仍 Doing；`validate --as shangshu` exit0，115条任务符合 schema v3.1。文档链接检查 exit0（当次1160条相对链接、2篇README），`git diff --check` exit0；最终字节/行数/SHA由外部交付回报，避免自引用。
- CLI progress 对斜杠片段会清洗缩短（实测PARTIAL/R52与B3/T3的后半被略），看板备注改用“PARTIAL及R52、B3与T3”，完整契约以上述文档与delegate验收文本为准。
- 尚书仅写本流程文档及合法看板 memo/progress；不写 crates/审计原件/target，不 commit、不跑 cargo/ci。共享工作树中的实现/v3审计变动由原owner负责。
