# R73 尚书派发（已登记）

主单 `JJC-20261027-010`；基线 `bd71bd049f516c3d7984a77c241d2630cad291d3`；门下 PASS-WITH-CHANGES，C1–C8/M1–M5全部生效。

|段|真实任务ID|部门|depends_on / 阶段前提|agent|
|---|---|---|---|---|
|主|JJC-20261027-010|尚书|Doing|本派发官|
|A|JJC-20261027-010-sub-41587dae|兵部|门下批准/更正契约|未启动|
|B|JJC-20261027-010-sub-86bd7fae|工部|JJC-20261027-010-sub-41587dae冻结交付|未启动|
|C|JJC-20261027-010-sub-cb519bdf|刑部|JJC-20261027-010-sub-86bd7fae完成|未启动|
|D|JJC-20261027-010-sub-19c6a581|礼部|JJC-20261027-010-sub-cb519bdf必改0→Lead功能提交+唯一门禁成功|未启动|

DAG：A→B→C→Lead提交+唯一门禁→D→尚书验收→指定门下确认；Doing仅表示登记，依赖仍须满足。
CLI无结构化depends_on/agent参数：前置真实ID写入return_spec，Lead启动时另存真实agent映射；本次禁启动执行者。
M1：派发基线修订为上述HEAD；原计划审议快照保留，修订契约另冻SHA；120/123链，非125/71；协议INDEX在server.rs:402/496。
M2：A注释与实现，B/C来源核验，D台账；Duration=400事实，80ms elapsed/400截止为产品精灵窗；700ms文字/色块窗保留；无owner=TargetPoint；rate1 Add有限近似。
M3：A写九测试、B/C独立复验；60/79/80和399/400/401；M3先证Magic2[15]存在；120/123×cast、149×param×ThunderBolt/Lightning矩阵逐行金样。
M4：B/C各独立物理副本与Python实现，逐M原件/日志/恢复SHA/计数；同最终测试与夹具SHA，Python实现各自单独冻结。
M5：A/B/C保留历史测试实际计数/SHA；R71/R72 param=false旧金样不改；C逐条原patch核对，不满足即BLOCK；D原号台账保留。
A代码白名单仅crates/mir-client/src/ui/spellfx.rs及新crates/mir-client/tests/round73_thunderbolt_hit_mapping.rs（本指令名称覆盖计划旧名）。
实现：仅ThunderBolt && !from_object_spell && param；Magic2 10..14；age/80非循环；age>=400 continue；格锚点+signed offset一次；Swing/start0、rate1 Add；Lightning旧行为不改（Crystal:3225）。
A/B/C物理副本禁hardlink/junction/symlink，nlink=1；独立绝对CARGO_TARGET_DIR与TEMP/TMP/TMPDIR，冻结原字节及SHA。
九测试：r73_g1_wire_mapping、r73_g1_ui_draw、r73_g2_frame_boundaries、r73_g2_age_window、r73_g3_anchor_offsets、r73_g3_offscreen_crop、r73_g4_add_per_cell、r73_g5_source_cast_legacy、r73_g5_missing_frame。
五变异：M1仅Swing→Bolt仍10+phase；M2仅80→60；M3仅去age>=400 continue且15诱饵可读；M4仅offset bool true→false；M5仅准入留技能条件。
红绿：A最终测试置bd71bd0物理副本，成功编译且映射/像素真红再实现；B/C逐M前绿9/0/0→编译成功/指定断言红EXIT101→生产原字节/SHA恢复→后绿9/0/0。
0tests/ignored/SKIP/编译或环境错误均无效attempt；测试改动重冻并重做受影响矩阵；B/C不得改交付测试或共用缓存。
B/C各自Python直接解实际WZX/WZL，RGB565/手算坐标裁剪/Add；actual/Rust expected/Python整帧RGBA及SHA三源一致，禁生产helper当oracle。
证据根target/round73/{a,b,c,d}/attempt-1/**；失败只增attempt；每命令UTC起止/cwd/完整argv/HEAD/环境/EXIT/计数/stdout/stderr/bytes/SHA，含首败expected/actual与变异diff。
A验收：cargo test -p mir-client --features mir-render/legacy --test round73_thunderbolt_hit_mapping -- --test-threads=1；B/C在各自副本重做九项与五变异。
回归：cargo test -p mir-client --features mir-render/legacy --test round72_poison_cloud_mapping --test round71_firewall_mapping --test round70_effect_blend_offset --test round63_projectile_wire；实际29项，0failed/0ignored。
补验：cargo test -p mir-client --lib ui::spellfx；cargo clippy -p mir-client --all-targets -- -D warnings；cargo fmt --all -- --check（均为未来执行契约）。
Lead门禁仅C必改0且功能提交HEAD后：pwsh -NoProfile -File tools/dev/ci.ps1 -KillStale；不SkipTests/SkipPostgres，EXIT0且实际12项全OK；crates/tools再变须新闸。
C仅自有证据与docs/testing/round73/round73-c-evidence.md及必要新round73-c-addendum.md；D仅README/round73-thunderbolt-contract/final-manifest.json/manifest.sha256及roadmap/client的R73小节与自有证据。
偏差/PARTIAL逐号承接原计划139–151行及审议32–33行；新增R73-T1起手/ActionFeed/owner跟随/严格时钟/回执起算/Light6/音频/400ms产品截止，不称全UI/全技能/GPU等价。
禁项：执行者禁commit/push/ci.ps1；任何人不得改freeze或扩白名单；不改协议/net/magic/队列/其他技能/历史测试报告；本派发官不写crates、不跑cargo/CI、不启动执行者。
本次派发证据target/round73/shangshu/attempt-1/**：草稿、修订契约、CLI原输出、show/validate JSON、偏差、文件元数据。
