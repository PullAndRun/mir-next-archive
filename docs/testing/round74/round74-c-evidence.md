# R74-C 刑部独立核验证据

裁决：**PASS-WITH-PARTIAL**；子单 `JJC-20261028-001-sub-c55440fc`；必改清单 **0**。仅验收普通 NORMAL CPU/rate1 回归锁切片，C3 文档终审及 C4 Lead 全量门禁待其 owner 执行。
核验窗口：2026-10-27 17:36:59Z–17:47Z（UTC，15 分钟内）；HEAD=origin/main=`70958088a6147a0ed2192fe420ca5f59be4d084f`。
原始命令、cwd、UTC 起止、EXIT、计数、日志 SHA：[runs](../../../target/round74/c/attempt-1/runs.json)；静态命令/语义记录：[static-audit](../../../target/round74/c/attempt-1/static-audit.json)。

|核验项|结论|命令/UTC/EXIT 与事实|
|---|---|---|
|1 范围与卫生|PASS|17:41:20Z `git status --porcelain` EXIT0，仅两份既有 freeze 未跟踪；`git diff 7095808 -- crates` EXIT0/空；assets 的 `check-ignore` EXIT0、`ls-files` 空。交付后仅再新增本报告。|
|1 SHA/attributes|PASS|测试 1375 行/49849B，SHA `a2b329ce88a3640ad3609c804a2e589a4177a7de8c0e1e6df19c6d5c40013cc1` 与 A/B 相同；新增 allow/ignore=0；两 freeze bytes/SHA 前后相同。|
|2 C2 注释|PASS|17:41:20Z `git show --stat c1d0c76` 及 `git show c1d0c76 -- crates/mir-client/src/ui/spellfx.rs` 均 EXIT0，仅一行注释；零可执行差异。|
|2 G1–G5 语义|PASS|人工读码，与门下 C1–C4 对照；详见下文。没有扩称严格 Crystal 等价。|
|3 独立基线|PASS|17:40:30–53Z `cargo test -p mir-client --features mir-render/legacy --test round74_normal_blend_lock -- --test-threads=1` EXIT0，5/0/0，与 A/B 一致。|
|3 五项回归|PASS|17:40:53–17:41:07Z `cargo test -p mir-client --features mir-render/legacy --test round63_projectile_wire --test round70_effect_blend_offset --test round71_firewall_mapping --test round72_poison_cloud_mapping --test round73_thunderbolt_hit_mapping -- --test-threads=1` EXIT0，3+9+8+9+9=38/0/0，与 A/B 一致。|
|3 clippy/fmt|PASS|17:41:07–40Z `cargo clippy -p mir-client --all-targets -- -D warnings` EXIT0；17:41:40–44Z `cargo fmt --all -- --check` EXIT0，与 A/B 一致。|
|4 隔离负控|PASS|M1–M5 全部 build EXIT0→指定断言 EXIT101→原字节恢复→5/0/0 EXIT0；每轮以上一恢复绿作为前绿，M4 前绿为独立基线。详见矩阵。|
|5 冻结与oracle|PASS|A frozen UTC17:05:48Z < 首个 actual 命令17:05:49Z；Node 独立解析/编码 WZX/WZL，四夹具逐字节相同、4个 G1 整帧 SHA 与 A frozen/B actual 一致；G4独算[61,47,34]。|
|6 自报与诚实性|PASS-WITH-PARTIAL|A证据37项、B证据56项 bytes/SHA全匹配；基线/回归实数一致，A五变异首败/计数与原日志一致。既有 R74 计划/门下/派发、A/B manifest 与生产注释保留局限；D六文档未在本次终审范围。|

隔离：`git -c core.autocrlf=false archive 7095808`，在 [物理副本](../../../target/round74/c/attempt-1/red-copy/repo-m4/) 执行；1297 tracked blob SHA1 全对、nlink=1、symlink=0。所有变异仅此副本，最终全部恢复。
写范围优先于示例环境路径：`CARGO_TARGET_DIR=<repo>/target/round74/c/attempt-1/cargo-target`，`TEMP=TMP=TMPDIR=<repo>/target/round74/c/attempt-1/tmp`，未复用 A/B target/TEMP。
C 内初次启动曾在主树启动同 HEAD 编译后中断，不计有效绿；有效测试全部来自上述 archive。首次 tar 把 `C:` 视作远端而 EXIT128，改相对路径解包 EXIT0；原失败记录保留。逐 blob 探针首次中文 Git quote 解析失败，设 `core.quotepath=false` 后1297/1297匹配。

