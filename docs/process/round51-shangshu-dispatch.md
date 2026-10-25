# 第51轮尚书省派发契约：技能首批补全与商城文档同步

## 一、事实与输入

- 父单 `JJC-20261025-003`；尚书 `shangshu / gpt-6.1-sol`；日期 2026-10-25（Asia/Shanghai）。
- 只读核对：HEAD 与 origin/main 均为 `716db4292ad6d475e92bf8b6cbd4f230f307684f`；父单 Assigned。方案实为85行，评审114行。
- 输入：[中书方案](round51-plan.md)、[门下评审](round51-menxia-review.md)、[R50派发](round50-shangshu-dispatch.md)、[团队职责](team-and-roles.md)。本契约吸收M1–M7及五条裁决；被修订处以门下评审为准，语义以Crystal源码为准。
- 已有门下→尚书协调单 `JJC-20261025-003-sub-91431c94`；交付本契约与合法看板登记，不作为功能实现单。
- 仓库现有未跟踪方案/评审及 `docs/testing/monster-native/player-resource-freeze-before.json`、`player-resource-freeze.json`；不覆盖、不删除、不擅自加入提交。
- 完整交付目标仅 Blink、ShoulderDash、StormEscape、FrostCrunch、ThunderStorm；PoisonCloud与TrapHexagon因排除必需耗材固定PARTIAL，保留真实拒绝/未支持，不得免耗施法。
- M7纠正：原Unsupported断言在 `crates/mir-server/src/world.rs:6801–6823`，是Fencing被动拒绝，须保留；Blink查名非生产接线，labels/spellfx非完整表现。
- 尚书不改crates、不跑cargo/ci、不git写、不启动执行者；Lead唯一启动、全量门禁与git，六部交diff和证据。

## 二、逐子任务契约

### A 文档批（原方案R51-B）

- SUB_TASK_ID：`JJC-20261025-003-sub-82bdab8e`；owner：礼部 `libu / gpt-6.1-sol`；依赖：无。
- write scope：仅 `docs/world/gameshop.md`、`docs/client.md`；必要新增 `docs/process/round51-libu-docs.md`；不得改方案/评审/本契约或crates。
- 必须遵守M6：只定点更新活动状态，链接[R50验收](round50-shangshu-acceptance.md)及R50-C2/T2证据；保留旧HEAD、缺陷、返工历史、P02–P08/P10及Crystal已披露差异。商城编号250/251、当前状态逐条有源证据。
- 验收：`py -3 tools/docs/check_links.py`（exit0、相对链接全通过并报实际数量）；`git diff --check`（exit0）；人工交当前陈述→R50证据映射，链接检查不证明商城语义。
- 证据路径：上述新文档内记录命令、cwd、起止/时区、原始exit、实际数量及diff摘要；不额外写target，保持文档任务写范围。
- PARTIAL：断链、无证据“已交付”、全局抹历史、越界写、缺原始exit；礼部不commit，Lead核对后独立提交文档批。

### B 功能批（原方案R51-A+C合一）

- SUB_TASK_ID：`JJC-20261025-003-sub-a51b133e`；owner：兵部 `bingbu / deepseek-flash`；依赖：A交付且Lead文档批已提交。
- 唯一功能写者；write scope精确限定如下，均允许内嵌测试：
  - `crates/mir-server/src/magic.rs`、`crates/mir-server/src/world.rs`、`crates/mir-server/src/buffs.rs`、`crates/mir-server/src/monster.rs`。
  - `crates/mir-client/src/net.rs`、`crates/mir-client/src/ui/spellfx.rs`、`crates/mir-client/src/ui/labels.rs`。
  - `crates/mir-protocol/src/manual/server.rs`仅相关测试；生成的 `crates/mir-protocol/src/packets/server.rs`只读；不改包号/线格式。
  - 证据只写 `target/round51/b/**`；不改活动文档、工具、配置或其他生产文件；需要扩围先报尚书审定精确路径/原因，未批准保持PARTIAL。
- A产生真实服务端行为包，C消费该输出、验证解码及net状态；不另设客户端写者。ObjectWalk28/ObjectRun29、商城250/251仅回归，不顺带扩功能。
- 必须逐项交下表七行：完整MagicInfo→消耗→效果→目标→位置/持续→包序→测试名与实测证据；共同覆盖成功、拒绝、延迟边界、随机分支。默认资料必须读完整对象，不截断引用。
- Crystal共同源：`github/Crystal/Server/MirObjects/HumanObject.cs:3375` MagicCost顺序、`:3773–3779` CastTime/本人Magic/他人ObjectMagic；Dash提前return，不套普通包。

