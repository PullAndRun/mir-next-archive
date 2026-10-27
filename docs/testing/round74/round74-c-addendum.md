# R74-C M3 精确负控补证附录
目的：T 发现 C 原 M3 与[计划](../../process/round74-plan.md#L73)的“仅 offset_x add→sub、不得语义替换”契约不一致；本附录仅补此证据缺口。
基线 HEAD=`8df57ef8fa2df721e1d8ee31f4f1bee8b22d7091`；`git -c core.autocrlf=false archive 8df57ef` 解包为[物理副本](../../../target/round74/c/addendum-1/repo/)；1303 文件 nlink=1、symlink=0，见[隔离证明](../../../target/round74/c/addendum-1/physical-copy-proof.json)。
冻结测试：1375行/49849B，SHA256=`a2b329ce88a3640ad3609c804a2e589a4177a7de8c0e1e6df19c6d5c40013cc1`，见[测试证明](../../../target/round74/c/addendum-1/test-sha-proof.txt)。
路径勘误：实际 offset 位于[entities.rs:1916](../../../crates/mir-render/src/entities.rs#L1916)的 draw_effect_impl；任务文字提及的 spellfx 无该运算，按计划及 A/B 原 patch 的实际位置执行。
精确[补丁](../../../target/round74/c/addendum-1/patches/M3-precise.patch)仅将 `x.saturating_add(i32::from(info.offset_x))` 改为 `saturating_sub`，y 保持 add、其他字节不动；与 B 的 M3 patch 归一 LF 后逐字节相同（本件LF、B件CRLF）。
补丁 SHA256=`73831540855d867317baabe4bc8e57eb0214e71d466f6c53b9f36f1170bade36`；post SHA256=`4f72cae4deaeeb05d125274aff5c54571591bf4644d43bda1f7512a85926f631`，见[pre/post](../../../target/round74/c/addendum-1/patches/M3-precise-sha.json)。
命令基干：`cargo test -p mir-client --features mir-render/legacy --test round74_normal_blend_lock`；基线/全套加 `-- --test-threads=1`，build 加 `--no-run`，G2 加 `r74_g2_cross_library_signed_offsets -- --exact --test-threads=1`。
隔离环境：CARGO_TARGET_DIR=`target/round74/c/addendum-1/cargo-target`，TEMP/TMP/TMPDIR=`target/round74/c/addendum-1/tmp`，CARGO_PROFILE_DEV_DEBUG=0；完整绝对路径、UTC、EXIT、计数、日志 SHA 见[runs](../../../target/round74/c/addendum-1/runs.json)。
精确链：前绿5/0/0 EXIT0 → build EXIT0 → 全套RED 1/4/0 EXIT101 → G2指定RED 0/1/0（4 filtered）EXIT101 → 恢复绿5/0/0 EXIT0。
全套首败 `r74_g1_cross_library_start_indices`，测试:643，像素(8,2)，actual/left=`[253,5,5,255]`、expected/right=`[5,5,5,255]`，见[全套红](../../../target/round74/c/addendum-1/logs/M3-precise-red.log)。
指定首败 `r74_g2_cross_library_signed_offsets`，测试:797，像素(6,9)，actual/left=`[253,6,7,255]`、expected/right=`[5,6,7,255]`，见[G2红](../../../target/round74/c/addendum-1/logs/M3-precise-G2-red.log)。
原 C double 原件是 x/y 各连续 add 两次（非双轴 sub），post SHA256=`f1ec627817355a59cb926d3cfcbba975065400ae48f174212b062454f7114181`，见[原件副录](../../../target/round74/c/addendum-1/patches/original-C-M3.json)；因此差异同时包含 x 语义及 y 敏感性，不能称“仅 y 不同”。
原 double 重放 build0 → 全套1/4/0 RED101 → G2 0/1/0 RED101：测试:797像素(18,8)，actual=`[253,6,7,255]`、expected=`[5,6,7,255]`；最终恢复绿5/0/0 EXIT0，见[G2对照](../../../target/round74/c/addendum-1/logs/M3-original-double-G2-red.log)及[坐标推导](../../../target/round74/c/addendum-1/comparison.json)。
两轮均恢复原字节，pre/restored SHA256=`f8dff727ee970fbe2dc4c8d55f06c7a2587cf84df134c7c4e08a670bc8a7afc6`；冻结测试不变，见[恢复证明](../../../target/round74/c/addendum-1/restored-sha.txt)。
结论：C-M3 证据缺口已由本附录闭合（精确 add→sub 红绿成立）；未用编译错、0tests 或 ignored 充红，不替代 T 独立终审。
范围：仅新增本附录及[addendum-1证据](../../../target/round74/c/addendum-1/)；C 原报告、A/B、主树实现/测试及历史原件不修改；不 commit/push，不跑全量 CI；原有 PARTIAL 全承接。
交付 bytes/SHA、日志完整性、原件前后摘要及 crates/tools 零差异见[最终核验](../../../target/round74/c/addendum-1/final-verification.json)与[证据清单](../../../target/round74/c/addendum-1/manifest.json)。
