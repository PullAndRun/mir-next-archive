# 第68轮：125/570逐帧图像偏移 Implementation Plan

> For agentic workers: 执行须加载 subagent-driven-development 或 executing-plans；仅中书规划，独立门下批准后尚书登记子单。

**Goal:** 125飞行帧与570命中帧消费已保存的 ImageInfo.offset_x/offset_y，真实UI绘制按帧叠加一次。
**Architecture:** 保留绝对左上角 draw_effect；新增显式 draw_effect_with_offset，EntityRenderer先加图像偏移再交Frame裁剪，MapScene转发，ProjectileState两处接线。
**Tech Stack:** Rust、mir-asset、mir-render、mir-client；合成WZL/WZX与RGBA像素断言，无新增依赖。
**Spec:** docs/roadmap.md:253-261；docs/testing/round67/README.md:66-79；docs/testing/round66/README.md:107-136；docs/client.md:1263-1276。

## 全局约束与事实锚点
- 基线HEAD=2c69c70fff34913e59fabb779b5bd602a6c32ff5；R67已Done为上级当前事实，旧README的pre-audit是历史快照，不回写。
- Crystal根github/Crystal，实测HEAD=0e315fe327192afe52c3d7357ddd1f5b7e26c5b8；只读，禁止照搬源码。
- Crystal github/Crystal/Client/MirGraphics/MLibrary.cs:880-890：MImage以ReadInt16读X/Y；:658-670：offSet=true先point.Offset(mi.X,mi.Y)，再屏幕裁剪。
- Crystal github/Crystal/Client/MirObjects/Effect.cs:112-132：Effect按Owner.DrawLocation或地图投影，Draw/DrawBlend均传true；:201-216：Missile逐帧index及运动量之后同样传true。
- 提示中的Shared/.../LibraryFile.cs在本地固定版本未找到；采用已实读MirGraphics锚点，不伪造路径。
- Rust crates/mir-asset/src/image.rs:35-44已有i16 offset；crates/mir-asset/src/wzl.rs:161-186已读header[8..12]；crates/mir-asset/src/library.rs:410-448有合成格式范式。
- Rust crates/mir-render/src/entities.rs:1800-1812提供图像/尺寸；:1816-1830 draw_effect直接blit(x,y)；:1875-1887已有cached_info与blit_cached。
- Rust crates/mir-render/src/scene.rs:413-427仅转发；crates/mir-client/src/ui/projectile.rs:917-929两处缺图像偏移；draw_location已叠加一次运动量。
- Rust crates/mir-client/src/ui/spellfx.rs:257-265按尺寸居中且调用旧draw_effect；必须维持其坐标语义，禁止全局替换。
- 历史原编号：R61 P04=ImageInfo/渲染；P02=SkipFrames/ActionFeed；P03=TargetPoint/容量；P09=CWD（docs/testing/round66/README.md:112-119）。
- 禁改两份未跟踪历史freeze：docs/testing/monster-native/player-resource-freeze-before.json、docs/testing/monster-native/player-resource-freeze.json；规划/执行前后核SHA。

## 选题、非目标与Review Focus
- 选C的125/570切片：已有读入链与明确Crystal消费点，合成帧可给像素反例，单轮不依赖真实素材管线。
- A排除：125只有Spell/Source/Destination，未证明既有服务端回收协议；不借本地ack发明wire契约。
- B排除：105真实素材重导/设备播放范围大；D/E另轮，避免跨音频根路径和动作系统扩张。
- 非目标：全UI offset统一、通用spellfx居中迁移、真实素材视觉签核、DrawBlend混合对齐、shadow/mask、服务端/wire、事件/位置容量及多消费者。
- 五个审查重点（均由B覆盖）：signed负偏移；逐帧不同偏移；camera/运动只一次；负坐标裁剪与缺帧；旧draw_effect语义与非累积。
- 无真实素材前置条件：合成WZL/WZX不证明真实素材已对齐；game-assets缺失则视觉结论继续PARTIAL，不能SKIP本轮合成验收。

## 子任务DAG与白名单
`A实现 → B测试 → C刑部独立核验 → Lead功能提交+唯一门禁 → D文档`；严格串行，禁止同文件并写。
- A/B同一工程责任链：先写B指定失败测试再实现A；DAG表达交付依赖，不允许先实现后补造基线红。
- 本规划官不创建子单/启动执行者；以下契约供门下批准后尚书登记。