| 技能/结论目标 | 完整资料源（均为github/Crystal/Server/MirEnvir/Envir.cs）及消耗 | 必须验的效果/目标/位置或持续 | 包序、测试与PARTIAL边界 |
|---|---|---|---|
| Blink / 完整目标 | :931–934；10+3L、Range9；传送惩罚再普通惩罚及整数截断 | HumanObject.cs:3537–3539、6114–6128；200ms；禁传/范围/有效落点/Random.Next(4)；指定落点，30s TemporalFlux/30% | 普通回显→延迟传送出入/AOI→Teleport效果/修炼/buff实序；成功/失败/200ms边界；随机Teleport不能代替 |
| ShoulderDash / 完整目标 | :333–350；4+4L、Delay2500ms | HumanObject.cs:4975–5157；受困/不可走、距离2+L+[0,1]、安全区/碰撞、低等级敌目标、Pushed/伤害/修炼、CellTime500ms | UserDash/ObjectDash逐格、零移动DashFail、末MagicCast；空间移除/加入；Walk/Run不可替代；测零步/阻挡/等级边界 |
| StormEscape / 完整目标 | :935–952；65+8L、Range9；同Blink惩罚 | HumanObject.cs:3580–3588、6079–6128；先500ms自身中心雷击，后750ms落点传送；失败/禁传不撤既有伤害；30s惩罚 | 分测500/750ms、StormEscape效果、位置与包序；不可作为Blink别名 |
| FrostCrunch / 完整目标 | :711–728；15+3L、Range9 | HumanObject.cs:3501–3504、3939–3952、5962–5995；敌目标/CanFly、距离×50+500ms；MAC、原落点±2/同图/节点、等级概率Slow/Frozen | 普通包→延迟伤害/状态；玩家/怪物概率与时长分测；缺Slow/Frozen则PARTIAL |
| ThunderStorm / 完整目标 | :729–748；29+9L、Range0 | HumanObject.cs:4264–4270；Map.cs:1303–1341；500ms，施法时自身中心5×5敌玩家/怪，非亡灵伤害/10 | 普通包→地图延迟伤害；测友方/边界/亡灵/中心时点；不得变成指定落点或持续圈 |
| PoisonCloud / 固定PARTIAL | :1247–1266；30+5L、Range9、冷却18000−2000L；护身符5+毒5并各耗5 | HumanObject.cs:4543–4563；Map.cs:1434–1488；3×3有效格去重、距离×50+500ms、实体6000ms/1000ms tick；绿毒Duration12、SC均值+BonusDmg、敌玩家/怪 | SpellObject.cs:150–166、503–548：MAC+绿毒、ObjectSpell Show/CastLocation、tick/移除；本轮不实现耗材，交拒绝/缺口断言，禁止免耗成功 |
| TrapHexagon / 固定PARTIAL | :1097–1112；7+3L、Range9、护身符1 | HumanObject.cs:4596–4641；Map.cs:1752–1826；y上界严格<，怪等级≤施法者+2；500ms ShockTime/清目标、(5L+10)s、外围8实体 | Cast真假/ObjectSpell生成移除/Shock解除；SpellObject.cs:57–79关联ShockTime清实体；本轮保留拒绝/缺口断言，不造对称范围或免耗 |

- 表内HumanObject.cs、Map.cs、SpellObject.cs均指 `github/Crystal/Server/MirObjects/HumanObject.cs`、`github/Crystal/Server/MirEnvir/Map.cs`、`github/Crystal/Server/MirObjects/SpellObject.cs`，须逐段实读。
- 功能命令（根目录，预期exit0且实际测试数>0）：
  1. `cargo test -p mir-server --lib magic -- --nocapture`（覆盖用户要求的magic定向测试）。
  2. `cargo test -p mir-server --lib -- --nocapture`（含world相关测试；交新增world测试名与七行维度映射，不替换Fencing拒绝）。
  3. `cargo test -p mir-protocol`。
  4. `cargo test -p mir-client --lib net::tests -- --nocapture`。
  5. `cargo test -p mir-client --lib ui:: -- --nocapture`。
