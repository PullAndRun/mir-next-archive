# R73 御史台独立终审

- 审计对象：HEAD/origin/main `34d5e17a46ff9c001c63f67e7d1ff275c1eb40e9`；基线 `93e1140`；T 独立复核，2026-10-27 UTC。
- **verdict：BLOCK；必改 1（证据字段一致性）；本次未发现限定实现的功能缺陷。**
- **T-D1（D/Lead）**：`final-manifest.json:262` 的 `tools/assets/analyze_usage.py.head_sha256=86d53716ea2e8e6c4b425ef61f2c7b7ce66222ca0db66d255c74e34eacbd521e`，实际 delivery_head 原始 Git blob SHA256=`e2d9f327f43b52e96a203a75ee3fb8f730db990e8e28ef40460162e98f9a40c1`（67,171B）。前者是 CRLF 工作树摘要（68,769B）；Git blob OID 正确，CRLF→LF 后内容全等，非功能变更。
- 最小复现：`py -3 -c "import subprocess,hashlib; print(hashlib.sha256(subprocess.check_output(['git','show','55f61c4:tools/assets/analyze_usage.py'])).hexdigest())"`；对照清单 `head_sha256` 即失败。修复方向：保留正确的工作树 `bytes/sha256`，将 `head_sha256` 改为原始 blob 摘要，或改名并明确声明该字段为工作树摘要；刷新外置摘要和相关终态自检。T 未代修交付物。

- 独立复跑统一环境：`CARGO_TARGET_DIR=target/round73/t/attempt-1/cargo-target`；TEMP=TMP=TMPDIR=`target/round73/t/attempt-1/tmp`（均使用绝对路径）；`CARGO_PROFILE_DEV_DEBUG=0`。
- R73 `--features mir-render/legacy --test round73_thunderbolt_hit_mapping`：UTC15:42:41–15:43:11，**9/0/0，EXIT0**。
- 指定回归同 feature：R71 8/0/0（15:43:11–13）、R70 9/0/0（15:43:13–14）、R63 3/0/0（15:43:14–15），均EXIT0；本次历史回归合计20，未冒称重跑含R72的29项。
- `--features mir-render/legacy --lib ui::spellfx`：UTC15:43:15–21，4/0/0（328 filtered），EXIT0；本次测试总计33通过/0失败/0忽略。
- `cargo clippy -p mir-client -p mir-render --all-targets -- -D warnings`：UTC15:43:21–44 EXIT0；`cargo fmt --all -- --check`：15:43:44–47 EXIT0。
- `py -3 tools/docs/check_links.py`：UTC15:43:47–48 EXIT0；`git diff --check`：15:43:48 EXIT0；完整命令、精确UTC、日志原字节摘要见 `target/round73/t/attempt-1/runs.json`。

- 门禁只读复算：attempt-1 49,146B / `d1cbb67e22f2d4e2f6d15266f72e9fbf3d2099e2010397a2deca4583e8f328f3`；attempt-2 182,653B / `fec31da6a4e2500af17fef54c6ed4fd0e025de593577f0ec11a966ebcfb966b2`；attempt-3 213,076B / `676b9d4f36a9a5ef59c47365eac890d42463d195c80532a6d12944ef49beddac`，均匹配清单。
- 原日志末段独立计数分别11OK/1FAIL、11OK/1FAIL、**12OK/0FAIL**；首两次唯一测试失败分别为真实素材client_fx与PostgreSQL池超时，原日志保留。attempt-3 HEAD3378303、EXIT0、UTC15:28:00–15:30:18Z及13个对应字段均匹配清单/meta。
- 口径纠偏：attempt-3 meta实际14字段（含meta_note），非交接所称13；attempt-1 meta仅5字段，无bytes/hash，故不冒称与不存在字段比对。UTC/operator/head由meta提供，原日志未独立内嵌这些时间/身份事实。
- 清单复算：**33/33 path/bytes/sha256匹配**；tracked项Git OID全部匹配delivery_head；raw Git blob摘要除T-D1外匹配。无自引用；外置SHA=`dba0ab26e23a5383d7ed5b5ab68c5829b59583d51249b4276b6bdece0925371a`匹配；实际343行/15,754B（交接304行属旧快照）。
- delivery_head=55f61c4指文档提交、清单在34d5e17另行固化的note诚实；D自检304行/旧SHA属于固化前历史，不将其当作最终343行清单的独立核验。

