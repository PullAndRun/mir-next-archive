# R51 终审索引

当前状态：**T1 BLOCK（审 13f7c78）→ B2 返工 → attempt5 合法 PASS（f500269）→ T2 BLOCK（P03/P10 open）→ B3 返工（1fe36dd）→ attempt6 合法 PASS（42a49e0）→ T3 BLOCK（P03 耐久过滤 / P10 Option 包序）→ B4 返工提交 0d27e5e → attempt7 合法 PASS（edad41b）→ T4 复审（追溯性校正后重派）。**

- [T1 终审报告与 P01–P10 返工清单](round51-final-audit.md)
- [final-manifest：T1 文件与证据清单](final-manifest.json)
- [T2 独立复审报告（BLOCK，审 f500269，P03/P10 open）](round51-final-audit-v2.md)
- [final-manifest v2：T2 文件与证据清单](final-manifest-v2.json)
- [T3 独立复审报告（BLOCK，审 42a49e0：损坏装备耐久过滤、Option 包序假绿）](round51-final-audit-v3.md)
- [final-manifest v3：T3 文件与证据清单](final-manifest-v3.json)
- [B4 追溯性校正说明（子单映射 e2cb39cb，B4 证据附录）](round51-b4-traceability-note.md)

P09 文档时态修正见[礼部返工记录](../../process/round51-libu-docs.md#L46)；B3 返工边界见[尚书返工派发](../../process/round51-shangshu-rework-dispatch.md)，B4 返工与 T4 契约见[尚书 B4 派发](../../process/round51-shangshu-rework-dispatch-b4.md)。B4 后门禁 attempt7 原件：`target/round51/final-gate7.log`（189,032 B，sha256 `f734bc55…`，exit 0，12/12，HEAD=edad41b）。旧审计产物不覆盖，后续复审另取新证据。
