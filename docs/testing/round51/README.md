# R51 终审索引

当前状态：**R51 已收口 Done（PASS-WITH-PARTIAL）**：T1 BLOCK（13f7c78）→ B2 返工 → attempt5 PASS（f500269）→ T2 BLOCK（P03/P10 open）→ B3（1fe36dd）→ attempt6 PASS（42a49e0）→ T3 BLOCK（耐久过滤/Option 包序）→ B4（0d27e5e）→ attempt7 PASS（edad41b）→ 追溯性校正（8cbe713）→ attempt8 PASS（8cbe713）→ T4b PASS-WITH-PARTIAL（v4）→ 尚书 ACCEPT-WITH-PARTIAL → 门下确认 Done。

- [T1 终审报告与 P01–P10 返工清单](round51-final-audit.md) ｜ [final-manifest v1](final-manifest.json)
- [T2 独立复审（BLOCK，审 f500269，P03/P10 open）](round51-final-audit-v2.md) ｜ [manifest v2](final-manifest-v2.json)
- [T3 独立复审（BLOCK，审 42a49e0：耐久过滤、Option 包序假绿）](round51-final-audit-v3.md) ｜ [manifest v3](final-manifest-v3.json)
- [B4 追溯性校正说明（B4=sub-e2cb39cb；sub-28585983 实为 B2）](round51-b4-traceability-note.md)
- [T4b 独立复审（PASS-WITH-PARTIAL，审 8cbe713；P03/P10 closed）](round51-final-audit-v4.md) ｜ [manifest v4](final-manifest-v4.json)

治理收口：[尚书验收（ACCEPT-WITH-PARTIAL）](../../process/round51-shangshu-acceptance.md) ｜ [门下确认（Review→Done）](../../process/round51-menxia-confirm.md)。

门禁原件：attempt8 `target/round51/final-gate8.log` 188,937 B sha256 `817f531c…` exit 0 12/12 HEAD=8cbe713（attempt1–7 历史保留，attempt1/4 为失败史）。P09 文档时态修正见[礼部返工记录](../../process/round51-libu-docs.md#L46)；B3 返工边界见[尚书派发](../../process/round51-shangshu-rework-dispatch.md)，B4/T4 契约见[尚书 B4 派发](../../process/round51-shangshu-rework-dispatch-b4.md)。旧审计产物不覆盖。
