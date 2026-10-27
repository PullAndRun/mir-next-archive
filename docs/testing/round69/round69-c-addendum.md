# R69-C 追加核验：历史保全补救

- verdict：**PASS-WITH-PARTIAL（必改 1 关闭为「披露+重做闭合」，条件 5 记 PARTIAL）**。
- 范围：仅核 B 历史保全补救；只写本页与 `target/round69/c/addendum-1/**`；未重跑/修改 B 证据、未 commit/push/ci.ps1。
- 实证：`target/round69/c/addendum-1/verification.json` 与 `patch-verification.json`；两核验脚本最终 EXIT0。初次脚本因默认 GBK 解码失败，明确 UTF-8 后成功；失败未写 B。

|必做项|状态|独立核对结论|
|---|---|---|
|1 PROVENANCE 诚实性|PASS|全文60行实读：明确 a77 原始红/隔离/日志被默认重跑覆盖、无备份且不可恢复；保留物5件逐件实算与表中SHA/bytes一致；今后新attempt/新work-root，禁止复用；未宣称条件5全关。|
|2 最终 SHA 红绿|PASS|R1–R4及R4b逐件核report、首败/目标首断言、原始runs的0→1→0；基线/恢复5/5；green验收15/15/0/0；46/46 artifact全部实算SHA+bytes吻合（超过抽6件要求）。|
|3 只读承诺|PASS（观测限度）|attempt1 124文件最晚04:16:43.951Z；attempt2 1261文件最晚04:18:03.751Z；attempt3 7514文件最晚04:33:33.261Z；全部≤04:35Z；6组out/console mtime与runs完成UTC同秒，收尾写早于冻结；mtime非不可篡改历史证明。|
|4 裁决落地|PASS；条件5 PARTIAL|Lead裁定19行已实读，明确历史原件不可恢复、披露+新最终SHA首败证据重做；没有把重做件冒充a77历史件；历史完整性继续PARTIAL。|
|5 边界|PARTIAL（并发HEAD偏差）|A工具/矩阵/测试实算均为冻结SHA，tracked diff空；采样HEAD为8fbb71b90e1e1d7fd4b9757c18697b69789208d1，提交时间12:50:45+08，与派发基线9e8d9e8不符；已通知Lead，不归因B越界；B冻结文件无新写入。未commit/push/ci.ps1是本C操作事实，无法仅凭git status独立证明其他代理从未执行。|

|变体|目标/实际红|目标首败断言（expected→observed）|基线/红/恢复|
|---|---|---|---|
|R1|G2/G2|offset_before_crop：true→false|0/1/0|
|R2|G3/G2+G3|restore_call.argc：1→2；全局首败G2 restore_args：['oldBlend']→['oldBlend','rate']|0/1/0|
|R3|G4/G4+G5|mask_draw.position：point.X/Y→point.X+MaskX/point.Y+MaskY|0/1/0|
|R4|G5/G5|classification_counts.other_read：0→1|0/1/0|
|R4b|G5/G5|classification_counts.deserialize_read：4→3|0/1/0|

- 最终测试 `1f3bb16927ff877da2e8c33b53c9f238e2caf37e95c6f24f5a13e5117a65c315`；工具 `1fdf4069a7841c507c931a3b74d4f03fbf549373522d146019bd135cc1f5e729`；矩阵 `e513550fffdf91c9809218a52a601270741b84622e6dc4cd89796063387b0416`，五first-failure冻结字段一致。
- 五patch绝对路径存在且SHA一致；恢复副本SHA均回到afda6ebb…；在内存应用patch重建五变异文件，SHA逐项等于记录after值，未写原证据。首败原文与真实report mismatch一致。
- 非阻塞表述偏差：PROVENANCE:46将R2 suite首败写为G2 restore_call.argc；实际G2首败是restore_args，G3才是restore_call.argc；同文表格:39及first-failure准确披露两者。首败report_line部分指observed字段而非mismatch起始，原文断言仍完整保留。
- 本追加不改变首轮报告的历史BLOCK，不要求恢复不可得原件；关闭的是Lead裁定后的补救动作，条件5历史保全仍PARTIAL；不背书像素/GPU/生产实现一致性。
