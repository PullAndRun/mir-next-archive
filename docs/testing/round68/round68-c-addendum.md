# R68-C 补充核验（新 SHA）
verdict：**BLOCK**；原必改1 PARTIAL、2/3 CLOSED；剩余必改1项退 B（字节比较），实现缺陷0；不宣称 R68 整轮通过。
基线 HEAD `2c69c70fff34913e59fabb779b5bd602a6c32ff5`；本轮独立执行 UTC 2026-10-27 02:06:07–02:09:13；只写本报告及指定 addendum-1 目录；未 commit/push/跑 ci.ps1。

| 原必改 | 状态 | 源码核验与剩余差距 |
|---|---|---|
| 1 飞行非空 tick 重复 draw / center | **PARTIAL** | [live:495](../../../crates/mir-client/tests/round68_effect_offset_live.rs#L495) 索引飞行 view，frame=1/451、运动(30,0)；:538–541重复draw、:547–555换center整帧+屏移-240均已补。但 [helper:330–340](../../../crates/mir-client/tests/round68_effect_offset_live.rs#L330) 只比尺寸及 FNV-1a 摘要，未比较 pixels 字节，不能按要求称“逐字节”。 |
| 2 新入口边界直测 | **CLOSED** | [wire:406–449](../../../crates/mir-client/tests/round68_image_offset_wire.rs#L406)：:409直调575零offset；:422直调450(-3,5)+(2,-6)=(-1,-1)，:434整帧仅右下(0,0)；:443/447全屏外true+零像素；:481缺帧false、:484零像素。 |
| 3 目标断言布尔字段 | **CLOSED** | [B旧JSON](../../../target/round68/b/attempt-2/red-failures.json) 7/7 `target_offset_assertion:true`；与 [C历史原日志摘录:121–185](../../../target/round68/c/attempt-1/static-audit.json#L121) 逐条test/首败行/消息相符；新SHA [B矩阵](../../../target/round68/b/attempt-3/manifest.json#L89) 的G2首败已正确迁至434。 |

唯一必改（B）：将飞行重复draw检查补为直接 `canvas.pixels == twice.pixels`（可修 helper 为精确字节比较并只在失败时打印摘要）；摘要相等不能充当字节相等。修改后新SHA重新冻结，受影响红绿与C复跑重做。

独立复跑全新 CARGO_TARGET_DIR=`target/round68/c/addendum-1/cargo-target`；TEMP=TMP=TMPDIR=`target/round68/c/addendum-1/tmp`（实际均绝对路径）。
命令、绝对cwd/env、完整UTC/EXIT/计数见 [runs.json](../../../target/round68/c/addendum-1/runs.json)，原始 [green.log](../../../target/round68/c/addendum-1/logs/green.log)。

| 独立命令 | UTC 起止（2026-10-27） | EXIT / 计数 |
|---|---|---|
| `cargo test -p mir-client --features mir-render/legacy --test round68_image_offset_wire --test round68_effect_offset_live --test round62_projectile_wire --test round63_projectile_wire` | 02:06:07.633–02:06:59.472Z | 0；wire2、live2、R62=5、R63=3，共12 passed/0 failed/0 ignored |
| `cargo clippy -p mir-client -p mir-render --all-targets -- -D warnings` | 02:06:59.473–02:07:32.712Z | 0 |
| `cargo fmt --all -- --check` | 02:07:32.714–02:07:35.229Z | 0 |

独立变异：`git archive 2c69c70` 解包到指定 repo-R2，再物理覆盖3实现+2新SHA测试；5文件nlink=1，见 [mutation-inputs.json](../../../target/round68/c/addendum-1/mutation-inputs.json)。只将飞行 `scene.draw_effect_with_offset` 退为 `scene.draw_effect`，命中保持新入口。
独立target/tmp均位于 addendum-1/isolated；先 `cargo test --no-run -p mir-client --features mir-render/legacy --test round68_effect_offset_live`，02:06:14.719–02:07:06.595Z，明确Compiling mir-render/mir-client，EXIT0。
红命令：`cargo test -p mir-client --features mir-render/legacy --test round68_effect_offset_live r68_g3_real_125_flight_frames_apply_per_frame_offsets -- --exact --nocapture`。
红02:07:06.596–02:07:08.493Z，EXIT101；0 passed/1 failed/0 ignored/1 filtered；[live:283:9](../../../crates/mir-client/tests/round68_effect_offset_live.rs#L283) @(418,254)，left `Some([0,0,0,0])`、right `Some([0,0,248,255])`，目标offset像素断言；[原始红](../../../target/round68/c/addendum-1/logs/red.log)。
恢复实现后隔离 `cargo clean -p mir-client` EXIT0，02:07:09.207–02:07:20.987Z 同命令明确Compiling mir-client，1 passed/0 failed/0 ignored/1 filtered，EXIT0；[红绿清单](../../../target/round68/c/addendum-1/independent-red.json)。
红绿测试SHA均 wire `c330a4b628749184461b595fe0681fbf161f042b578586ebd02483e390908c73`（569行/23355B）、live `19cf4cce0e6563f99bb3e54e7338556eb83d12208bfa72a064463ea266eae50c`（746行/30442B）；3实现SHA与交接一致，见 [输入清单](../../../target/round68/c/addendum-1/inputs.json)。

范围：[git status](../../../target/round68/c/addendum-1/logs/status.log)、[完整crates diff](../../../target/round68/c/addendum-1/logs/crates-diff.log) 显示仅3 src改动+2未跟踪新tests；[protocol diff](../../../target/round68/c/addendum-1/logs/protocol-diff.log)/[server diff](../../../target/round68/c/addendum-1/logs/server-diff.log) 均空。
新增ignore/allow=0；既有client ignore=7且baseline不变；两player-resource freeze SHA仍70845aef…dc9de/c7d896e7…de32；[范围证据](../../../target/round68/c/addendum-1/scope.json)。
本轮A/B attempt-1/2/3共81个既存文件前后SHA完全相同，见 [manifest](../../../target/round68/c/addendum-1/manifest.json)；本C未覆盖历史证据。
诚实性：A新壳红0/4、恢复绿4/4+回归8，与原始日志一致；B新SHA矩阵R1 wire0/2 live0/2、R2 wire2/0 live1/1、R3 wire2/0 live0/2，与原日志一致；独立12项复跑匹配A/B。
历史归档PARTIAL交D/T：B共享isolated原日志已被Phase-3替换，旧首败应引用上述C历史摘录+旧日志SHA，不能声称共享原日志未覆盖；旧7条布尔补录与其历史首败一致，故必改3关闭。
措辞PARTIAL交D/T：[前C报告:59](round68-c-evidence.md#L59) 已指出A旧manifest:394“真实资源字节”不精确；[A新manifest:222](../../../target/round68/a/attempt-2/manifest.json#L222) 已写合成WZL限制，但尚未见正式勘误条目；本报告明确两测试均合成夹具，不作真实素材视觉签核。
Crystal独立打开 [MLibrary.cs:880–890](../../../github/Crystal/Client/MirGraphics/MLibrary.cs#L880)，X/Y均ReadInt16，保留有符号帧头偏移语义；未将offset当宽高或运动量。
PARTIAL承接：真实素材/设备视觉签核、全UI offset、spellfx居中迁移、DrawBlend/shadow/mask及历史R49–R67遗留保持原边界；本次仅核125/570切片。
