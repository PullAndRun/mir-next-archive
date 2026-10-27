# R75 149事件到精灵生命周期差异合同 Implementation Plan

> For agentic workers: REQUIRED SUB-SKILL: subagent-driven-development 或 executing-plans；门下独立批准后才实施。
> 基线 HEAD/origin/main：6502a90f83a977562c20a1b5b901a5672b0410d2；主单 JJC-20261028-002 / Menxia；本次仅规划，不启动执行者。

**Goal:** 候选C：用可执行的软件合同区分 Crystal SpellObject 对象寿命、Repeat/NextMotion 与 Rust 149瞬态精灵窗；生产零改，交付限定 PASS-WITH-PARTIAL。
**Architecture:** 一个新增集成测试文件包含测试私有 CrystalSpellClock 参考模型与真实149→26→149 wire/生产 draw_sprites 差分；双方预期分别固定，不把差异当等价。
**Tech Stack:** Rust、legacy合成WZL/WZX、CPU整帧RGBA、独立Python参考轨迹；无新依赖。
**Spec:** [R74合同](../testing/round74/round74-normal-blend-lock.md)、[R74台账](../testing/round74/README.md)、[历史台账](../roadmap.md)、[客户端合同](../client.md)。

## 全局约束、缺口与非目标

- 当前 spellfx 不含 SpellObject 状态机；但 net 已有 spell_fields 持久登记，不得写成“Rust完全没有对象状态”。
- 真缺口：26只移除持久登记，瞬态事件仍供精灵绘制；NextMotion/Repeat并未控制生产精灵寿命，4000ms后仍存在的场不再画该瞬态精灵。
- 题面2000ms须纠偏：当前 CAST_MS=700、AREA_MS=4000、AURA_MS=1200；2000只作为检测错误提前截止的内部采样点，不能写成当前寿命常数。
- 本轮闭环的是“差异可复查、可失败、原号承接”的软件合同；生产行为不修复，不把测试私有模型称为生产能力。
- 非目标：新技能映射、rate0.8公式/Mode、持久对象生产接线、26删事件、时钟替换、全UI、真实素材/GPU/设备/Live音频。
- 本规划禁止改crates、commit/push、ci.ps1、启动任何执行者；两历史freeze JSON保全原字节、不入库。
- 后续执行也不改历史R71–R74测试/证据，不顺手修R74 G5措辞；R73-T1反向注释已由c1d0c76修正，不再立项重复修。

## Crystal 实读锚点（本地源码；原文摘句）

