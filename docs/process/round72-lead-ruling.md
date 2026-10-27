# R72 Lead 裁定：C BLOCK 两项必改的处置

- 依据：`docs/testing/round72/round72-c-evidence.md`（42 行/6,291B/sha256 `5f6addd04b331f5341f0b6032332e768d76fb0e7585571e6f7d1fea630f7ed6a`，BLOCK，必改/待裁量 2 项）；子单 `JJC-20261027-009`。
- A 交付冻结 SHA：`spellfx.rs` `df572677…78be6`（386 行/17,755B）、测试 `58f64ab2…baea`（1,660 行/65,322B），主树 `sha256sum` 实算一致；工作树仅 A 两文件 + 2 历史 freeze JSON（另 `docs/testing/round72/` 未跟踪为审查产物）。
- 裁定日期：2026-10-27（Asia/Shanghai）；基线 HEAD `69b2e0c`。

## 必改 1（B 变异矩阵与获批 M1–M7 不一一对应，manifest.md:103 声明不实）

**证据**：B `attempt-1` 实际为 base1640/clamp/移 center/offset/Magic/Over(保 offset)/1200ms，缺获批 plan-M2（Swing→Bolt 检测 start20）与 plan-M7（去 `from_object_spell` 检测非149旧行为）；plan-M4 精确写法（按帧宽高居中）仅被近似执行（center 锚点替换）。

**处置**：
1. 属主 B 已追加 `target/round72/b/attempt-2/**`（attempt-1 只读未改）：
   - plan-M2：`patched 0d3188fb…9d13` → RED EXIT101、首败 `:939:5` (384,256) actual `[48,136,176,255]`=`expand565(0x3456)`=Magic2[1670] Bolt start20 诱饵 vs expected `[0,0,0,255]` → 恢复 pre SHA → GREEN 9/0。
   - plan-M7：`patched 968b625d…b302` → RED EXIT101、唯一失败 `r72_g5_legacy_sources` `:1491:5` (381,229) actual `[248,4,0,255]`=Magic2[1650] vs expected `[0,0,0,255]` → 恢复 → GREEN 9/0。
   - `attempt-2/link.md` 逐项对账并**撤回** attempt-1「一一对应」结论（原件不改写）。
2. 残余差异：plan-M4 精确「按帧宽高居中」写法未被 B 补跑（其近似项仍红 G3），但 **C 已在自身隔离副本独立完成获批全部七语义**（含尺寸居中、Bolt、去来源；build0→红101→恢复绿，测试 SHA 不变），语义覆盖在独立证据中闭合；B 的近似项如实记为残余差异（PARTIAL 披露），不构成功能必改。
3. 判定：**必改 1 CLOSED（含 M4 残余差异披露）**；不要求 B 再补 attempt-3，除非 T/尚书复核发现新缺口。

## 必改 2（同 legacy feature 的 clippy `-D warnings` EXIT101，两处既有 needless_return）

**证据**：C `baseline-clippy-run.json` 基线生产同命令亦 EXIT101，命中 `crates/mir-render/src/minimap.rs:353:13`、`crates/mir-render/src/tiles.rs:311:9` `needless_return`；B attempt-2 双口径实跑记录（官方不带 legacy EXIT0；带 legacy 探针 EXIT101 同两点）。
**既有性**：两文件相对 `f37a402` 及本轮 A 交付均零 diff；R70 已按同一性质裁定为既有 lint 债（`docs/process/round70-lead-ruling.md` §差异1）。
**处置**：官方唯一门禁 `tools/dev/ci.ps1` 的 clippy 口径**不带** `--features mir-render/legacy`，实测 EXIT0；带 legacy 的探针失败为既有债的 PARTIAL 披露项，非 R72 引入、非本切片必改；登记为后续清理候选（可在未来轮次以独立小提交修复，不在 R72 功能批内触碰）。
**判定：必改 2 CLOSED（裁量为 PARTIAL 披露项，不阻断）**。

## 处置结论

- 两项必改均按「补证 + 勘误 + 裁量」闭合，**无功能/测试必改**；C 的 BLOCK 不再阻断，待 C2 追加核验确认后由 Lead 提交功能批（仅 A 两文件）并运行唯一全量门禁。
- 本轮仍为 **149 PoisonCloud/Magic2 切片 PARTIAL**：严格 NextMotion、对象寿命、rate 0.8/src×0.8×alpha、Light3、其他 26 技能 GAP-5、4000ms 产品瞬态窗均继续原号承接；不得宣称 Crystal 全量等价或 R72 全量通过。
- 门禁后 D 文档须引用：C 报告 sha、本裁定、B `link.md`、C2 addendum（待出）。
