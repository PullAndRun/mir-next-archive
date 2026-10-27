# 第68轮尚书省派发（JJC-20261027-005）
状态：派发完成；主单 Doing；四子单已登记，执行者尚未启动。
基线 HEAD：2c69c70fff34913e59fabb779b5bd602a6c32ff5；依据 round68-plan.md（83行）及门下 PASS-WITH-CHANGES 全文，C1–C8/M1–M5全部纳入。
| 单项 | 真实 ID | 承接角色 | 依赖与交付 |
| --- | --- | --- | --- |
| 主 | JJC-20261027-005 | 尚书协调/Lead执行 | Doing；未验收 |
| A | JJC-20261027-005-sub-9407a193 | 兵部 bingbu | 显式入口/转发/125与570接线/注释/自测/红壳准备 |
| B | JJC-20261027-005-sub-dea98aab | 工部 gongbu | A交付后完成G1–G4、R1–R3；工程链内先落测试 |
| C | JJC-20261027-005-sub-226d511b | 刑部 xingbu（gpt） | depends_on=B；只读独立核验，必改0/PASS-WITH-PARTIAL |
| D | JJC-20261027-005-sub-61532b37 | 礼部 libu（gpt） | depends_on=Lead门禁成功；文档与C证据归档 |
顺序：A→B→C→Lead功能提交+唯一门禁→D；串行交接，禁止同文件并写；尚书本次仅登记不启动deepseek/gpt执行者。
M1责任A+B、C核：B测试先落盘→A最小可编译旧语义转发壳（不加offset）→命名assert红→A实现→B绿；API不存在编译错仅准备态，不替代R1–R3。
M2责任B、A实现、C核：缺帧false；有效图像完全屏外仍true；覆盖屏外锚点经offset移入可见区、零偏移、负坐标裁剪、i32极端无panic，整帧像素/返回值均断言。
M3责任B、C核：G4出生1000，依次1032/1063/1094/1125/1156/1187/1218/1249到达570；1309相等不进、1310进571；禁止大跳或elapsed helper伪到达。
M4责任A+B、C核：Magic start_frame=0、右向dir16=4、450→451、1032运动(30,0)独立预期；禁止被测draw_location/offset作oracle。
M5责任A、C核：修crates/mir-render/src/scene.rs:412为bool存在性；crates/mir-client/src/ui/projectile.rs:911及新入口注明运动偏移/ImageInfo偏移各一次；C逐项列C1–C8/M1–M5证据和实际计数。
A源白名单仅crates/mir-render/src/entities.rs、crates/mir-render/src/scene.rs、crates/mir-client/src/ui/projectile.rs；证据target/round68/a/**。
A实现：EntityRenderer/MapScene新增draw_effect_with_offset；按kind.library()/index+start_frame取当帧cached_info，signed offset各saturating_add后blit_cached；旧draw_effect绝对左上角不变，125飞行/570命中改新入口。
B源白名单仅crates/mir-render/tests/round68_effect_offset.rs、crates/mir-client/tests/round68_projectile_offset.rs；证据target/round68/b/**；变异只在独立物理副本，禁改共享生产树。
C唯一写白名单target/round68/c/**；生产/测试/docs只读；docs/testing/round68/round68-c-evidence.md前身暂存该目录，门禁后由D据原证据归档；修复退A/B。
D白名单docs/testing/round68/**、docs/client.md、docs/roadmap.md；证据target/round68/d/**；README/offset-contract/final-manifest/manifest.sha256，清单不含自身、外置自身摘要，禁改旧证据/门禁meta。
C1/G1：真实LibrarySource合成WZL/WZX，450/451/570/571 signed(-3,5)/(4,-2)/(-2,-3)/(3,4)，逐帧不同色整帧像素，原点/双偏移无误绘。
C2/G3–G4：71/125 encode/decode→apply_packet_at→UiState.sync→projectiles().draw，先断言views/hits与精确sprite_index；独立投影+运动一次+帧偏移，换center/重复draw验证平移与不累积。
C3–C4/G2：旧入口非零offset仍原点，meta/调用坐标不变；新Frame/交替入口不累积；完整覆盖M2。
验收绿命令：cargo test -p mir-render --test round68_effect_offset；cargo test -p mir-client --test round68_projectile_offset；每组2 passed/0 failed/0 ignored。
回归：cargo test -p mir-client --test round59_projectile_render；cargo test -p mir-client --test round62_projectile_wire；改动包fmt/clippy；执行者不自跑全量ci。
C5–C6/三红：R1新入口offset置0→G1；R2仅飞行退旧入口→G3；R3仅命中退旧入口→G4；各独立物理源码副本、无硬链接、独立CARGO_TARGET_DIR/TMP/TEMP/TMPDIR。
红回执：绝对manifest/cwd、变异源SHA/patch、同一测试红绿SHA、编译来源、UTC起止/EXIT/命令/stdout/stderr/计数/首败与恢复证明；编译成功、执行至目标assert、left/right+行号、EXIT101才有效。
禁止以0tests/编译错/ignore/skip/无关失败/改测试造红或exit0充红；C独立重跑G1–G4/两回归并另做R2/R3之一；受影响源/测试SHA改变需重跑对应红绿。
唯一门禁仅Lead：C通过后核无并发cargo/rustc/服务端，先提交A/B功能，再在功能提交完整HEAD运行pwsh tools/dev/ci.ps1 -KillStale；EXIT0且12/12。
门禁日志用 *> 完整捕获至target/round68/gate/attempt-N/，meta含EXIT/完整HEAD/UTC起止/cwd/命令/bytes/sha256/operator=lead；失败保全，新attempt不覆盖；定向测试不代门禁。
门禁后crates diff=0；任何crates变化使门禁失效，须Lead新功能提交/新attempt；D仅在成功门禁后落文，push由Lead依上级授权。
C7/PARTIAL：仅关闭R61 P04“125飞行/570命中图像偏移消费”子项；全UI/通用spellfx/真实素材视觉/设备Live/DrawBlend/shadow/mask继续PARTIAL，合成素材不得称真实视觉签核。
历史承接：R49–R60/R60 P-1–P-6、R61其余P01–P12、R62①–④、R63 P1–P7、R64①–⑥、R65 G1.2/G2、R66生命周期/全局水位、R67容量/保护/map epoch/同代ID复用；125无点坐标语义不变。
C8禁止项：wire/服务端/旧入口语义/Frame全局语义/生产mir-asset/计划修改；spellfx.rs:257–265居中不得回归；两份历史freeze JSON禁止修改，已保存基线SHA并核前后。
风险：漏接一路由R2/R3检出，signed/缓存误帧由G1/G4检出，center/运动重复由G3检出，隔离与同SHA防伪红；真实视觉保持PARTIAL。
回滚：未提交仅逆向本轮明确patch，已提交由Lead revert功能commit；保留证据，禁reset/clean损伤并发工作/freeze。
回执目录：target/round68/shangshu/attempt-1/；contracts.json含完整逐题指令/白名单/验收/禁项/PARTIAL；delegate-A/B/C/D、state、validate、show及list均有原始stdout/stderr和EXIT JSON。
CLI实测：help=0；show-before=0；delegate A/B/C/D均0；state Doing=0；validate=0（ok=true/issues=[]/216 tasks）；show-after=0；list-after=0；四子单归属据list/show核对。
留痕：首次骨架write参数校验失败（invalid justification，未写入），修正后成功；无CLI失败/重试、无代笔或身份绕过；C文档延后归档按本次白名单收紧，非核验豁免。
