# R74 Lead 裁定记录

- 轮次：R74「普通混合跨图库测试锁」；主单 `JJC-20261028-001`。
- 日期（UTC）：2026-10-27T17:5x（见各证据 UTC）。
- 裁定人：Lead（三省六部流程；编码由 deepseek 团队成员执行，非编码由 gpt-6.1-sol 执行）。

## 裁定 1：C2 注释修正独立小提交（已执行）

- 计划 C2 要求 R73 遗留注释勘误独立成小提交、不混入功能批。执行：`c1d0c76`，仅 `crates/mir-client/src/ui/spellfx.rs:309` 一行注释，改为「`CMain.Time > NextFrame` 才推进一帧（`<=` 时先 `return`、不推进）」，零可执行差异。
- 依据：Crystal `Client/MirObjects/Effect.cs:81-96` 先 `if (CMain.Time <= NextFrame) return;` 再 `++CurrentFrame`；严格 NextMotion/对象寿命等价仍 PARTIAL。
- C 独立核验（R74-C）：确认单注释行、零可执行 diff，事实正确。

## 裁定 2：G5 `golden_magic` 注释措辞不精确（非阻塞，不修改）

- C 指出 `round74_normal_blend_lock.rs` G5 中 `golden_magic` 注释称该 scene「未经历缺帧」，实际与缺帧同 scene；但测试 :1291 另有独立整帧 oracle，断言强度不受影响。
- 裁定：记为**非阻塞 PARTIAL 披露**，不在本轮修改测试文件。理由：测试文件 SHA `a2b329ce…13cc1` 已贯穿 A/B/C 三方红绿与冻结证据；改注释会重置 SHA、使既有证据失效并需重跑全部负控，收益为零。
- 后续：如 R75 需触碰该测试，可在同一次提交中顺带澄清措辞。

## 裁定 3：R74 范围与证据边界

- 本轮为「普通 NORMAL/rate1 CPU Add 跨 Magic/Magic2 测试锁 + rate/Mode 差异合同」；生产仅注释一行（C2）。完整 DrawBlend/BlendMode/DXManager、NORMAL rate/INVLIGHT 设备状态、严格 NextMotion/对象寿命、其他技能、真实素材/GPU/设备视觉继续 PARTIAL，原号承接。
- 证据：A `target/round74/a/attempt-1/**`（37 项）、B `target/round74/b/attempt-1/**`（56 项）、C `target/round74/c/attempt-1/**`（runs/static-audit/delivery-sha256 与物理副本）；三方均未改主树实现，测试 SHA 一致。

## 裁定 4：题面 Crystal 锚点笔误（Lead 交接口误）

- Lead 给 D 的交接题面误写 `github/Crystal/Client/MirScenes/DXManager.cs` 与 `github/Crystal/Client/MirGraphics/Oracle.cs`。实读：DXManager 位于 `github/Crystal/Client/MirGraphics/DXManager.cs:353-389`（`case BlendMode.INVLIGHT` 在 :372）；`Client/MirGraphics/` 无 Oracle.cs，`find -iname '*oracle*'` 为空；`INVLIGHT` 枚举定义于 `github/Crystal/Shared/Enums.cs:75`，`LightSetting` 于 `:809`。
- 裁定：D 文档按实读锚点书写并披露题面错误，**不创建/不伪造 Oracle.cs**；Light/INVLIGHT 以 Effect.cs:20 与 DXManager.cs:372-375 实读为据。此为交接笔误，非 Crystal 结构缺失。
