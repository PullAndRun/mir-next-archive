# R70 御史台终审补充复评

- **verdict：PASS-WITH-PARTIAL（R70-T1 CLOSED）；剩余必改 0。**
- 审计官：gpt-6.1-sol；审计 HEAD `4d3d84b3da6e73484ce908ce2b6c824c8575e315`；2026-10-27 UTC。
- 证据：[runs](../../../target/round70/t2/attempt-1/runs.json)、[static](../../../target/round70/t2/attempt-1/static-audit.json)、[logs](../../../target/round70/t2/attempt-1/logs/)。

| 必核项 | 判定 | 独立实证 |
|---|---|---|
| 1 勘误真实性/原件保全 | PASS | [勘误](round70-c-erratum.md)计数/分类与原日志一致；`git show 5f0d781:docs/testing/round70/round70-c-evidence.md` 完整字节仍为当前C报告前缀，仅追加纠正；首轮runs保留。 |
| 2 下游刷新/清单 | PASS | README:23及disclosures[0]明确4条D8050+1条129/4权限失败、C2不改写历史；实际14/14 artifacts独立复算全匹配，无清单/外置SHA自引用。 |
| 3 提交/范围/门禁 | PASS | 提交链核对；`git diff 6a08c65..HEAD -- crates tools/dev/ci.ps1`空；初始status仅两历史freeze，SHA与首轮终审一致；唯一门禁原字节/元数据保持。 |

- [render原日志](../../../target/round70/c/attempt-1/logs/04-render.log)：L21编译完成12.70s、L24运行133；L163/169为`entities.rs:2244:44`两例、L174为`text.rs:790:46`、L179为`tiles.rs:372:45`；随后四处均PermissionDenied/code5；L189为129 passed/4 failed/0 ignored。
- `01-round70/02-wire/03-live/05-clippy.log`各有D8050且未运行tests；04无D8050。保留原件+追加勘误+刷新下游三要素齐备，R70-T1 CLOSED。
- [首轮终审](round70-final-audit.md)原样：36行/5213B，SHA256 `19e97348850f2f7b505450cd01f3d84d18ff3c553c0eabced4ee80e9f0f361f8`；5f0d781中尚不存在，4d3d84b新增原件。
- [manifest](final-manifest.json)：7057B，SHA256 `e05cd4b0654d2eb78434f9f43396baad80aa1925c0822eb969a0d995f8416387`；[外置摘要](manifest.sha256)同值；14项含新增勘误，README/C证据的新SHA/bytes均匹配。
- 门禁仍为6a08c652/attempt-1、EXIT0、12/12；log 211886B/`6acfc87fb6602dd510e94c72a212455f3a571c29d18957dcc103c85c0f390e28`，meta 465B/`4cbb0ee8b2db1edd7b8b27c268279704cd55633c4d0e2396c3c0dbd0e66efe86`。
- 两freeze仍为44844165B/`70845aefd4dd689a55ab0618b36bbf692a4b33ebb5ee2f3a662dedc17a3dc9de`、45219533B/`c7d896e7f5eee4706b943912c0c564d10d61c98d163143f051c2d632daecde32`，与首轮static及R68:22一致；未找到raid50具名记录。
- 非阻断记录：README:75与manifest checks内13项为D历史自检，实际清单14项已复算；勘误:5结果行定位L180应读L189，计数事实正确；不据此新增必改。
- L0静态核验36项PASS，实测1.259s；JSON解析、links及diff-check EXIT0；无需升级到Cargo；落盘后终验见static.final_verification。
- **PARTIAL承接**：[首轮终审:35](round70-final-audit.md#L35)完整台账及[勘误历史](round70-c-erratum.md)：完整DrawBlend/rate/INVLIGHT、magic/帧号/时序、spellfx/UI迁移、状态/mask/shadow/照明/尺寸居中、真实素材/GPU设备、A不同SHA历史、D文件名过程偏差、R69条件5及R68/R49–R67遗留；C首轮环境权限失败历史保留。
- 范围：只写本补充报告与`target/round70/t2/attempt-1/**`；未改实现/测试/manifest/首轮报告/freeze，未commit/push，未执行ci.ps1或cargo；仅复评CPU rate=1 Add+可选当前帧offset子集。
