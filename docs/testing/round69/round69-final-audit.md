# R69 刑部独立终审（T）

- verdict：**BLOCK；必改 1 项**。退回 B 测试契约适配阶段，再由 D 刷新清单/文档、T 新 attempt 复审；不得交尚书宣称通过。
- 审计 HEAD：`f156f0e5d2be0b1cbcabcf237823627000f3c1b7`；2026-10-27 UTC。使用 verification-before-completion：以本次实际退出码裁决。
- 仅新增本页与 T attempt-1 证据；未 commit/push、未运行 ci.ps1、未改 manifest/实现/测试/旧证据。
- 证据：[runs.json](../../../target/round69/t/attempt-1/runs.json)、[static-audit.json](../../../target/round69/t/attempt-1/static-audit.json)、[logs](../../../target/round69/t/attempt-1/logs/)。静态命令 `py -3 target/round69/t/attempt-1/static_probe.py` 05:16:59–05:17:01Z EXIT0；补核 supplement_probe.py 05:19:20–05:19:21Z EXIT0。

|必做项|裁决|独立核验依据（命令/UTC/EXIT详见 runs 与 static）|
|---|---|---|
|1 提交链/生产零差异|PASS|rev-parse=f156f0e；8fbb71b→492b20c→5297dea→74d1608→f156f0e；`git diff 8fbb71b..HEAD -- crates` 与 `git diff 492b20c..HEAD -- crates` 空；工具及 round69 所有 py/json 与 HEAD blob 按 CRLF 归一一致。|
|2 门禁原字节/整改范围|PASS|两 log/meta 原字节匹配；attempt-1 只有链接项失败，428/428 断链解析后均位于 target；492b20c 仅豁免 resolve 后的 target 子树，非 target 断链仍报错。|
|3 清单独立复算|PASS|19/19 bytes+SHA；13 tracked blob 经 cat-file 实算吻合且对象 ID 等于 delivery_head=74d1608；6 untracked 如实，外置摘要匹配；gate/UTC/12/0一致，pre-audit 明示终审 pending。|
|4 隔离独立复跑|**FAIL**|契约 suite EXIT1：14 passed/1 failed/0 skipped；唯一 T9 HEAD 硬编码不兼容。独立审计 EXIT0，G1–G5 5/5。|
|5 B attempt-3|PASS|46/46 artifacts（超≥8要求）SHA+bytes；五 patch/首败/恢复/原始runs交叉一致；五隔离副本各140文件，实查 nlink=1；attempt-1/2 最晚04:16:43.951Z/04:18:03.751Z≤04:35Z。|
|6 C两轮/裁定|PASS；条件5 PARTIAL|50/26行及SHA实算吻合；Lead裁定19行、PROVENANCE60行；必改1补救以披露+重做闭合，历史原件缺失没有冒充恢复。|
|7 文档诚实性|PASS（当前复跑结果须随返工更新）|README60行，无三项禁语；G1–G5各抽样及6 gaps/3 candidates全部原字段一致；client§5.34/roadmap§十七一致限定生产未做；check_links EXIT0、diff --check EXIT0。|
|8 卫生|PASS（观测范围）|本轮 crates 无差异，无新增 Rust ignore/allow；两份 player-resource freeze 初末均未跟踪，git ls-files 空、无操作写入，观测期SHA/mtime一致。|