- A真红原字节抽核：物理副本测试与最终 `1ec3f6dbbf7368e7dbd82c78529f1e6605908b04c11164ba860ab28b42513c5e` 全等；impl与93e1140 blob全等；两文件nlink=1且非symlink；日志0/9/0 EXIT101，首败G1实际`:1239`（非1226），(381,261) `[0,0,0,255]`→`[248,0,0,255]`。
- B抽核采用最终实现的attempt-2 M1/M4：patch分别只改Swing→Bolt、offset true→false；build0→0/9/0 EXIT101→恢复9/0/0 EXIT0；首败(384,256)分别`[48,136,176,255]`、`[248,0,0,255]`对背景；恢复impl/test SHA均等最终。attempt-1旧实现patch不冒充最终证据。
- C oracle脚本已逐读；仅调整输出到T后独立重执行：8/8 EXIT0，actual/A expected/Python逐字节相等，8条摘要与C历史结果完全一致；覆盖age0/80/399/400、ROI、offset裁剪、双事件Add、cast=false旧路径。未声称T重跑B全部63条。
- Crystal逐读 `PlayerObject.cs:2956–2963` 与 `Effect.cs:38–61/81–96/113–130`：Magic2/10/5/400→10..14、80ms名义间隔；`Time<=NextFrame`返回，严格大于才推进、每Process至多一步。实现elapsed与恰400ms截止为已声明PARTIAL，不能视为严格Crystal等价。
- 文档范围检查：README/契约/client§5.38/roadmap§二十一及清单均无整轮通过、全部雷电已证、严格等价或真实设备视觉已签核扩称；`SpellObject.cs:57–64`毒云误锚已明确纠偏；源码:309反向条件注释按Lead裁定MINOR交R74，披露充分。
- 素材核对：旧6份PNG逐字节相同，旧13项index元数据逐项相同；仅增10..14；game-assets被.gitignore:27排除且未入库。两次失败、扩帧本地性、M3/M5以C精确证据为权威及BLOCK历史均已披露。
- PARTIAL原号承接：GAP-5/R70-C1、R70-C3、R71时钟/对象、R73-T1、GAP-1/2/R70-C2、GAP-3/4/6、R72 B plan-M4/legacy lint、R71-C7/E1/R69条件5、R68/R49–R70/R61–R67；owner/ActionFeed/Light6/音频、严格时钟、rate0.8、其余26技能及设备视觉均未清零。

- 范围/卫生：逐提交stat与提交链核对；93e1140..HEAD仅13个白名单文件，最大blob186,824B；3378303后crates零差异；无wire/server/旧测试改动；新增allow/ignore=0，spellfx既有allow3→3。
- 两freeze全程未跟踪且SHA未变：before `70845aefd4dd689a55ab0618b36bbf692a4b33ebb5ee2f3a662dedc17a3dc9de`（44,844,165B）；after `c7d896e7f5eee4706b943912c0c564d10d61c98d163143f051c2d632daecde32`（45,219,533B）。审计前及报告生成前工作树仅这两项；报告为本轮唯一新增非忽略文件。
- 证据：`target/round73/t/attempt-1/{runs.json,gate-check.json,manifest-recompute.json,red-check.json,static-audit.json,logs/}`；仅写本报告及该T目录，未改交付物/清单/实现/测试，未commit/push，未运行ci.ps1。