- 命令绿不等于技能完整；缺新增行为断言、过滤0例、仅移除Unsupported、自构造回环冒充真实服务端链、必需效果/状态/实体缺口，一律逐项PARTIAL。
- C协议子层最低线：真实服务端输出→编解码→客户端包消费/状态断言；仅枚举/回环不足。视觉单列结论，缺Crystal素材/运行时证据为PARTIAL，不把色块称完整视觉。
- 证据 `target/round51/b/**`：逐命令原始日志、七行行为/差异表、测试名→维度、真实包序/延迟、HEAD+porcelain；统一记录cwd、起止/时区、原始exit、通过/总数、路径/字节/SHA256。

### T 只读终审

- SUB_TASK_ID：`JJC-20261025-003-sub-c0e495c7`；owner：刑部 `xingbu / gpt-6.1-sol`；依赖：A/B交付且Lead本轮全量门禁完成。
- write scope：仅新建 `docs/testing/round51/round51-final-audit.md`、`docs/testing/round51/final-manifest.json`，各≤1MiB；此为用户指定路径，替代评审的docs/audit路径。实现及原始证据只读。
- 验收命令：`git diff --check`、`git rev-parse HEAD`、`git status --porcelain`；只读核各证据日志，复算SHA256/字节，核命令/起止/HEAD/原始exit/实际项数。
- 必须核七行矩阵、M1–M7、五条裁决、文档事实/历史保护、真实包链与视觉分层、所有PARTIAL；禁止代修、自跑cargo/ci、补跑ignored或用R50日志代本轮。
- 证据路径即两份审计产物；manifest登记Lead门禁和子单证据路径/字节/SHA256/HEAD/状态，报告列证据清单；两份审计产物写完后的SHA256由尚书memo外部登记，不做自引用哈希。
- PARTIAL：门禁非0、缺原始exit/实际项数、日志哈希不符、旧/跨树日志、任一缺口未列；发现问题报尚书退回原owner，不直接补证据。

### D 尚书协调（既有子单）

- SUB_TASK_ID：`JJC-20261025-003-sub-91431c94`；owner：`shangshu / gpt-6.1-sol`；write scope仅本契约与合法主单memo/派发/状态操作。
- 交付：三执行单ID映射、M1–M7吸收、DAG、命令、范围与风险；验收为文档≤200行、CLI show/validate真实回执、Lead收到启动包。
- 本次只完成派发；后续尚书验收须A/B→Lead全量→T齐备，读取父子show/todo/delegation_result后才可done进入Review，再按真实confirm_by交门下确认。

## 三、DAG与波次

- 最小拆分：3个执行子单+已有D协调单，共4条责任链；客户端证据并入B，避免A+C重复实现或同文件并写。
- 波1：Lead启动A；A交文档diff和验证→Lead文档批独立提交。
- 波2：Lead确认文档提交后启动B；B串行完成服务端、协议测试、客户端消费/状态证据，无第二功能写者。
- 波3：Lead核定向证据并执行唯一全量门禁；完成后启动T；T只读终审→尚书验收→Review→门下confirm。
- 依赖图：`D登记 → A → Lead文档提交 → B(A+C) → Lead全量 → T → 尚书验收 → 门下确认 → Lead功能提交`。
- 当前仅登记，不启动执行者；Lead收到回执再按波次启动，一次唤醒一人。Lead保留SUB_TASK_ID↔agent_id及实际provider/model，回报尚书memo。
- gpt：`subagent`显式 `provider:"sol-relay", model:"gpt-6.1-sol"`；deepseek：Lead用 `spawn_teammate`，继承runtime provider，不填sol-relay。不可用时报告路由缺口，不混用模型。

## 四、提交策略与门禁

