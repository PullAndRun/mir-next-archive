# R75 基线勘误：A first-actual 补录的版本差（只增不改）

- 性质：**只增不改**；原 C 报告、C2 追加报告、A 补录、Lead 裁定/勘误均未修改。
- 事实经过：A 补录 `target/round75/a/attempt-1/first-actual-claim-amendment.md` 在撰写过程中曾短暂落盘 3,216B 版本（sha256 `600460581f3ca1eba26227bf0f903862a1ba43b299a6e341addc3b5b4f8de8b6`）；A 于收尾时定稿为 **3,210B/32 行**（sha256 `26eaf4faa657661f2dea8aeaf27e9d0165973d7d5defd743358f68f410046224`），`.sha256` 与 PROVENANCE 均对应定稿版本。Lead 在 04:31Z 的早期读取引用了中间版本并写入 C2 任务书，属**任务书基线错误**；Lead 裁定 `docs/process/round75-lead-ruling.md` 与勘误 `docs/testing/round75/round75-evidence-erratum.md` 已引用定稿版本。
- 权威口径：**以 `26eaf4fa…`（3,210B/32 行）为 A 补录唯一权威版本**；`60046058…`（3,216B）为撰写中间快照，未被任何后续记录引用为结论依据，不构成证据矛盾。两者均为新增文件，未覆盖/改写任何既有证据（测试 SHA `ecff550d…`、A manifest `c10a84fe…`、B 记录、两 freeze 均未变）。
- 影响：C2 的两条 FAIL 均由该任务书基线错误引起，非证据保全缺陷；E1 仍为 CLOSED-AS-PARTIAL，E2 为声明级 PARTIAL。C2 报告与 checks 保留原文，不修改。
- 后续：由追加复核（C3）确认本勘误后以 PASS-WITH-PARTIAL/必改 0 口径收尾；历史 PARTIAL 原号承接，不称生产寿命修复或与 Crystal 严格等价。
