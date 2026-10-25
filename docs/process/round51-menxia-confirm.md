# R51 门下省最终确认：PASS-WITH-PARTIAL

- 日期：2026-10-25（Asia/Shanghai）；主单 `JJC-20261025-003`；确认官：门下省 gpt-6.1-sol。
- 独立确认结论：**PASS-WITH-PARTIAL 收口成立**，P03/P10 closed；尚书已明确接受 PARTIAL/R52 并转下一轮，没有将七技能完整实现作为本次已达成事实。
- 依据[尚书验收](round51-shangshu-acceptance.md)、[T4b v4 终审](../testing/round51/round51-final-audit-v4.md)、[v4 manifest](../testing/round51/final-manifest-v4.json)及门禁原字节；本次不重新运行 cargo/全量门禁，不将历史报告代作独立重跑。

## 原字节抽核（全部重新计算一致）

| 路径 | bytes | SHA256 |
|---|---:|---|
| [docs/process/round51-shangshu-acceptance.md](round51-shangshu-acceptance.md) | 8137 | `38df93cb58492cdfe62941a4afcd44828540d09302d2d416b66585857489e5cb` |
| [docs/testing/round51/round51-final-audit-v4.md](../testing/round51/round51-final-audit-v4.md) | 14223 | `8bff8f8f72e78b349064e6a123b26a41f5bb29258ebf9c14b8ccce4d41b5510b` |
| [docs/testing/round51/final-manifest-v4.json](../testing/round51/final-manifest-v4.json) | 13511 | `142a9ba8e28693adea67ab6997a6514aea59c90b00d6905aa69958cf1eb55535` |
| [target/round51/final-gate8.log](../../target/round51/final-gate8.log) | 188937 | `817f531c078679bb8d4519576b1a155f2119aae3b92e7fcfec502fda6ff28510` |
| [target/round51/final-gate8.meta.txt](../../target/round51/final-gate8.meta.txt) | 378 | `3c27884305764dfb0fd667d54081871e2b1acc449d8e8ae0612a6cc46dd99ee9` |

- 尚书文档实为74行；manifest JSON解析成功、verdict及P状态一致。gate8原日志含非UTF-8字节，UTF-8 reader拒读后仅在内存按GB18030解码，原件未转码/覆盖。
- meta实有 command=`pwsh -NoProfile -File tools/dev/ci.ps1 -KillStale`、cwd=`/c/Users/diana/Documents/github/mir-next`、exit_code=0、branch=main、head=`8cbe71332ff3dc2037b9bea4ab564f6fb3ff0e75`。
- UTC `2026-10-25T11:39:18Z–11:41:48Z`、150秒；原日志3215–3226行12项OK、3227行全部通过。逐项数字exit未另存，不将OK分类冒充独立数值回执。
- attempt1–7逐一重算log bytes/SHA、meta SHA及exit均与manifest一致；attempt1/4 exit1失败史完整保留。

## Review→Done 依据及边界

- 起始 `show --json` 实为Review、todos=[]，output指向尚书验收，summary为“R51 验收：PASS-WITH-PARTIAL，P03/P10 closed，PARTIAL/R52 转下轮；门禁 attempt8 合法、终审 v4”。
- `list --all --json`筛得10个直接子单，逐个show均Done、delegation_result非空、todos全完成；B4=`sub-e2cb39cb`，`sub-28585983`是历史B2；T3历史BLOCK是交付结果而非当前阻断。
- live HEAD=origin/main=`ec53dfa4c11aa5187beb08e51ec3547e53b81c12`，main；`git diff 8cbe713..HEAD`仅[README](../testing/round51/README.md)、v4报告、v4 manifest三份审计文档，功能树未变。
- `0d27e5e`为B4功能修复/测试，`8cbe713`为追溯/索引文档；认可v4对P03/P10的有界闭合，不扩大为全部技能、PvP、socket、边界均完成。
- 接受的PARTIAL：P01/P02耗材及完整语义未实现；P04火墙逐对象/InTrapRock未闭合；P07敏感回纠待证；P08借用Walking/Walk，原生dash未验；Frost玩家目标仍待证。

## R52补证清单（尚书明确接受转下轮，11项）

1. PoisonCloud/TrapHexagon耗材及完整语义；FrostCrunch玩家目标PvP。
2. 真实默认MagicInfo逐技能耗蓝/冷却，包括ShoulderDash 2500ms。
3. 真实非空Slow；Slow/Frozen概率两侧及玩家/怪边界。
4. StormEscape buff→LevelMagic严格包序、专门跨图运行例。
5. 玩家Frozen实际拒攻运行断言。
6. 152/153错误预测格+已有motion敏感回纠、Standing/清队列不可删证明。
7. 火墙逐对象ProcessSpell、InTrapRock系统边界。
8. 原生dash帧/视觉/声音/时序。
9. guild Freezing来源映射、真socket端到端；wire链不等于socket级。
10. 多槽durability>0叠加与Crystal 1965/1966插槽属性。
11. 同种子后续RNG状态对照；坏装备+有效buff、无耐久+非零附加；收紧Freezing7时长上界。

## 非阻断残余与保全

- 当前[magic.rs:5823/5908](../../crates/mir-server/src/magic.rs#L5823)普通附加错引1965应1859；[4899](../../crates/mir-server/src/magic.rs#L4899)及B4矩阵“四包”应五个位置。尚书明确转后续勘误，不隐去、不改原证据。
- Freezing7理论Frozen上限11，现有5..=12断言偏宽；收紧运行断言归R52，不在本次越权改crates。
- ignored日志/B4原件跨机复核须另行保全，manifest不能替代原字节；两份monster-native冻结JSON继续排除。
- 本门下只写本过程文档及CLI看板，不改crates/证据原件，不commit/push。

## CLI真实流转回执

- `memo JJC-20261025-003 menxia … --as menxia`：exit0；stdout=`📝 JJC-20261025-003 任务记忆：menxia`；stderr空。
- `state JJC-20261025-003 Done "门下确认：PASS-WITH-PARTIAL 收口，PARTIAL/R52 已接受" --as menxia`：exit0；stdout=`⏳ JJC-20261025-003 高风险操作 Review→Done，已进入 PendingConfirm，需 门下省 执行 confirm approve`；stderr空。
- 请求后show核实：PendingConfirm，from_state=Review，target_state=Done，requested_by=menxia，confirm_by=menxia，requested_at=`2026-10-25T12:06:41Z`；未跳过独立指定确认。
- `confirm JJC-20261025-003 approve "PASS-WITH-PARTIAL收口成立" --as menxia`：exit0；stdout=`✅ JJC-20261025-003 confirm approve → Done`；stderr空。
- `validate --as menxia`：exit0；stdout=`✅ 看板校验通过：115 条任务符合 schema v3.1`；stderr空。
- 最终show断言exit0：state=Done、pending_confirm=null、todos=[]；实际确认时间=`2026-10-25T12:06:58Z`。output仍为尚书验收文档，终态不再改写看板。
- CLI限制：含斜杠备注可能净化；因此memo以“与R52”表达，完整范围以本文为准。无exit3/角色/收口拦截；没有把validate schema校验冒称游戏行为测试。
