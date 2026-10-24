# R50 尚书省验收收口

- 父单：`JJC-20261025-002`；验收角色 `shangshu`；日期 2026-10-25（Asia/Shanghai）。
- **验收结论：PASS-WITH-PARTIAL**，与 [T2 二次终审](../testing/round50/round50-final-audit.md#L3) 一致；P01/P09/P11 闭环，保留 P02–P08/P10/P12。
- 依据：[派发验收契约](round50-shangshu-dispatch.md) §二/§四；逐个 CLI `show --json` 实读父单及全部 13 子单，并以 `list --all --json` 核对后代。
- 父单初始 Doing、owner=main、todos=[]；13 子单均 Done、delegation_result 非空，所有已登记 todo completed，无孙任务；Done 表示交付完成，不把历史 BLOCK 当成成功。
- 本次仅只读复核既有材料并新增本记录；未执行 cargo、ci.ps1、ignored 测试，未改实现/审计/冻结文件，未 git 写或推送。

## 一、提交身份与原始门禁

- 当前 HEAD、origin/main 跟踪引用、`ls-remote origin refs/heads/main` 均为 `4c41a456843806e2ea2e25dd7fbf8a755a18bc09`。
- 链：`4d4175a → 2a7e77e → b3be4af → 2ef4285 → 2e7e89b → 4c41a45`。
- [原始 meta](../../target/round50/final-gate2.meta.txt)：attempt=3，原始 `exit_code=0`，head=`2ef42854a2ed3b1bd3ad719dbc64eb19c2cf20d5`，main；07:12:39–07:15:50 +08:00。
- [原始日志](../../target/round50/final-gate2.log)：实算 186,193 B、SHA256 `47c8405c1a28c4bd651ff9abc21896ac7e4611617e573cab1f810035afeeeacb`，与 meta 一致。
- 按原始字节哈希、GB18030 解码尾部：汇总实际 12 行 OK；末行 `==> 全部通过（12 项）`；全文无 `[FAIL]`/`[ERROR]`。不以全文 `[OK]` 数代替门禁汇总项数。
- 12 项为 fmt、clippy、duo_smoke、workspace、协议生成、文档/包编号、TextKey、player-visible≤70、no-test=0、旧资源引用、图标、PostgreSQL。
- `git diff --name-only 2ef4285 HEAD` 仅两份审计文档，功能树不变；不声称门禁曾在后续文档提交 HEAD 上运行。
- T2 报告当前 28,560 B，SHA256 `3d2cb6e43c04494a32636d7127239cc7258b7a06a3060a0aee00df0ac958ae9c`。
- 新写本记录前 `git status --porcelain` 仅 [freeze-before](../testing/monster-native/player-resource-freeze-before.json)、[freeze](../testing/monster-native/player-resource-freeze.json) 两个已知未跟踪大 JSON。

## 二、逐子单验收（均以父单完整前缀 JJC-20261025-002 连接下列后缀）

| 子单 | 交付与复核证据 | 退出码/验收结论 |
|---|---|---|
| 01 / sub-b9c2de60 | [facts 证据](../../target/round50/facts/evidence.md)、facts.json、verify.py；108锚点=92hit+16deviation | 方案77/评审57/合并108均exit0；负探针exit1符合预期；接受历史取证，P12保留 |
| A / sub-dfd9ba4c | 三文件门禁行与追加节；[r50-a.log](../../target/round50/r50-a.log)保留前后diff与退出码；独立提交2a7e77e | show/diff/check与哈希命中exit0；零计数grep exit1为预期；范围验收通过，R49 verdict仍PARTIAL |
| B / sub-2b2da8e0 | 登录目录、事务后库存刷新、失败回滚；[B证据](../../target/round50/r50-b/README.md)六断言映射及原始日志 | 21/3/1/6用例exit0；server879 exit0；历史并行fmt exit1由最终gate fmt OK覆盖；接受，P07/P08保留 |
| C / sub-7d8f26cc | 独立面板/缓存/K守卫/双币/隔离；[C日志目录](../../target/round50/r50-c/) c1–c4与帧链证据 | 15/10/7 exit0；round2初次27+1失败exit101，TMP覆盖28通过exit0；初始零库存缺陷须连同C2验收，保留P05/P08 |
| D / sub-d38516b3 | 素材工具、82条GIndex manifest；[D证据](../../target/round50/r50-d/README.md)与cross-check | check/verify-icons均exit0、82/82；异常模拟exit1/2符合预期；接受，历史827条/772675B更正P06保留 |
| E / sub-177decbe | 显示/别名/日志三分、阈值15→0；[E证据](../../target/round50/r50-e/README.md)逐项去向 | 五验收命令exit0：no-test0、strict60≤70、keys一致、core48+9+2、embedded1；接受，P03/P04两deviation保留 |
| F / sub-5f8f99a4 | env输出覆盖、同快照/冻结清单/attack对照；[F证据](../../target/round50/r50-f/r50-f-evidence-summary.md) | 两ignored及attack两跑exit0；F1 7/7、F2 5/5，冻结70文件129219674B零变化；接受语义证据，T2覆盖其“无PARTIAL”自评，P05/P10保留 |
| H / sub-a78487f6 | 商城/客户端/i18n/授权与R50索引；回填自述756链接exit0但独立r50-h.log缺失 | 原H不独立背书756/0；契约diff-check当时未跑，转H2补证；按H2/T2修订后有条件接受，P12保留 |
| T v1 / sub-f2318e27 | [审计报告v1历史](../testing/round50/round50-final-audit.md#L103)，发现初始限量耗尽误判及P01/P09 | 接受BLOCK发现作为返工依据；只读审计无独立测试进程exit；旧gate exit0不消除P11 |
| C2 / sub-735cf6c2 | upsert/显示/夹取/禁购；[C2证据](../../target/round50/r50-c2/README.md)、[原始退出码](../../target/round50/r50-c2/commands-and-exit-codes.txt) | 红lib3失败/面板5失败exit101→绿15/13通过exit0；fmt/clippy exit0；3lib+3真实帧链边界闭环P11 |
| H2 / sub-e15245f8 | 五组文档返工及[链接原日志](../../target/round50/r50-h2/check-links.log)、[回执](../../target/round50/r50-h2/README.md) | 782相对链接、2篇README、0断链exit0；diff-check exit0；闭环P09，状态文案滞后并入P12 |
| T2 / sub-dbf6cc9a | [v2报告](../testing/round50/round50-final-audit.md)、[manifest](../testing/round50/final-manifest.json)与attempt3原始产物 | 接受PASS-WITH-PARTIAL；只读审计无新gate进程，核验原始exit0/12OK；关闭P01/P09/P11 |
| 尚书协调 / sub-9469ed8b | [dispatch](round50-shangshu-dispatch.md)162行、9执行契约、DAG/责任边界；main转录已有结果 | 接受已完成派发交付（非测试，无测试退出码）；结果旧HEAD=2e7e89b以本次4c41a45只读核验刷新 |

- A/B/D/F 曾在完整 facts 交付前使用在途 refs/权威数据开工，回填和证据已有披露；不追认其为严格等待 DAG 的执行记录，不伪造补跑。
- B 既有测试 helper 200→512有授权与真实目录帧数说明；E 模板词表替代TextKey及world/maps扩写有授权记录；H/H2扩展或历史引述改动按现存回执披露，未据此清零偏差。

## 三、阻断闭环与保留边界

- **P11 → C2 → 2ef4285 → T2**：首包/目录刷新中正总量、零剩余均售罄禁购；i_stock只选计数域；零总量仍不限量；红绿和最终同功能HEAD门禁均存在。非法负总量防御条件`<=0`与文档`==0`差异仍披露。
- **P01 → attempt3**：门禁在已提交功能树2ef4285运行，meta head/原始exit0/12OK/日志SHA吻合；后续两次提交只含审计文件，不把attempt2提交前身份解释提升为同快照证明。
- **P09 → H2**：782/0/exit0原日志替代未留独立证据的756声明；未恢复或伪造原H日志。
- **P02**：首跑clippy21/exit1原文被覆盖，仅历史说明，不能独立重建。
- **P03/P04**：E两deviation继续保留；扫描归零不等于完全遵循原TextKey方案。
- **P05**：F 25帧语义一致但3帧共2,580像素不同；C无像素回归基线。
- **P06/P07**：素材历史计数更正；真实样本库与素材可选依赖可能软跳过，测试计数不能证明真实分支必执行。
- **P08**：固定K/R51重绑、角色元宝、背包优先邮件、GIndex修正、缺货不刷库存等Crystal差异。
- **P10**：R49精确!Mon.dat映射、坐骑/Push/Jump/Dash/Blend117/Reverse/Effect/Sorting及self_moving/landed证据缺口。
- **P12**：facts为历史108锚点/16deviation；[商城文档](../world/gameshop.md#L39)仍记b3be4af，[商城P11](../world/gameshop.md#L384)/[客户端P11](../client.md#L909)仍写返工中；以T2及本记录校正，建议R51文档批同步。

## 四、合入建议与看板决策

- 建议接受R50界定范围并进入Review，保留全部上述PARTIAL；现有代码与审计已在远端main，不执行新的合入/推送，不认定严格Crystal复刻或逐像素一致。
- 尚书验收与T2 verdict对应一致：执行交付完整、三项闭环成立、九项PARTIAL继续保留；历史v1 BLOCK保留作为发现与返工链证据。
- CLI `memo ... shangshu ... --as shangshu`、`progress ... --as shangshu`、`done ... --as shangshu` 均exit0；done返回“执行完成，已提交尚书省审查”。
- 随后 `show --json` 实证父单 **Review**、owner=main，now为指定验收summary；没有readiness拦截。`ready_to_close=false`是原样字段，不替代真实状态/逐单证据判断。
- `validate --as shangshu` exit0：`看板校验通过：104 条任务符合 schema v3.1`。本次止于Review，不代替后续门下对口确认。
- 本记录为新增未提交文档；最终工作树预期仅本记录和两个既有冻结JSON未跟踪，未改变HEAD与审计文件。
