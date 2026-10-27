# R74 普通混合跨图库测试锁

- 阶段：**D 文档批、待独立终审**；`pre-audit`，**PASS-WITH-PARTIAL 候选**；子单 `JJC-20261028-001-sub-05088f98`。
- 范围：普通 NORMAL/rate1 CPU Add 跨 Magic/Magic2 测试锁 + rate/Mode 差异合同；生产仅 `spellfx.rs:309` 一行注释修正，零可执行差异。
- DAG：A → B → C → Lead（提交与唯一全量门禁）→ D → 独立终审 → 尚书验收 → 门下确认。
- 提交链：`c80ef2a`（process）→ `c1d0c76`（独立注释）→ `7095808`（测试）→ `c222fe6`（C 报告与 Lead 裁定）→ HEAD（D 未提交）。
- HEAD 快照：`c222fe65b92b629299c007cf23c60481e2fb53b2`；D 纯文档不重跑 cargo/CI。

## 门禁与证据索引

- Lead 唯一全量 attempt-1：HEAD `c222fe6`，EXIT0，12/12 OK、0 FAIL；UTC `2026-10-27T17:49:40Z`–`17:54:13Z`，门禁后 crates 零差异。
- [门禁原日志](../../../target/round74/gate/attempt-1.log) 213918B，SHA256 `e2b1c25ac15d8207b0c786386fcb193279d9956c6f75eac770264784ef9771b3`。
- [门禁 meta](../../../target/round74/gate/attempt-1.meta.txt) 573B，SHA256 `4e622efe3f95b2094731790dae45b107d4c3499ac8c36ea88cccd430482a8c75`。
- [A 证据](../../../target/round74/a/attempt-1/)：37 项（以 A manifest 证据集合为准），基线5/0/0、回归38、M1–M5物理副本RED101；expected冻结17:05:48Z，首个actual17:05:49Z；新增allow/ignore=0，A生产零改。
- [B manifest](../../../target/round74/b/attempt-1/manifest.json)：8物理副本、1297 tracked SHA1核对、nlink1/symlink0；5/0/0、38/0/0、clippy/fmt EXIT0，M1–M5 RED101→恢复绿，独立Python oracle 18整帧三源一致。
- [C 报告](round74-c-evidence.md)：PASS-WITH-PARTIAL、必改0；独立archive复跑5/0/0与38/0/0、clippy/fmt EXIT0、五负控复现；独立Node oracle仅4个G1整帧。
- [Lead 裁定](../../process/round74-lead-ruling.md)：C2注释独立提交、G5措辞非阻塞披露、范围边界。
- [计划](../../process/round74-plan.md) / [门下审议](../../process/round74-menxia-review.md) / [派发](../../process/round74-shangshu-dispatch.md)。
- [差异合同](round74-normal-blend-lock.md) / [pre-audit清单](final-manifest.json) / [外置摘要](manifest.sha256) / [D自检证据](../../../target/round74/d/attempt-1/)。
- [client §5.39](../../client.md#539-r74-普通混合跨图库测试锁) / [roadmap §二十二](../../roadmap.md#二十二第74轮-普通混合跨库锁切片)。

## 披露与 PARTIAL 台账

- G5 `golden_magic` 注释称“未经历缺帧”的 scene 实际为同一scene；:1291另有独立整帧oracle，按Lead裁定非阻塞，不改测试SHA。
- A基线`0ee34e1`，B/C基线`7095808`；测试SHA一致，生产差异仅C2注释。C的4帧复算不扩称B的18帧；冻结顺序无外部可信时间戳证明。
- game-assets未入库；合成RGB565仅alpha0/255，G4直接RGBA验证CPU取整；真实素材/设备视觉未签核。
- 本次授权合同名为`round74-normal-blend-lock.md`，替代旧计划中`round74-normal-blend-contract.md`命名；源锚点纠偏见合同。
- 原号台账：[R74计划107–117](../../process/round74-plan.md#L107)、[门下31–32](../../process/round74-menxia-review.md#L31)、[C报告42](round74-c-evidence.md#L42)；完整DrawBlend/BlendMode/DXManager、NORMAL rate/INVLIGHT设备状态均PARTIAL。
- GAP-1/2、R70-C2及GAP-5/R70-C1双记、R70-C3、R71时钟/对象、R72 B plan-M4、R73-T1、GAP-3/4/6原号承接。
- R71-C7永久PARTIAL、E1仅补救CLOSED；R69条件5、R68/R49–R70/R61–R67照承；严格NextMotion/对象寿命、其他技能、真实素材/GPU/设备视觉/全UI/跨后端/Live音频未闭合。