### A：显式偏移绘制与125/570接线
白名单：crates/mir-render/src/entities.rs、crates/mir-render/src/scene.rs、crates/mir-client/src/ui/projectile.rs；target/round68/a/**。
- [ ] 前两文件所属impl新增 `pub fn draw_effect_with_offset(&mut self, frame: &mut Frame, kind: FxKind, index: u32, x: i32, y: i32) -> bool`；scene沿用crate的FxKind路径。
- [ ] EntityRenderer按kind/library与start_frame加载当前帧，cached_info取signed偏移；x/y分别saturating_add(i32::from(offset))后blit_cached；缺帧false；不修改meta/调用者坐标。
- [ ] MapScene同签名转发；ProjectileState::draw飞行和命中两处调用新入口，保留draw_location/时间/声音/命中判定。
- [ ] 旧draw_effect语义保留；注释区分绝对左上角与图像锚点入口；不改Frame::blit全局行为，不写mir-asset生产代码。

### B：合成读入至真实UI像素测试
白名单：crates/mir-render/tests/round68_effect_offset.rs、crates/mir-client/tests/round68_projectile_offset.rs；target/round68/b/**。
- [ ] 进程唯一临时目录构造magic.wzl/wzx，稀疏索引覆盖450/451及570/571；2×2非黑不透明像素、各帧不同颜色；offset依次(-3,5)、(4,-2)、(-2,-3)、(3,4)。不读resource/game-assets。
- [ ] G1 `r68_effect_offset_signed_per_frame`：真实LibrarySource读回signed偏移；origin=(20,20)新入口画450→(17,25)、451→(24,18)；整帧像素精确比较，原点/双偏移处无误绘；新Frame重画结果相同。
- [ ] G2 `r68_effect_offset_clipping_missing_and_legacy`：负坐标部分裁剪、全屏外、零offset、缺帧false均断言像素/返回值；旧draw_effect带非零offset仍落原点，图像meta不变。
- [ ] G3 `r68_projectile_flight_offset_once`：71/125真实encode/decode+apply_packet_at→UiState.sync→公开projectiles().draw；source=(10,10)、target=(15,10)、时钟1000→1032、450→451；期望=world_to_screen+独立运动量(30,0)+该帧offset，比较整帧；换center等量平移，重复draw不累积。
- [ ] G4 `r68_projectile_hit_offset_once`：同UI流推进越过到达，目标存活；570→571严格NextFrame，期望=目标world_to_screen+该帧offset并比较整帧；缺帧不画占位。
- [ ] 先断言非空views/hits、具体sprite_index和帧推进；禁仅测辅助公式/非空像素。沿既有公开API，禁为测试开放私有内部。
- [ ] 绿命令：`cargo test -p mir-render --test round68_effect_offset`、`cargo test -p mir-client --test round68_projectile_offset`，各2 passed/0 failed/0 ignored；保存完整命令与日志。
- [ ] 回归：`cargo test -p mir-client --test round59_projectile_render`、`cargo test -p mir-client --test round62_projectile_wire`；按改动包fmt/clippy，禁自跑全量ci。

### 真红契约（A/B/C通用）
- [ ] R1隔离副本仅将新入口offset置0：G1失败；R2仅将飞行调用退回旧入口：G3失败；R3仅将命中调用退回旧入口：G4失败。
- [ ] 每变异独立物理复制、无硬链接；禁共享树应用变异；回执记录绝对manifest/cwd、独立CARGO_TARGET_DIR/TMP/TEMP/TMPDIR，证明编译源即变异源，禁复用旧二进制。
- [ ] 红与对应绿测试SHA256完全一致；编译成功、目标命名测试确实执行、断言首败含left/right与行号、退出101才有效。
- [ ] 0 tests、编译错、ignored/skip、改测试造红、既有无关失败、变异未命中exit0均不计；失败重试留原attempt不覆盖。
- [ ] 保存patch、实现前后SHA/测试SHA、UTC起止/exit、stdout/stderr、passed/failed/ignored、命令与首败；核工作树绿源码已恢复。

### C：刑部独立核验（depends_on=B）
白名单：docs/testing/round68/round68-c-evidence.md；target/round68/c/**；生产与测试源只读。
- [ ] 实读Crystal/Rust锚点，核旧入口、signed逐帧、完整UI像素断言；独立重跑G1–G4及上述两回归组，报告实际计数。
- [ ] 独立再做R2/R3之一，核同测试SHA与隔离来源；逐项复核B三红、freeze SHA、白名单。必改0且PASS-WITH-PARTIAL方可交Lead。
- [ ] 生产修复退A/B；C不得代改；源码/测试SHA变化使受影响红绿证据重跑。

### Lead：功能提交与唯一门禁（depends_on=C）
白名单：暂存A/B功能源与测试；target/round68/gate/**；依法更新看板。
- [ ] Lead核无并发cargo/rustc/服务端后提交功能（完整HEAD），仅Lead执行 `pwsh tools/dev/ci.ps1 -KillStale`；规划官不跑cargo/ci，不commit/push。
- [ ] attempt-N日志+meta记录UTC、cwd、命令、EXIT、完整HEAD、operator=lead、实数与SHA/bytes；要求EXIT0且现行12/12全通过，定向不代门禁。
- [ ] 门禁后crates diff=0；任何crates修改使门禁失效，须Lead新功能提交/新attempt；失败不覆盖。push由Lead按上级授权执行。

### D：礼部文档（depends_on=Lead门禁成功）
白名单：docs/testing/round68/README.md、docs/testing/round68/round68-offset-contract.md、docs/testing/round68/final-manifest.json、docs/testing/round68/manifest.sha256、docs/client.md、docs/roadmap.md；target/round68/d/**。
- [ ] 按实际C/门禁写矩阵、入口、限定完成与PARTIAL；清单不含自身，外置自身摘要；不改旧轮README/旧证据或门禁meta。
- [ ] 仅关闭R61 P04“125/570图像偏移消费”子项；全UI/其它Fx、真实素材/设备Live、blend仍PARTIAL，不清零整项。
- [ ] 承接R49–R60/R60 P-1–P-6、R61 P01–P12、R62①–④、R63 P1–P7、R64①–⑥、R65 G1.2/G2、R66生命周期/全局水位、R67容量/保护/map epoch/同代ID复用；125无点坐标语义不变。

## 回滚、风险与待裁决
- 回滚：未提交仅逆向本轮明确patch；已提交由Lead revert功能commit；保留所有证据；禁reset/clean损伤并发工作和freeze。
- 风险：漏接一路（R2/R3检出）；signed/缓存误帧（G1/G4）；center/运动重复叠加（G3）；共享target伪红（来源回执）；真实旧素材锚点不可靠（视觉PARTIAL）。
- 待门下裁决：认可C的125/570切片而非全UI扩面；建议认可，显式入口避免spellfx居中回归。候选P02/P03误编号已按历史台账纠正，无需新增用户授权。
- 若要求全UI/真实素材签核须退中书缩限或另轮；禁用无素材SKIP冒充验收。目前无已证明的执行外部阻塞。
