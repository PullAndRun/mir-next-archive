# R51 尚书省验收：ACCEPT-WITH-PARTIAL

- 日期：2026-10-25（Asia/Shanghai）；主单 `JJC-20261025-003`；验收官：尚书省 gpt-6.1-sol。
- 结论：**ACCEPT-WITH-PARTIAL**，接受刑部 T4b **PASS-WITH-PARTIAL**；P03/P10 closed，其余明确 PARTIAL 与 R52 责任转下一轮。
- 本结论仅在下列限定范围内验收，不等于七技能完整实现，不直接认定主单无条件 Done；本次仅申请 Doing→Review。
- 实读原派发、两次返工契约、B4 追溯说明、B4 摘要/矩阵及附录、T4b v4 报告与 manifest、门禁原字节及 meta；未改 crates、审计/日志原件，未 commit/push。

## 提交树与证据归属

- 门禁/终审对象 `8cbe71332ff3dc2037b9bea4ab564f6fb3ff0e75`；实际链 `f500269→1fe36dd→42a49e0→0d27e5e→edad41b→8cbe713`。
- 本次 live HEAD=origin/main=`ec53dfa4c11aa5187beb08e51ec3547e53b81c12`，main；相对 8cbe713 仅三份 v4 审计/索引文档，功能树未变。
- B3=`sub-3b310e70`，T3=`sub-d0ea04bd`，B4=`sub-e2cb39cb`，T4/T4b=`sub-1c3b4c52`，协调=`sub-91431c94`；均属主单且逐个 show 核验 Done/非空结果。
- 全部 10 个直接子单逐个 show 均 Done，有结果、无未完成 todo、无后代；父单初态 Doing、todos=[]。子单 Done 表示交付已回填，T3 的 BLOCK 历史继续保留。
- `sub-28585983` 是历史 B2，不能替代 B4；B4 原头部误号以摘要/矩阵校正附录与[追溯说明](../testing/round51/round51-b4-traceability-note.md)为准，原件不覆盖。
- 派发依据：[原派发](round51-shangshu-dispatch.md)、[B3/T3 返工](round51-shangshu-rework-dispatch.md)、[B4/T4 返工](round51-shangshu-rework-dispatch-b4.md)。

## 原字节证据（SHA256）

| 路径 | bytes | SHA256 |
|---|---:|---|
| [target/round51/final-gate8.log](../../target/round51/final-gate8.log) | 188937 | `817f531c078679bb8d4519576b1a155f2119aae3b92e7fcfec502fda6ff28510` |
| [target/round51/final-gate8.meta.txt](../../target/round51/final-gate8.meta.txt) | 378 | `3c27884305764dfb0fd667d54081871e2b1acc449d8e8ae0612a6cc46dd99ee9` |
| [docs/testing/round51/round51-final-audit-v4.md](../testing/round51/round51-final-audit-v4.md) | 14223 | `8bff8f8f72e78b349064e6a123b26a41f5bb29258ebf9c14b8ccce4d41b5510b` |
| [docs/testing/round51/final-manifest-v4.json](../testing/round51/final-manifest-v4.json) | 13511 | `142a9ba8e28693adea67ab6997a6514aea59c90b00d6905aa69958cf1eb55535` |
| [target/round51/final-gate8-summary.log](../../target/round51/final-gate8-summary.log) | 2409 | `f1e026d5379ef5b95d80316cf58e0a693b81252d69a3badf0d9a4995185ed0ea` |

- 上述独立重算一致，manifest JSON 解析通过；报告、manifest 与摘要的 verdict、P 状态、映射、门禁及 R52 一致。历史 README 指纹对应被审树，后续 ec53dfa 索引更新已单独核对。
- attempt8：`pwsh -NoProfile -File tools/dev/ci.ps1 -KillStale`；cwd `/c/Users/diana/Documents/github/mir-next`；UTC 11:39:18–11:41:48、150s、raw exit=0、head=8cbe713、branch=main。
- GB18030 解码原日志 3215–3226 行逐项实有 OK：fmt、workspace clippy、duo_smoke 串行、workspace test、协议生成、文档链接/包编号、TextKey、player-visible 棘轮70、no-test棘轮0、旧资源格式、物品图标、PG 串行；3227 行“全部通过（12 项）”。
- 逐项数值 exit 未另存，仅按门禁真实 LASTEXITCODE 分类为 OK；不冒称逐进程数字回执。attempt1–7 log/meta 指纹和原 exit 均与 manifest 一致；attempt1/4 exit1 原件完整保留。
- B4 三来源/耐久、真实 Frozen 非退化与 Blink 五个 is_some 前置断言支持 P03/P10 closed；36 例运行由 B4、attempt8 及 T4b 独立复跑支撑。本尚书未再跑 cargo/全量，未做源码变异红灯实验。

## 明确接受的 PARTIAL

