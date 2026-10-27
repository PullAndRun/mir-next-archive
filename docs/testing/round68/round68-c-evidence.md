# R68-C 刑部独立核验
verdict：**BLOCK**；必改 3 项（均退 B），实现缺陷 0；未交 Lead 功能提交/唯一门禁。
子单 JJC-20261027-005-sub-226d511b；基线 HEAD `2c69c70fff34913e59fabb779b5bd602a6c32ff5`；本次 UTC 2026-10-27T01:41–01:48。
只写本文件及 target/round68/c/**；生产/测试只读，未 commit/push、未跑 ci.ps1。

| 必做项 | 结论 | 独立证据/实值 |
|---|---|---|
| 1 复跑 | PASS | R68 2+2、R62 5、R63 3，合计12 passed/0 failed/0 ignored；clippy/fmt EXIT0，与 A 最终自报一致 |
| 2 独立变异 | PASS | C 自建 archive 物理副本 R2，编译成功后目标像素断言 EXIT101；强制重编译绿1/0/0 EXIT0；测试SHA相同 |
| 3 实现语义 | PASS | entities.rs:1855–1869 当帧 cached_info、signed i16→i32、saturating_add 后 blit_cached；旧入口不变；缺帧false/全屏外true |
| 4 测试正确性 | FAIL | 现有断言真实，G4全10帧有效；但飞行状态 center/重复draw及新入口零偏移/部分裁剪缺直接覆盖，见必改1–2 |
| 5 范围/边界 | PASS | crates仅3 src+2未跟踪tests；protocol/server diff空；无新增ignore/allow；客户端既有7个ignore未动；两freeze SHA与历史冻结相符 |
| 6 证据/诚实性 | FAIL | A/B计数与原始日志相符，R3交叉红解释成立；B red-failures.json 7条均缺 target_offset_assertion=true，见必改3 |
| 7 Crystal | PASS | 独立打开 Client/MirGraphics/MLibrary.cs:658–670、880–890，Client/MirObjects/Effect.cs:112–132、201–216，Client/MirScenes/GameScene.cs:10328 |

独立复跑环境：CARGO_TARGET_DIR=仓库绝对路径/target/round68/c/cargo-target；TEMP=TMP=TMPDIR=同根/tmp，全新目录。
所有命令 cwd=仓库根；完整绝对路径、UTC、EXIT、日志及计数见 target/round68/c/attempt-1/runs.json。

| cargo 命令（逐条实际执行） | UTC 起止（2026-10-27） | EXIT/结果 |
|---|---|---|
| test -p mir-client --features mir-render/legacy --test round68_image_offset_wire --test round68_effect_offset_live | 01:42:09.359–01:42:48.280Z | 0；各2/0/0 |
| test -p mir-client --features mir-render/legacy --test round62_projectile_wire | 01:42:48.282–01:42:50.070Z | 0；5/0/0 |
| test -p mir-client --features mir-render/legacy --test round63_projectile_wire | 01:42:50.070–01:42:51.877Z | 0；3/0/0 |
| clippy -p mir-render -p mir-client --all-targets -- -D warnings | 01:42:51.878–01:43:35.257Z | 0 |
| fmt --all -- --check | 01:43:35.257–01:43:37.989Z | 0 |

独立 R2：`git archive 2c69c70`→tar解包至 target/round68/c/isolated/repo-R2，无硬链接；覆盖3实现+2最终测试，仅飞行调用退 draw_effect。
绝对 manifest/cwd、独立 target/tmp、实现变异前后SHA、补丁及命令见 target/round68/c/attempt-1/independent-red.json 和 logs/R2.patch。
红命令：`cargo test -p mir-client --features mir-render/legacy --test round68_effect_offset_live r68_g3_real_125_flight_frames_apply_per_frame_offsets -- --exact --nocapture`。
红 UTC 01:43:04.086–01:43:46.687Z，编译成功，0 passed/1 failed/0 ignored/1 filtered，EXIT101；live:283:9 @(418,254)，left Some([0,0,0,0])，right Some([0,0,248,255])。
恢复实现 SHA 3528a4ee0252a1bc0af5235dace5e92f437b79075300828fabcdc1ed409acdea；首次绿运行未重编译仍101，原日志保留。
随后仅隔离 target 执行 `cargo clean -p mir-client` EXIT0，01:44:32.942–01:44:43.660Z 同命令明确 Compiling mir-client，1/0/0/1 filtered，EXIT0。
红绿 wire SHA e9f3a20a3a2bccf0b234dc0af31ef61b4e02bfcc0ba27849fae5866721a120c2；live SHA f011395db6669a9ffd026f4c3af59971590d802748bd64a744c982b9991d4658，与工作区绿一致。

| 门下契约 | 结论/定位 |
|---|---|
| C1 | PASS：wire:252–266读回四组signed；G1整帧450/451/570/571像素，正负偏移有效 |
| C2 | FAIL：71/125真实wire→snapshot→sync→公开draw有效；G3飞行451与回绕450有像素，但换center/重复draw只测命中（live:538–570） |
| C3 | PASS：wire:380–390旧入口非零偏移仍原点；:486–520交替新旧入口不累积、meta尺寸不变 |
| C4/M2 | FAIL：屏外移入/缺帧/极值有效，但零偏移和部分负裁剪仅走旧入口（wire:393–416） |
| C5 | PASS：B三物理副本SHA与日志核对；C另建独立R2，并有同SHA绿 |
| C6 | PARTIAL：红均目标断言、非setup/编译错/0 tests；B缺布尔字段，不能写“全true” |
| C7 | PASS-WITH-PARTIAL：无真实视觉签核；R68 README尚不存在，不能据此称最终文档通过 |
| C8 | PASS：生产仅白名单；全部5文件SHA与交接相符，freeze未变；三份process文档存在且为R68 |
| M1 | PASS：A保留早期壳红历史、最终同SHA隔离baseline红；B R1是支持层变异，二者区分明确 |
| M3 | PASS：固定1032…1249到达；1309仍570、1310进571；G4 :671/:690逐帧覆盖570–579，577(-5,2)/578(4,-4)/579(2,2) |
| M4 | PASS：Magic start_frame=0；右dir16=4；1032运动(30,0)；570+frame与Crystal BaseIndex+CurrentFrame一致；center改15的X差=-240 |
| M5 | PASS：scene:412–442明确bool存在性及新入口；projectile:911–932运动/图像偏移各一次 |

必改1（B）：在真实飞行非空、1032/451状态补换center与重复draw整帧像素断言；现有live:549中心平移发生在1249命中，不能代替飞行覆盖。
必改2（B）：新 draw_effect_with_offset 直接补零offset像素、偏移后部分负坐标裁剪像素与返回值断言；旧入口用例不能证明新入口这些边界。
必改3（B）：补7条 target_offset_assertion=true 的可追溯证据；原始日志已独立证实均目标offset断言，保留原日志与旧attempt。
B修测试后SHA改变，受影响红绿、三变异及C复跑须重做；C不代修。

A/B抽核：A最终绿2+2、回归5+3一致；B R1 wire=0/2、live=0/2，R2 wire=2/0、live=1/1，R3 wire=2/0、live=0/2，均编译成功。
R1首败wire:455（400B全0 vs 11非零字节）及:281（亮像素1620 vs 2017）；R2 live:283蓝像素；R3 live:283 @(622,253)透明 vs Some([0,252,0,255])；G3到达也绘570故R3双红合理。
范围补充：全crates固定字面ignore命中42行（含注释），基线/当前完全一致；mir-client实际ignore属性7个；两新测试无ignore/allow。
偏差留痕：首次tar把C:误认远程主机，改相对路径重试；首次git grep未转义[失败，改-F并保留原日志；恢复绿曾复用二进制，强制重编译解决。
诚实性补充：A manifest:394“live测试用真实资源字节”措辞不精确，两测试均合成WZL；本核验明确不作真实素材人工验收或生产视觉签核。
PARTIAL承接：全UI offset、spellfx居中迁移、真实素材/设备Live、DrawBlend/shadow/mask及R49–R67历史遗留全部保留；仅核125/570切片。
