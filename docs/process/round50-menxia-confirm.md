# R50 门下省最终确认

- 父单：`JJC-20261025-002`；确认角色：`menxia`；日期：2026-10-25（Asia/Shanghai）。
- **复核结论：approve；R50 收口为 PASS-WITH-PARTIAL。** P01/P09/P11 闭环，P02–P08/P10/P12 保留。
- 复核方式：只读既有证据和 Git 元数据；未执行 cargo、ci.ps1 或 ignored 测试；仅新增本确认文档。

## 一、验收链路完整性

- [尚书验收](round50-shangshu-acceptance.md)完整实读，当前实物 8,831 B / 62 行，覆盖13子单、返工、原始门禁和残余项；输入快照的8,328 B已过时。
- [T2终审v2](../testing/round50/round50-final-audit.md)及[manifest](../testing/round50/final-manifest.json)与尚书结论一致；v1 BLOCK仅保留为发现与返工历史。
- CLI show实证父单Review，updatedAt=`2026-10-24T23:34:34Z`，todos=[]；recall具备规划、门下评审、派发、执行、C2/H2/T2、尚书验收链。
- CLI list实读全部13个直属子单：均Done、delegation_result非空，已登记todo全部completed，无孙任务；未将旧T的BLOCK结果冒充通过。
- 当前HEAD与origin/main跟踪引用均为`4c41a456843806e2ea2e25dd7fbf8a755a18bc09`；远端实时查询以尚书已留证结果为依据，本轮未重新联网。
- git log最近六笔为`4c41a45 / 2e7e89b / 2ef4285 / b3be4af / 2a7e77e / 4d4175a`。
- `git diff --name-only 2ef4285 HEAD`仅两份审计文件；后续文档提交不改变被门禁验证的功能树。
- 初始status实际为尚书新增验收文档及两个既有冻结JSON未跟踪，共3项；无跟踪文件改动。与尚书交付记录一致，比任务快照多出的验收文档为已知产物。

## 二、门禁原始证据

- [原始meta](../../target/round50/final-gate2.meta.txt)：attempt=3、exit_code=0、main、head=`2ef42854a2ed3b1bd3ad719dbc64eb19c2cf20d5`。
- 原进程窗口：2026-10-25 07:12:39–07:15:50 +08:00；本轮未重跑或从摘要推算退出码。
- [原始日志](../../target/round50/final-gate2.log)实算186,193 B，SHA256=`47c8405c1a28c4bd651ff9abc21896ac7e4611617e573cab1f810035afeeeacb`，与meta/manifest相符。
- 原始字节哈希；GB18030解码尾部实读12行OK，末行为“全部通过（12 项）”，全文`[FAIL]`/`[ERROR]`计数0。
- 12项包括fmt、clippy、duo_smoke、workspace、协议生成、文档/包编号、TextKey、player-visible≤70、no-test=0、旧资源引用、图标、PostgreSQL。
- manifest的65条files记录已逐条重算字节与SHA256，65/65匹配、0不符，命令exit0。
- 门禁通过严格归属于功能提交2ef4285；不声称attempt3曾在4c41a45上运行。

## 三、P项闭环与非阻断风险

- **P01闭环**：attempt3在已提交功能树运行，原始meta、日志哈希、退出码与汇总相符。
- **P09闭环**：H2实测782相对链接、2篇README、0断链/exit0；旧H的756声明仍仅历史。
- **P11闭环**：C2修复正总量限量商品首包/目录刷新耗尽误判；3 lib+3真实帧链边界红绿和最终门禁证据齐备。
- **P02保留**：首跑clippy21/exit1原文被覆盖，不能独立重建。
- **P03/P04保留**：E模板显示词表替代TextKey及授权world/maps扩写两项deviation。
- **P05保留**：F语义一致，但3/25帧共2,580像素不同；C无像素回归基线。
- **P06/P07保留**：素材历史826→827条、771854→772675 B更正；可选真实库/素材依赖可软跳过。
- **P08保留**：固定K/R51重绑、角色元宝、背包优先邮件、GIndex修正、缺货不刷库存等Crystal差异。
- **P10保留**：R49精确!Mon.dat映射、坐骑/Push/Jump/Dash/Blend117/Reverse/Effect/Sorting及self_moving/landed证据缺口。
- **P12保留**：facts历史108锚点/16deviation不等于当前源行认证；产品文档身份与返工状态滞后。
- P12具体为[商城:39](../world/gameshop.md#L39)仍记旧HEAD，[商城:384](../world/gameshop.md#L384)及[客户端:909](../client.md#L909)仍写返工中；以T2校正，列入R51文档批同步。
- 非法负总量`<=0`防御与文档`==0`表述差异继续披露；不影响本次正总量耗尽/零总量不限量边界闭环。
- A/B/D/F未严格等facts交付的历史协作偏差继续保留，不补造严格DAG执行或首跑日志。

## 四、与R49口径一致性及最终确认

- [R49索引](../testing/round49/README.md#L66)区分“门禁12/12 PASS”与“整体PASS-WITH-PARTIAL、P01–P08未清”；R50同样区分行为门禁与范围验收。
- R50的12项来自当前原始日志实测；不将历史12/12预写为未来固定项数，也不混用两轮P编号。
- **最终确认：同意Review→PendingConfirm→Done，以PASS-WITH-PARTIAL收口已界定R50范围。** 不清零剩余PARTIAL，不认定严格Crystal复刻或逐像素一致。
- 看板仅经CLI以menxia执行memo、state、show核验指定confirm_by后confirm approve、validate；实际结果补记如下。