- **P01 PoisonCloud、P02 TrapHexagon**：耗材系统与完整语义未实现；保留 UnsupportedSpell 真拒绝、不扣蓝，不能称技能完成。
- **P04**：ShoulderDash 火墙仅入格结算、命中即停；逐对象 ProcessSpell 与 InTrapRock 系统边界保留。
- **P07**：World→编码帧→decode→WorldSnapshot 两例通过；152/153 的错误预测格+已有 motion 敏感回纠尚未证明消费不可删。
- **P08**：Walking/Walk 动作声音借用；原生 dash 帧/视觉/时序未验收。
- FrostCrunch 玩家目标 PvP 未覆盖；P03/P05/P06 closed 各限于 v4 范围，不吞并下列补证缺口。

## R52 转下一轮（11 项逐项保留）

1. PoisonCloud/TrapHexagon 耗材及完整语义；FrostCrunch 玩家目标 PvP。
2. 真实默认 MagicInfo 的逐技能耗蓝/冷却，包括 ShoulderDash 2500ms。
3. 真实非空 Slow；Slow/Frozen 概率两侧及玩家/怪边界。
4. StormEscape buff→LevelMagic 严格产包顺序、专门跨图运行例。
5. 玩家 Frozen 实际拒攻运行断言。
6. 152/153 错误预测格+已有 motion 的回纠、Standing/清队列不可删证明。
7. 火墙逐对象 ProcessSpell、InTrapRock 系统边界。
8. 原生 dash 帧/视觉/声音/时序。
9. guild Freezing 来源映射、真 socket 端到端；当前真实 wire 链不等于 socket 级。
10. 多槽 durability>0 装备叠加及 Crystal 1965/1966 插槽属性覆盖。
11. 同种子后续 RNG 状态对照；坏装备+有效 buff、无耐久+非零附加边界；收紧 Freezing7 时长上界。

## 入库卫生与禁止捷径

- 接受并复核 T4b §G/§H 的有界结论：R51 被审改动31路径，无新增/改动 blob>1MiB；最大350031 B，既有1228554 B资产计划是基线同 blob。
- 无 target/.env/密钥路径入库；新增行私钥/AWS/GitHub token 形态扫描0命中仅限本轮范围，不外推全历史。
- 两份 monster-native 冻结 JSON 仍未跟踪排除；44844165/45219533 B 及 manifest 指纹重新核验一致，不纳入建议提交。
- B4 为有效装备过滤+五例+五项存在断言；未新增 allow/ignore/skip，未弱化旧断言，真实 Frozen helper 未手工注毒；包序和协议未为判绿调整。
- `git diff 716db42..HEAD -- crates/mir-protocol tools/dev/ci.ps1` 为空；250/251 与 CI 无本轮改动。断言可失败性来自静态逻辑及正向运行，不冒充变异实验。

## 提交建议、后续文档批与遗留风险

- 已提交 `0d27e5e`（B4 生产修复/测试）与 `8cbe713`（映射追溯/索引）；v4 审计产物文档提交为 `ec53dfa`，无需重复提交或重跑相同功能树门禁。
- 建议 Lead 将本验收记录随后续文档批独立处理；保全 ignored 的门禁/B4 原件，排除 target、两份冻结 JSON；本验收官不 commit/push。
- **非阻断文字残留明确转后续文档批，不影响本轮验收**：[magic.rs](../../crates/mir-server/src/magic.rs) 5823/5908 普通附加错引1965应为1859；4899及B4矩阵“四包”应表述五个位置。原证据保留，采用新增勘误。
- 后续文档批准确注明 Freezing7 理论 Frozen 上限11，现有断言5..=12偏宽；收紧运行断言属于 R52 补证，不在本次改 crates。
- 风险：耗材/PvP、概率边界、严格包序/跨图、152/153回纠、火墙及原生表现、guild/socket/多槽/RNG 尚有缺口；跨机复核必须取得 ignored 原始字节，manifest 不能替代原件。
- 看板实际回执（均 `--as shangshu`）：memo exit0，`📝 JJC-20261025-003 任务记忆：shangshu`；progress exit0，CLI 输出进展并显示 `[0/0]`。
- done exit0，`✅ JJC-20261025-003 执行完成，已提交尚书省审查`；summary 使用指定的 PASS-WITH-PARTIAL 文本，output 指向本文。
- validate exit0，`✅ 看板校验通过：115 条任务符合 schema v3.1`；最终 show exit0，state=Review、todos=[]、pending_confirm=null，Doing→Review 已真实生效。
- CLI 无 readiness/权限拦截；progress 显示文本经 CLI 清洗及截断，不能单独代替完整验收范围，以本文、memo 与 done summary 为准；未请求 Review→Done。
- 本文落盘后链接检查 exit0（1216 条相对链接、2 篇 README）；`git diff --check` exit0。原 tracked 文件无修改，新文件仅本文，另两份历史冻结 JSON 继续未跟踪排除。