|文件:行号|原文摘句与约束|
|---|---|
|[SpellObject.cs:30–38](../../github/Crystal/Client/MirObjects/SpellObject.cs#L30)|`MapControl.AddObject(this);`、`Repeat = true; Ended = false;`：149创建对象，未设2000/4000ms寿命。|
|[SpellObject.cs:49–64](../../github/Crystal/Client/MirObjects/SpellObject.cs#L49)|FireWall `DrawFrame = 1630; FrameInterval = 120; FrameCount = 6;`；PoisonCloud `Libraries.Magic2`/1650/120/20。|
|[SpellObject.cs:71–79](../../github/Crystal/Client/MirObjects/SpellObject.cs#L71)|Blizzard `FrameCount = 30;`、`Repeat = false;`；仅用于参考模型非Repeat控制组，不迁移映射/AnimationOffset。|
|[SpellObject.cs:288–302](../../github/Crystal/Client/MirObjects/SpellObject.cs#L288)|`NextMotion -= NextMotion % 100;`；`if (CMain.Time >= NextMotion)`；`if (++FrameIndex >= FrameCount && Repeat)`；`NextMotion = CMain.Time + FrameInterval;`。|
|[SpellObject.cs:296–300](../../github/Crystal/Client/MirObjects/SpellObject.cs#L296)|`FrameIndex = 0; Ended = true;`：Repeat回卷，Ended不等于删除。|
|[SpellObject.cs:328–339](../../github/Crystal/Client/MirObjects/SpellObject.cs#L328)|`if (FrameIndex >= FrameCount && !Repeat) return;`；`DrawBlend(... DrawColour, true, 0.8F)`：非Repeat隐藏与对象移除是两事。|
|[GameScene.cs:5412–5415](../../github/Crystal/Client/MirScenes/GameScene.cs#L5412)|`SpellObject ob = new SpellObject(p.ObjectID); ob.Load(p);`。|
|[GameScene.cs:2289–2294](../../github/Crystal/Client/MirScenes/GameScene.cs#L2289)|`MapControl.Objects.TryGetValue(p.ObjectID, out MapObject ob)`→`ob.Remove();`：26按ID移除。|
|[Effect.cs:81–96](../../github/Crystal/Client/MirObjects/Effect.cs#L81)|`if (CMain.Time <= NextFrame) return;`、`Owner.SkipFrames`、`RepeatUntil`：Effect与SpellObject不是同一时钟，禁复用比较符。|
|[Effect.cs:38–61](../../github/Crystal/Client/MirObjects/Effect.cs#L38)|`NextFrame = Start + (Duration / Count) * (CurrentFrame + 1);`，Effect为起点排期。|
|[MLibrary.cs:692–709](../../github/Crystal/Client/MirGraphics/MLibrary.cs#L692)|`if (offSet) point.Offset(mi.X, mi.Y);`、`DXManager.SetBlend(true, rate);`：先offset后裁剪。|
|[DXManager.cs:353–389](../../github/Crystal/Client/MirGraphics/DXManager.cs#L353)|NORMAL `SourceAlpha`/`One`；INVLIGHT `BlendFactor`/`InverseSourceColor`；383写BlendFactor不证明NORMAL乘rate。|
|[PlayerObject.cs:1708–1709](../../github/Crystal/Client/MirObjects/PlayerObject.cs#L1708)|ThunderBolt起手 `Effect(Libraries.Magic2, 20, 3, 300, this)`，不能当149图库映射。|
|[PlayerObject.cs:2956–2963](../../github/Crystal/Client/MirObjects/PlayerObject.cs#L2956)|ThunderBolt命中 `Effect(Libraries.Magic2, 10, 5, 400, TargetPoint/ob)`，owner分支不同。|
|[GameScene.cs:4687–4710](../../github/Crystal/Client/MirScenes/GameScene.cs#L4687)|125 `CreateProjectile(410, Libraries.Magic, true, 6, 30, 4, ...)`、命中570；本轮不碰。|

## Rust 实读锚点

|文件:行号|现状|
|---|---|
|[spellfx.rs:27–60](../../crates/mir-client/src/ui/spellfx.rs#L27)|700/4000/1200产品窗；FireWall/PoisonCloud均4000。已实读全文件1–440。|
|[spellfx.rs:287–389](../../crates/mir-client/src/ui/spellfx.rs#L287)|只消费events；age范围过滤，149 FireWall/PoisonCloud为elapsed模6/20；draw_sprites不读spell_fields。|
|[net.rs:1541–1576](../../crates/mir-client/src/net.rs#L1541)|SpellEvent无帧/NextMotion，另有SpellFieldView持久登记；149兼写两表。|
|[net.rs:5322–5333](../../crates/mir-client/src/net.rs#L5322)|26按object_id retain spell_fields，不删spell_events。|
|[net.rs:5624–5647](../../crates/mir-client/src/net.rs#L5624)|149 push_spell_event→register_spell_field；[7424–7475](../../crates/mir-client/src/net.rs#L7424)同ID更新场、追加事件。|
|[projectile.rs:380–468](../../crates/mir-client/src/ui/projectile.rs#L380)|HitEffectView已有严格Effect时钟；不能因已有模型就给SpellObject套用严格>。|
|[entities.rs:1816–2000](../../crates/mir-render/src/entities.rs#L1816)|draw_effect_impl先解码、当帧offset一次、Add/Over分流；blit_cached_add转Frame。|
|[scene.rs:417–462](../../crates/mir-render/src/scene.rs#L417)|MapScene只转发；[frame.rs:216–304](../../crates/mir-render/src/frame.rs#L216) blit_add为CPU Add，无rate。|

## 文件结构、接口与白名单

|属主|允许写入|性质|
|---|---|---|
|A兵部|新增 crates/mir-client/tests/round75_spell_lifetime_contract.rs；target/round75/a/**|仅tests；生产所有crates/src零改。|
|B工部|target/round75/b/**|独立物理副本、Python oracle、逐变异patch/日志，不改A测试。|
|C刑部|docs/testing/round75/round75-c-evidence.md；target/round75/c/**|独立复核报告、隔离负控，不改A/B原件。|
|Lead/协调|docs/process/round75-*.md；target/round75/{lead,gate,menxia,shangshu}/**|独立审核/提交/唯一门禁，处理裁决。|
|D礼部|docs/testing/round75/{README.md,round75-lifetime-contract.md,final-manifest.json,manifest.sha256}；docs/roadmap.md追加R75节；docs/client.md追加R75节；target/round75/d/**|后置差异合同和原号台账，不改历史合同。|

- 测试私有接口：`struct CrystalSpellClock { frame: u32, count: u32, interval: i64, next: i64, repeat: bool, ended: bool, removed: bool }`。
- `fn load(start: i64, count: u32, interval: i64, repeat: bool) -> Self`；`fn process(&mut self, now: i64)`；`fn visible(&self) -> bool`；`fn remove(&mut self)`。
- load用正时戳、next=start+interval再减next%100；process为if一次推进，Repeat回卷/Ended=true，next=now+interval；visible=!removed && (repeat || frame<count)。无Effect式SkipFrames/RepeatUntil/防同刻附加逻辑。
- 生产接口保持 `draw_sprites(frame: &mut Frame, events: &[SpellEvent], scene: &mut MapScene, center: Point, now_ms: i64)`；不用测试模型生成生产期望。
- 夹具参考[R71 wire helper](../../crates/mir-client/tests/round71_firewall_mapping.rs#L680)与[R72](../../crates/mir-client/tests/round72_poison_cloud_mapping.rs#L755)，新文件自包含，不改/导入历史私有helper。

## Review Focus 与 G1–G7 可失败验收

每个G均为新文件中恰一个同名测试。命令模板T：`cargo test -p mir-client --features mir-render/legacy --test round75_spell_lifetime_contract <测试名> -- --exact --nocapture --test-threads=1`。
每条命令将T的测试名替换为下表名称；绿必须EXIT0、恰1 passed/0 failed/0 ignored；红必须编译成功后该测试断言失败EXIT101，0 tests/编译错/资源缺失不算红。

|G/命令T测试名|固定断言、预期与红绿判据|
|---|---|
|G1 r75_g1_initial_quantization_and_equality|参考FireWall start1234→next1300；process1299帧0；1300帧1,next1420。生产age66仍Magic1630，与参考1631不同；实画整帧，不仅算索引。比较符错变异应红。|
|G2 r75_g2_sparse_process_is_one_step|参考start0→next100；只process2000→frame1,next2120；再2120→frame2,next2240。生产age2000→Magic1634；分别固定expected，严禁让差异必须相等。稀疏推进/错误排期应红。|
|G3 r75_g3_repeat_wrap_is_not_removal|参考FireWall ticks100/220/340/460/580/700→frame0,ended=true,removed=false,visible=true；PoisonCloud ticks100+120*k(k=0..19)同样回卷；继续一tick帧1且Ended仍true；不是生命周期截止。|
|G4 r75_g4_product_window_is_4000_not_2000|实际149 FireWall与PoisonCloud事件：age -1空、0/1999/2000/3999有精灵、4000/4001空；在3999的index分别1633/1663。参考repeat对象未remove时仍visible，不把2000当截止。所有图非黑、每帧异色/offset，整帧对预冻结金样。|
|G5 r75_g5_remove_field_leaves_transient_event|真实encode→decode→apply：ID A/B两条149@1000；26(A)@1100后fields仅B、events仍A/B且ID/started_ms不变；draw@1120仍两处精灵。Crystal参考remove(A)仅A不可见。不把当前残影说成正确寿命实现。|
|G6 r75_g6_draw_is_query_and_reentry_appends|G5后同ID A新149@1200位置换格；fields A/B共2，events共3且旧A起点1000/新A1200；draw@1240两次整帧相等、事件/场表逐字段不变，绘制不推进或回收；分别对3事件手算金样，避免同ID去重伪等价。|
|G7 r75_g7_nonrepeat_hides_without_remove|测试私有Blizzard count30/interval100/repeat=false，ticks100..3000后frame30、visible=false、removed=false；3100后frame31仍不删；调用remove才removed=true。另以Effect参考next=80在80不进、81才进，与SpellObject等号进区别断言；不称已迁移Blizzard生产。|

- 五焦点：初始量化/等号、稀疏排期、Repeat与隐藏/移除、4000ms产品窗、149→26→149同ID与纯查询；各有上述G覆盖。
- G1/G2/G4/G5/G6必须走真实draw_sprites；G3/G7明确为参考模型合同，不能混称生产测试。
- 所有expected先冻结字节/SHA/UTC再生成actual；RGB565颜色独立解码、格投影与signed offset手算；禁用effect_image或生产phase helper当oracle。
- B/C独立Python读实际fixture输出G1/G2/G4/G5/G6全部采样整帧SHA与参考时钟完整轨迹；A/B/C三源逐项一致，分别披露覆盖数。

## 精确变异 M1–M5

|变异点（隔离副本单变异）|指定红|
|---|---|
|M1 测试私有CrystalSpellClock.process的 `now >= next`→`now > next`，断言不动|G1等号1300帧1变0；参考模型负控，非生产修复证据。|
|M2 同模型 `next = now + interval`→`next += interval`，其余不动|G2 next2120变220；参考模型负控。|
|M3 同模型Repeat回卷分支 `removed = false`语义改成 `removed = true`（显式赋值可先在基线写入），其余不动|G3首次回卷visible=true失败；参考模型负控。|
|M4 隔离生产spellfx AREA_MS 4000→2000|G4 age2000精灵缺失；生产负控。|
|M5 隔离生产net ObjectRemove分支额外 `snapshot.spell_events.retain(|e| e.object_id != remove.object_id);`|G5事件2变1/像素缺失；生产负控，不把这个未来可能的修复称坏设计。|

- B/C每个变异前7绿→冻结测试断言与夹具SHA→单patch→`cargo test ... --no-run` EXIT0→T指定G红101→恢复原字节SHA→7绿；记录参考模型/生产负控类型。
- M1–M3不可修改固定assert/expected，C逐patch确认仅模型语句变更；编译错误/ignored/0tests/环境失败均无效，保留失败attempt后纠正。
- 本轮基线预期7绿，新增合同无功能修复红；只有负控制造可失败证据，不宣称现有代码已修好Crystal寿命。

## DAG、实施步骤与交接

`门下独立批准 → 尚书登记 → A兵部 → B工部 → C刑部 → Lead提交+唯一门禁 → D礼部 → 独立终审 → 尚书验收 → 门下确认`

- [ ] A（依赖批准）：先写七项固定预期与参考模型/fixture并冻结；跑七绿；至少M4红→恢复七绿；定向回归；提交原字节给Lead，不自行commit。
- [ ] B（依赖A冻结）：物理副本nlink1、独立CARGO_TARGET_DIR/TEMP/TMP/TMPDIR；独立全矩阵oracle与五精确负控；不覆盖A文件。
- [ ] C（依赖B）：新物理副本独立重做G/M与oracle；对原作行号、26事件差异、模型非生产属性、恢复SHA逐项核验，必改0才交Lead。
- [ ] Lead（依赖C必改0）：只提交白名单；唯一全量 `pwsh -NoProfile -File tools/dev/ci.ps1 -KillStale`，不得SkipTests/SkipPostgres；原EXIT/12门禁项目实计，失败保留attempt。
- [ ] D（依赖门禁成功）：六文档、整帧/模型覆盖分别计数、R74历史清单不变（实读artifacts数组34项，题面35项计数口径待Lead核定）；R75清单外置SHA免自引用；纯文档后不重跑CI，crates/tools有新改动则重闸。

未来定向命令（本规划均未运行）：

```text
cargo test -p mir-client --features mir-render/legacy --test round75_spell_lifetime_contract -- --test-threads=1
cargo test -p mir-client --features mir-render/legacy --test round70_effect_blend_offset --test round71_firewall_mapping --test round72_poison_cloud_mapping --test round73_thunderbolt_hit_mapping --test round74_normal_blend_lock --test round63_projectile_wire
cargo clippy -p mir-client --all-targets -- -D warnings
cargo fmt --all -- --check
```

- 新增恰7 passed/0 failed/0 ignored；历史目标计数以实跑为准，禁止将R73旧命名误写成不存在的round73_thunderbolt_mapping。
- 每命令保存UTC起止/cwd/HEAD/完整argv/EXIT/stdout/stderr/计数与bytes/SHA；证据根target/round75/**，attempt只增不改，隔离副本禁硬链接/junction/symlink。

## PARTIAL承接、风险、回滚与裁决

- GAP-1/2、R70-C2完整DrawBlend/rate/Mode/DrawColour仍PARTIAL；不推导或实现src×0.8×alpha。
- GAP-5/R70-C1其余149技能/起手/1620/其他调用点，R70-C3尺寸/居中，GAP-3/4/6 mask/tint/shadow/Light/AnimationOffset照承；shadow只解析不消费。
- R71时钟/对象、R73-T1仍PARTIAL；本轮只把NextMotion/Repeat/26差异转为软件合同，不CLOSED生产寿命；R73-T1历史注释修正仅该子项已完成。
- R72 B plan-M4与legacy lint债照承；R71-C7永久PARTIAL，E1仅补救CLOSED；R69条件5永久证据缺口；R68/R49–R70/R61–R67全部原号照承。
- R61 P01–P12、R62①–④、R63 P1–P7、R64①–⑥、R65 G1.2/G2、R66生命周期/全局水位、R67容量/保护/map epoch均不因本轮清零。
- 风险：模型测试自证，故用冻结轨迹+独立Python对原文复算；现状负控未来修复会红，未来替换合同必须注明supersede与真实生产新验收。
- 无资源/数据库新增依赖；legacy为合成夹具必需；纯软件不证明真实素材/设备视觉/全UI/跨后端/Live音频。
- 回滚由Lead仅撤新增测试/本轮文档；生产零改，无迁移，无数据删除；历史证据与两freeze原字节保留，不reset共享树。
- 门下/Lead须裁定：是否接受“现状差异合同、生产零改、7测试含2纯模型项，完整寿命仍PARTIAL”的完成定义；通过前不派发，作者不自批。