1. 文档批先提交：A通过链接/差异检查后，仅Lead执行git；方案/评审/派发等治理材料是否随文档批纳入，由Lead按明确清单处理，不把冻结JSON顺带加入。
2. 功能批在后：B交diff/定向证据→Lead全量→T终审→尚书验收/门下确认后，由Lead提交功能批与允许的审计索引。六部和尚书均不git写。
3. Lead唯一全量命令：`pwsh -NoProfile -File tools/dev/ci.ps1`；不加SkipTests/SkipPostgres；`target/soak.lock`存在禁KillStale，不并发全量/ignored/soak。
4. 门禁绿须原始进程exit0、末行 `==> 全部通过（N 项）` 且无FAIL/ERROR；N取本次实际数，不预写12/12。保存命令/cwd、开始结束/时区、HEAD+porcelain、日志路径/字节/SHA256、逐项通过/总数；管道末端码不替代原始exit。
5. Lead只读以 `git log -2 --oneline`、`git status --porcelain`核提交顺序和树状态，不伪造恰好两条历史提交；文档提交是R50事实基线，不能覆盖Crystal/新功能证据。
6. 审计产物在门禁后写不使门禁失效：T明确列出两文件与哈希、所审功能快照。任何实现/测试/配置等变化都须重审相关证据及必要重跑，不能借审计例外越界。
7. 失败保留原日志与SHA，退回owner；重跑用新日志名，不覆盖历史/冻结证据。单文件>1MiB只登记本地路径/字节/SHA，不入库。

## 五、风险与不做清单

- M1/M2：毒云5护符+5毒、困魔1护符与耗材排除冲突，固定PARTIAL；禁止删除Unsupported或免费施法凑七技能全绿。其余五技能缺任何必需语义也逐项PARTIAL。
- M3/M4：真实命令与非零例数已固定；随机门槛/整数截断/延迟中心/状态实体/专包均需断言；生成协议只读、包号冻结，扩围须尚书裁定。
- M5/M6：Lead唯一git/全量；T严格后置；P12仅改当前状态，历史与既有差异不清除。
- 裁决1：缺实体/字段允许裁剪并PARTIAL，不造字段/规则/假支持；裁决2：协议最低证据不等于视觉或C整体PASS。
- 裁决3：文档先提交仅R50事实基线；裁决4：保留R50真实ci入口并现已登记增量标准；裁决5：PetSave/PetExp为既有未完成缺口，R52由有权角色后续登记关联ID，本次不越权建单、不虚构ID。
- 本轮不做：护身符/毒耗材系统、PetSave/PetExp、AOI全量验收、攻城旗/控制点/城门修理/领地、他人/怪物格间插值剩余、寻路、天气/光照、坐骑翅膀、小地图NPC、CallDefaultNPC、行会、R49 attack 3/25帧像素差、K键重绑、新玩法、包号/Crystal数值规则变更。
- 禁捷径：改DB、GM造状态、静态fixture冒充链路、旧日志/截图、缺素材早退成功、SKIP、硬编码成功、冻结证据改写；缺证据如实PARTIAL。

## 六、六部→尚书回填要求

- 启动先 `recall JJC-20261025-003`、`recall <自身SUB_TASK_ID>`；读本契约/评审/角色简报，按真实角色 `progress`、`todo`；同任务最多一个in-progress。
- `progress`只记当前事实/依赖/阻塞，不作为结果或父单通过；六部不state、不confirm、不替父任务done，不跨部再派实现者。
- 自身todo全completed、实际产出/证据齐备且无未收齐后代后，以 `delegate-result <自身SUB_TASK_ID> "产出/证据/风险/PARTIAL" --as <承接角色>`回填；不得空结果或口头成功。
- 回填必须含：实际文件、命令/cwd/起止/时区、原始exit、实际通过/总数、日志路径/字节/SHA256、HEAD+porcelain、七行结论及未决项；终态冻结前一次写齐。
- 实际阻塞用block并通知尚书/Lead，不冒名state，不删失败证据。尚书只读核父子show/todo/delegation_result并验收；Review→Done的confirm仅门下按pending_confirm.confirm_by执行。
- 登记顺序：先落盘本文件，再CLI登记三单、回填真实ID，执行memo、Doing、validate；回执如下。原始功能与全量测试由后续执行者及Lead完成。
- 看板回执：三执行单登记、memo、Assigned→Doing均exit0；validate exit0，109条schema v3.1；链接检查exit0，899条相对链接/2篇README；本文件112行。未启动执行者、未跑cargo/ci、未git写。
- 协调单收口异常：`delegate-result JJC-20261025-003-sub-91431c94 --as shangshu`被CLI角色权限拒绝exit4（允许列表不含delegate-result）；未重试/未换身份。show确认仍Doing、无结果，真实progress已留存并通知Lead；后续须有权治理角色依实际CLI能力处理，不假报Done。