|变异|UTC build→RED→恢复绿|RED计数 过/败/忽略|首败实值（原测试行）|
|---|---|---|---|
|M4 Add→Over|17:41:44–17:41:56Z；0→101→0|1/4/0|G1:643，像素(2,3) actual=[248,0,0,255] expected=[253,5,5,255]。|
|M1 Bolt20→0|17:41:56–17:42:07Z；0→101→0|1/4/0|G1:617，left=Some((2,2)) right=Some((3,1))。|
|M2 Swing Magic2→Magic|17:45:41–17:45:52Z；0→101→0|3/2/0|G1:643，像素(7,3) actual=[253,5,5,255] expected=[5,255,5,255]。|
|M3 offset叠两次|17:45:52–17:46:03Z；0→101→0|1/4/0|G1:643，像素(20,1) actual=[253,5,5,255] expected=[5,5,5,255]。|
|M5 Add alpha强制255|17:46:03–17:46:14Z；0→101→0|4/1/0|G4:1162，alpha=1 像素(3,4) actual=[111,73,37,255] expected=[10,20,30,255]。|

以上每轮恢复绿均为5/0/0；无0tests、ignored、编译错或feature错冒充红。M4原始 [build](../../../target/round74/c/attempt-1/mutation-m4/build.log)、[RED](../../../target/round74/c/attempt-1/mutation-m4/red.log)、[恢复绿](../../../target/round74/c/attempt-1/mutation-m4/restored.log)及 [patch](../../../target/round74/c/attempt-1/mutation-m4/patch.json)保全；其余各目录同样保全。

语义审查：[spellfx](../../../crates/mir-client/src/ui/spellfx.rs#L309) 现为 `CMain.Time > NextFrame` 才推进、`<=`先return；Crystal [Effect](../../../github/Crystal/Client/MirObjects/Effect.cs#L81) 第81–96行确实先返回再 `++CurrentFrame`，每Process至多一次、整数Duration/Count、Owner.SkipFrames另加帧；注释继续披露产品窗近似、NextMotion/寿命PARTIAL，C2可单独闭合。
G1通过生产Magic/Swing/Bolt入口，异色/异尺寸/offset诱饵区别Magic0、Magic2 0/20/21；G2锁signed offset一次/禁offset、四侧裁剪、极端坐标及缺帧；G3每步整帧锁跨库Add/Over与缓存不污染；G4直接RGBA alpha128锁CPU取整并排除0.8缩放；G5缺/空帧不污染后继Magic/Bolt与旧sprite路径。
Expected 路径只从声明式RGB565帧与独立整数oracle计算，不调用生产 `effect_image`/blit取金样；生产actual才走这些入口。G5 `golden_magic`注释称“未经历缺帧”的scene实际是同一scene，此措辞可由A后续澄清；测试:1291另有独立整帧oracle，本轮不阻塞、不自行修改。
A frozen 四夹具经C独立zlib-store重建均逐字节吻合，也等于B复制原测试 `build_wzl` 的probe输出。独立G1四帧 SHA依次为：
`9303a9b6ae658d04b0e2caf67a0402b25e625126b319b0a31ecc3297d96842c0`、`7328863621d0e77f11a645dade3a4c65686bdeed8c4b4e2050e8c9062c21f8db`、`51b8b9d6ac49252560a26dddc3efb2511050137f0bf3cf8d61d262eb50d0e7a6`、`e41f67855b4875ea594b3ef3069cc3402598fc94757028a654ff1b8578e7783f`。
冻结时序结论依据保全文件/日志UTC，不具有外部可信时间戳证明；未将B全18帧结果冒称C全18帧复算，C独算4帧。A历史baseline为0ee34e1，B/C为7095808，测试同SHA，生产差异仅C2注释。
两份monster-native freeze分别44844165B/SHA `70845aefd4dd689a55ab0618b36bbf692a4b33ebb5ee2f3a662dedc17a3dc9de`，45219533B/SHA `c7d896e7f5eee4706b943912c0c564d10d61c98d163143f051c2d632daecde32`，本次始末未变。

PARTIAL承接：完整DrawBlend/BlendMode/DXManager、NORMAL rate/INVLIGHT设备状态、严格NextMotion/对象寿命、其他技能、真实素材/GPU/设备视觉/全UI/跨后端/Live音频；GAP-1/2、R70-C2及GAP-5/R70-C1双记、R70-C3、R71时钟/对象、R72 B plan-M4、R73-T1、GAP-3/4/6原号不变；R71-C7永久PARTIAL、E1补救CLOSED，R69条件5与R68/R49–R70/R61–R67照承。
范围声明：只新增本报告及 [attempt-1证据](../../../target/round74/c/attempt-1/)；未改主树实现/测试/其他文档，未commit/push，未运行全量CI；结论不替代Lead门禁或D/门下终审。交付SHA见 [delivery-sha256](../../../target/round74/c/attempt-1/delivery-sha256.json)。