- 必改1定位：[contract-tests.py:82](contract-tests.py#L82) 固定 BASELINE_HEAD=9e8d9e828742f866f8b3a42f4bf352c21ce9667c；[T9:1883](contract-tests.py#L1883) 要求当前HEAD完全相等。交付提交后即必红，不是生产渲染缺陷；静态零差异不能把实际 EXIT1 改记15/15。
- 必改1验收：B应让提交后的交付树能按受支持隔离参数验证同一冻结源码与无生产改动，并保留失败能力；新 attempt 下15 passed/0 failed/0 skipped、EXIT0，再更新相关SHA/清单/文档并独立复审。T未修改测试或伪装HEAD。
- 本次完整命令：`py -3 docs/testing/round69/contract-tests.py --work-root target/round69/t/attempt-1/work --out target/round69/t/attempt-1/work/test-results.json`；05:15:07.534–05:15:31.502Z，EXIT1；[原始结果](../../../target/round69/t/attempt-1/work/test-results.json)。help先读EXIT0；TEMP/TMP/TMPDIR均设T自有tmp，子变体各自隔离。
- `py -3 tools/testing/round69_render_contract.py --root .`：05:15:31.502–05:15:32.216Z，EXIT0；5/5、missing_required=0、missing_optional=0。
- `py -3 tools/docs/check_links.py`：05:15:32.217–05:15:32.843Z，EXIT0；1686检查/571 target跳过。`git diff --check` 05:17:01Z EXIT0；本页落盘后再次核验见runs。
- 门禁 attempt-2：492b20c，04:59:06–05:01:36Z，EXIT0，12 OK/0 FAIL；log **210828B / ced721954f9b9e446c4f663cd1efff723be583a41f773fd2e728b76b4633d084**。
- attempt-2 meta：**741B / 0ba8cd38dad86120f632cc8a4edeb548ca872fa7c3bd9ad7a7f79ea1f85b0e0f**；attempt-1：8fbb71b，04:50:46–04:57:57Z EXIT1，11 OK/1 FAIL；log269825B/fb453575f2216b1cb562c42c0f525a2cfd365b4c1cd03d14711ffc2664374fde，仅文档链接与包编号失败。
- 清单实算：19/19、B46/46；head_blob SHA由原blob字节计算，CRLF仅用于工作树等价判断；snapshot_head保留5297dea，pre_audit_refresh明确delivery_head=74d1608且不替代终审。

|B变体|实际红/首败|基线→变异→恢复|
|---|---|---|
|R1|G2；offset_before_crop true→false|0→1→0，恢复5/5|
|R2|G2+G3；全局G2 restore_args；目标G3 restore_call.argc 1→2|0→1→0，恢复5/5|
|R3|G4+G5；mask_draw.position加入MaskX/Y|0→1→0，恢复5/5|
|R4|G5；classification_counts.other_read 0→1|0→1→0，恢复5/5|
|R4b|G5；classification_counts.deserialize_read 4→3|0→1→0，恢复5/5|

- 文档抽样：G1 default_offSet=false/offset_calls=1/先裁剪前偏移；G2 rate=1/restore_args=[oldBlend]；G3 NORMAL SourceAlpha/One、INVLIGHT BlendFactor/InverseSourceColor；G4主体mask同point/Width/Height；G5 97文件/6行/8标识符/other_read=0，均与矩阵一致；9对象全部逐值比对。
- C首轮SHA：903b8b7f40acd264e3e255b1a0c7fecc7a6b9aadc864e737486413b3f51736bc；追加：6de3a2c711c275666db8f4493c9ec3e17f4639e8dce74d29d3f79816727c73a6。PROVENANCE：28cb9be4f68611601e0e99bf1010d99ef7aa0152c3de2e4402bb5ad5a25f7075。
- 偏差披露：D原指定 --audit --check-observed 不支持（python EXIT49/py EXIT2），受支持 --root . 实测绿；PROVENANCE:46 R2笔误已由C追加准确更正；mtime仅是观测，不是不可篡改历史证明。
- T执行偏差：包装打印GBK编码异常发生在check_links子进程EXIT0落盘之后；静态初次UTF-8 meta按GBK读取失败、补核初次未剥Markdown尖括号断言失败，均仅修T探针后成功，未删suite失败或修改被审原件。
- PARTIAL承接：生产offset消费/六gaps及R70三候选待实施；DrawBlend rate/INVLIGHT、mask/全UI/场景条件offset未完成；shadow维持解析不消费；真实素材视觉、设备/pixel/GPU一致性未签核；条件5 a77历史原件不可恢复，永久PARTIAL；R49–R68其余历史PARTIAL原号承接。
