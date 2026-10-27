# R70 御史台独立终审

- **verdict：BLOCK；必改 1 项（证据历史分类失真），实现复跑通过；修复披露后可再评 PASS-WITH-PARTIAL。**
- 审计官：gpt-6.1-sol；审计 HEAD `5f0d78139245d549c4bd71c4fe2329ce96df77d9`；2026-10-27 UTC。
- 提交链：`72d9313 → b11deac → 6a08c65 → 89daaaa → 5f0d781`；审计未提交/推送。
- 完整命令、UTC 起止、EXIT、计数及日志摘要：[runs](../../../target/round70/t/attempt-1/runs.json)、[static](../../../target/round70/t/attempt-1/static-audit.json)。

| 必核项 | 判定 | 实证与边界 |
|---|---|---|
| 1 提交/范围 | PASS | log 链一致；6a08c65..5f0d781 仅 C 两报告+D 六文档；`git diff 6a08c65 -- crates tools/dev/ci.ps1` 空；初始 status 仅两历史 freeze ??。 |
| 2 门禁原字节 | PASS | 仅 attempt-1；原 log/meta bytes/SHA 全匹配；汇总12 OK/0 FAIL、exit侧录0；meta head/command/operator/UTC一致。 |
| 3 独立复跑 | PASS | 全新 target、独立 TEMP/TMP/TMPDIR、`CARGO_PROFILE_DEV_DEBUG=0`；9/0、2/0、2/0、133/0；clippy/fmt EXIT0。 |
| 4 清单 | PASS | 13/13 path/bytes/SHA；9/9 指定 commit blob；无自引用；README/blend按D pre-commit tracked=false声明核对；外置SHA一致。 |
| 5a C诚实性 | FAIL | 首轮 render-lib 实际129/4权限失败，被报告与runs误记成D8050未执行；见必改。C2四有效red/恢复green及M2初次E0599披露成立。 |
| 5b A/B隔离 | PARTIAL | B冻结b188febc及M1独立patch/671B、恢复SHA、st_nlink=1自洽；A中间红76e573/绿970b80非同SHA，A已披露，不作最终冻结红绿证明。 |
| 5c G1–G7/边界 | PASS | 当前帧offset一次/先裁剪、false绝对坐标、Add公式与代码一致；公开文档未扩称完整DrawBlend/映射/真实视觉。 |
| 5d D过程偏差 | PARTIAL | README/manifest未载临时文件名偏差；据派单披露两次误带后缀后mv；最终无多余文档，本报告显式承接，非阻断。 |
| 6 卫生 | PASS | 新ignore=0；新allow仅entities:1892私有8参impl，固定API/正交开关的局部lint豁免；无skip/0 tests；审计文档UTF-8无BOM/LF；links与diff-check EXIT0。 |

- 独立执行 UTC `07:12:27.908580Z–07:13:44.400869Z`；四套测试 passed/failed/ignored/filtered=`9/0/0/0、2/0/0/0、2/0/0/0、133/0/0/0`；共146例，无忽略/过滤。
- 命令依次为 `cargo test -p mir-client --features mir-render/legacy --test {round70_effect_blend_offset,round68_image_offset_wire,round68_effect_offset_live}`（分别执行）、`cargo test -p mir-render --lib`、`cargo clippy -p mir-render -p mir-client --all-targets -- -D warnings`、`cargo fmt --all -- --check`；全部EXIT0。
- `py -3 tools/docs/check_links.py` EXIT0：1755链接/626 target跳过；`git diff --check` EXIT0；报告落盘后再验见static.final_verification。
- 门禁 UTC `06:54:11Z–06:58:15Z`，HEAD=`6a08c652df794a9b19a8324a180d589bcce33d24`，operator=lead，EXIT0；本审计未重跑ci。
- gate log **211886B** SHA256=`6acfc87fb6602dd510e94c72a212455f3a571c29d18957dcc103c85c0f390e28`；原日志含宿主旧编码，按原字节保全，未转换。
- gate meta **465B** SHA256=`4cbb0ee8b2db1edd7b8b27c268279704cd55633c4d0e2396c3c0dbd0e66efe86`。
- manifest **5654B** SHA256=`b4b1174545a4ebabcee9350b653d1dfc800f1691c4f6c3b5d7cd31871047d965`；test SHA256=`b188febc6a67e26ee14a815672db0d597bcc7e054423a5c103d9b41bddc1df84`。
- C2 M1/M2-corrected/M3/M4均编译成功→running9→目标assert红EXIT101→恢复9/0 EXIT0；首败L461/L622/L506（另L758）/L885及像素值与冻结测试相符；初始M2 E0599未运行tests，不计有效红。
- Add为`min(255,dst+(src*alpha+127)/255)`；alpha=0整点跳过，否则目标alpha255；false保留(x,y)，仅true消费当帧offset。旧Over与Add并不等价。

**必改 R70-T1（C/Lead负责历史勘误，D负责摘要/清单刷新）：**
- 最小复现：对照[原日志](../../../target/round70/c/attempt-1/logs/04-render.log) L21–24、L163–180、L189，与[C报告](round70-c-evidence.md) L9/L22、[C runs](../../../target/round70/c/attempt-1/runs.json) L8。
- 原日志明确编译成功、running133、`129 passed; 4 failed`；四败均`PermissionDenied`/OS code5（临时目录或文件）；不是ring D8050未执行。其他4条Cargo日志确有D8050。
- README L23与manifest disclosures首项沿用“5条/未运行tests”的错误概括。保留原件并追加勘误，区分4条编译阻断与1条执行后权限失败，刷新下游摘要/清单；无需改功能。C2/本次133/0证明后续恢复，不能改写首轮历史。

- **PARTIAL承接**：magic映射差异/帧号/时序；spellfx/UI调用点未迁移；完整DrawBlend/公共单API/全矩阵；INVLIGHT/rate；状态恢复、mask/shadow、GameScene条件/照明、尺寸/居中；真实素材/GPU/设备视觉；C首轮环境限制及权限失败历史；D临时文件名过程偏差；A不同测试SHA的历史迭代；R69条件5及R68、R49–R67原编号遗留。
- 范围：只写本报告与`target/round70/t/attempt-1/**`；未改实现/测试/manifest/历史freeze，未commit/push，未跑`tools/dev/ci.ps1`。结论仅覆盖CPU rate=1 Add+可选当前帧offset子集。
