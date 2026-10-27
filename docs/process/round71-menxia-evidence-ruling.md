# R71 门下省证据裁决（一次书面审议）

- **裁决：接受 C7 缺口以永久 PARTIAL 处置；PASS-WITH-PARTIAL；本裁决新增必改 0（实现/测试 0、证据补救 0）。** E1 可按下列条件关闭补救动作，C 原 BLOCK 历史保持有效；C2 追加独立复检通过前不得视为已解除 C→D 阻塞。
- 主单 `JJC-20261027-008`；审阅者 gpt-6.1-sol；独立命令 UTC `2026-10-27T09:57:32.578437+00:00` 起，均 EXIT0；本次不运行 Cargo/CI、不 commit/push、不作看板终态确认。

## A．C7 原要求与现实
- [原审议:31](round71-menxia-review.md#L31)、[派发:28](round71-shangshu-dispatch.md#L28)、[计划:116–117](round71-plan.md#L116)均要求失败 attempt 全保留、证据只增不改；本次明确认定 A 未满足 C7，不能改记 PASS。
- [A manifest:64/76](../../target/round71/a/attempt-1/manifest.md#L64)列出的 red-c5c7d8.log / green-c5c7d8.log 实际缺失；独立 `find target -iname "*c5c7d8*"` EXIT0、输出空，吻合 [C missing-index:5–6](../../target/round71/c/attempt-1/missing-index.json#L5)。
- [勘误:7–17](../testing/round71/round71-a-erratum.md#L7)披露覆盖、不可恢复及最终证据替代；当前不存在可独立验证，覆盖原因/不可恢复性依据 Lead 披露，不能声称通过当前搜索证明全部历史。保留物 [sha-final:2](../../target/round71/a/attempt-1/sha-final.txt#L2)、[test-sha-green:1](../../target/round71/a/attempt-1/test-sha-green.txt#L1)仅证明 SHA 记录残留 `c5c7d8d6598047c746c29d66dbf89334bac345cd8dc427832d71fa59a14537b8`，不证明该阶段红绿结果。
- 接受结案理由：缺失限于中间阶段，最终冻结红绿和 C 独立复算另有原件，功能验收不依赖缺失日志；永久披露保全违约，禁止补写历史。此为已发生缺口的明确例外处置，不削弱今后 C7。

## B．E1 关闭口径
- [C:3/21/56](../testing/round71/round71-c-evidence.md#L56)的 BLOCK 正确；[勘误:13–19](../testing/round71/round71-a-erratum.md#L13)明确撤回两日志存在及阶段完整保全的表述，结合本裁决足以关闭“记录失真”的补救动作，历史保全缺口仍永久 PARTIAL。
- **不要求 A 重写 manifest。** 原清单留作错误历史，追加勘误优先解释其:64/76及“均有日志”标题；重写/覆盖会违背只增不改并消灭审计轨迹。C2/D/终审的现行索引须并列引用原清单、勘误、本裁决，不得裸引原清单作为历史完整性证明。

## C．功能与推进
- [C:13/20/27/30–41](../testing/round71/round71-c-evidence.md#L13)确认最终红绿及五变异独立复现；本门下核读这些报告并实算原件 SHA，未重新运行测试。A [最终红:1–5/19–20/57/60](../../target/round71/a/attempt-1/red-final2.log#L57)为成功编译、running8、2/6、EXIT101；[最终绿:15/17](../../target/round71/a/attempt-1/green-final2.log#L15)为8/0、EXIT0。
- 最终测试冻结 SHA `64ff3f597e2530ce0d32abb21eb09b36638083384f27638a0b29177ac262b422` 与 [red2:1](../../target/round71/a/attempt-1/test-sha-red2.txt#L1)、[green2:1](../../target/round71/a/attempt-1/test-sha-green2.txt#L1)及现行测试字节一致；不以此补认 c5c7d8 阶段。
- **允许 Lead 在 C2 追加复检 PASS-WITH-PARTIAL、必改0后推进 D 文档、独立终审、尚书验收，仅限149 FireWall六帧 elapsed 循环与格锚点/offset接线。** 不重改实现/测试、不要求为本次纯文档裁决重跑门禁；后续 crates/工具变化仍触发原 C8 新 attempt 门禁。
- [Lead:14–17](round71-lead-ruling.md#L14)、[gate meta:2–10](../../target/round71/gate/attempt-1.meta.txt#L2)：功能 `cedf903`；门禁 `16644d28df142a0b99264c9700674ff701fdea97`、EXIT0、12/12；日志实算212193B/SHA `d1fcbf77dfe7751eefeaf03b5a6f8af70e564eacf94a8fc15b10789d2b2c8173`。本次只核原字节及meta，未重跑门禁。
- 独立 git log/show 确认16644d2提交的是 C 初稿 PASS-WITH-PARTIAL；采样 HEAD 仍16644d2，当前 C BLOCK 为未提交修改、勘误/Lead裁定未跟踪，须由 Lead 后续文档批保留提交关系，不能称它们已在门禁 HEAD 内；`git diff cedf903 -- crates`及`-- tools`均空。

## D．永久台账与原号承接
- R71-C7：两份 c5c7d8 中间日志不可恢复，永久 PARTIAL；E1补救闭合不等于 C7 全满足。R69 条件5 a77历史原件同样永久 PARTIAL，不以本轮清零。
- [计划:132–138](round71-plan.md#L132)原号保留：GAP-5/R70-C1非149旧行为、1620起手/其它调用点/真实素材视觉；R70-C3全局尺寸/高度/居中；GAP-1/2/R70-C2实际复用rate=1 Add、rate0.8及`src×0.8×alpha`近似未实施、DrawColour/INVLIGHT/完整DrawBlend未闭合。
- GAP-3/4/6 mask/tint/shadow（shadow只解析不消费）、场景条件offset/Light3；本切片显式偏差：严格NextMotion、初始100ms取整、每Process一步、149持久场/26移除，4000ms仅事件窗口；R68/R69条件5及R49–R67/R61–R67历史原号全部承接，真实设备/跨后端/全UI未验收。

## E．R69 一致性
- [R69 Lead:12–15](round69-lead-ruling.md#L12)采用披露+最终SHA新证据、不冒充历史原件及此后目录只增不改；[R69 C追加:3/26](../testing/round69/round69-c-addendum.md#L26)只关闭补救、不改首轮BLOCK；[门下确认:16](round69-menxia-confirm.md#L16)明确条件5永久PARTIAL。本裁决遵循相同口径，不将最终复算当作历史还原。

## F．裁决边界与 C2 通过条件
- 本裁决仅解决 E1/C7 可接受性；**不等于 R71 全量通过、生产完整等价、完整Crystal FireWall迁移或真实素材/视觉签核**，独立终审、尚书验收及指定门下终态确认仍须各自实际履责。
- **C2（本次追加复检，区别于原审议 C2 测试先行）通过条件：** ①独立核勘误及本裁决，确认两日志不存在且不补造；②原 manifest、C BLOCK及16644d2初稿历史均保留、当前索引附勘误；③核最终红绿/冻结SHA、B/C五变异及原门禁SHA/meta，crates/工具无门禁后变化；④明确 E1补救关闭、C7永久PARTIAL及D原号台账；⑤新追加报告/独立证据只增不改，给PASS-WITH-PARTIAL、实现/测试/证据必改0。任何一项失败则继续BLOCK，不能沿用初稿PASS跳过。
- 独立审阅逐文件行数/bytes/完整SHA及命令UTC/cwd/EXIT见 [checklist](../../target/round71/menxia-evidence/attempt-1/checklist.json)、[commands](../../target/round71/menxia-evidence/attempt-1/commands.log)；C BLOCK 56行/SHA `e41c1edb4483b13a64c1cf90f07a59188c7d4e091a6d4d51b4c9dd2a72df6629`；勘误19行/SHA `18861622c61a60d78d46f471c43a4c973e6740c64134ff69f3864468c9d83e53`；Lead裁定17行/SHA `f3a4adaa0a87b245293a74e5060faa1b5782258612345f45d0bedad490f756df`。
