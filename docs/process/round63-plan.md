# R63 中书规划：570 严格时钟与缓存契约

## 一、决策与事实基线

主任务：`JJC-20261026-010`「第63轮：570严格时钟与缓存契约」。本阶段只规划、建主单并转Menxia/validate；不建子单、不改crates、不commit/push、不运行cargo/ci.ps1。
受测基线：只读核实 `HEAD=origin/main=f80724e`，初始工作树干净；R62功能 `a7362fc`。edict初始validate为186条有效，stats Done159/Cancelled27/未结0；建主单后应187条、未结1，以最终CLI输出为准。
历史证据：`docs/testing/round62/README.md:3-14,22-25` 记录PASS-WITH-PARTIAL、Lead门禁EXIT=0/12项、558 passed/0 failed/7 ignored。门禁日志SHA256=`81e8ef05131460635adaee4bf02066d4e2187a2a852573a8ae7191a23adfd9ab`；本规划未重跑，不替代R63门禁。
候选来源：`docs/roadmap.md:213-232`、`docs/testing/round62/README.md:65-71`。本轮选G1「570严格状态时钟」+G2「跨代首批位置缓存刷新顺序」；G3容量/回收只交契约文档。
降范围理由：Rust当前没有ActionFeed FIFO，直接按包数/时间推测skip不能严格对齐；完整排队涉及移动、攻击、预测/动画生命周期，不是读取一个字段。将建议的ActionFeed核心延期交门下裁决，继续PARTIAL。
G1/G2同属客户端单模块，能真红→绿且不依赖声卡、素材、服务端或新协议；容量驱逐会丢可信落点，因此不实现任意上限/LRU。

## 二、Crystal与现状锚点

行号为当前基线；实施后须回填新锚点，不覆盖历史冻结证据。

| 项目 | Crystal锚点 | Rust锚点与现状 |
|---|---|---|
| 570建立 | `github/Crystal/Client/MirScenes/GameScene.cs:4701-4709`：目标存在且非Dead，Effect(570,10,600,target)，播+2 | `crates/mir-client/src/ui/projectile.rs:363-397`：HitEffectView只有started_ms，sprite_index/finished均elapsed |
| 严格时钟 | `github/Crystal/Client/MirObjects/Effect.cs:38-48,81-96`：NextFrame=Start+60；now<=NextFrame不进，skip额外+1，再普通+1，frame>=10删除 | `crates/mir-client/src/ui/projectile.rs:319-338`飞行已有严格时钟；`:388-396`的570未使用 |
| Owner跟随 | `github/Crystal/Client/MirObjects/Effect.cs:116-119`：Owner.DrawLocation | `crates/mir-client/src/ui/projectile.rs:370-377`记录target_id/position；保留目标跟随与已有消失规则 |
| 旧断言冲突 | `github/Crystal/Client/MirObjects/Effect.cs:81-96`无墙钟600ms强制删除 | `crates/mir-client/tests/round62_projectile_wire.rs:229-253`要求等号进帧/恰600ms删除，需supersede |
| TargetPoint | `github/Crystal/Client/MirObjects/PlayerObject.cs:3546-3556`优先对象位置，否则TargetPoint；非全局LRU | `crates/mir-client/src/ui/projectile.rs:506-512,627-640,668-674`先填缓存再跨代clear，首批可见位置被清；静态发现待红测 |
| ActionFeed | `github/Crystal/Client/MirObjects/PlayerObject.cs:750,966-967`非自机Count>0、FIFO出队；`github/Crystal/Client/MirObjects/MonsterObject.cs:346,528-529`Count>1、FIFO出队 | `crates/mir-client/src/ui/projectile.rs:480-490`生产false；`crates/mir-client/src/net.rs:1033-1042,1106-1113,1149-1153,5253-5274,5336-5345`只存当前动作/时间、直接覆盖 |
| 125回收 | `github/Crystal/Shared/ServerPackets.cs:3560-3578`仅Spell/Source/Destination；`github/Crystal/Client/MirScenes/GameScene.cs:4687-4715`逐包建立 | `crates/mir-client/src/net.rs:7287-7311`append-only/total；消费者游标不证明生产者物理回收 |

## 三、最小修改与测试计划

### G1：570严格时钟（核心）

只给HitEffectView补frame/next_frame_ms/last_tick_ms等必要状态；frame=0、NextFrame=Start+60。在advance_hit_effects的sync路径推进；sprite_index只读frame，finished按frame>=10，绘制查询不得推进。
可保留sprite_index(now_ms)/finished(now_ms)兼容签名，不能再以elapsed决定生命周期。hit_effect_index若为历史兼容保留，注明仅离线公式helper，生产HitEffectView不再调用。
无需重建Effect/Missile类体系；共享小型时钟逻辑不得破坏已有飞行断言。570的Owner是目标，飞行Owner是源，不能传反。
skip接口仍生产false；纯时钟测试可显式注入true，不得据此宣称ActionFeed已接。保留R62同刻幂等、回退不推进的客户端约束并披露。

新增模块单测名与断言：

- `round63_hit_clock_strict_boundary_and_completion`：start1000；1060仍570；1061→571；1120仍571；1121→572；逐边界+1至1541→579；1600仍579；1601删除。
- `round63_hit_large_jump_advances_once_and_draw_is_pure`：新效果跳11000只进1帧；同刻sync及连续sprite_index不再进；11001才再进，不追补elapsed。
- `round63_hit_skip_adds_one_extra_frame`：等号不进；true到期+2、false+1；到/越10恰删一次。
- `round63_hit_clock_rollback_is_noop`：回退/同刻不变，后续有效tick可继续；不用sleep或真实时钟。

改写原测试且保留覆盖：R59 `hit_effect_plays_all_ten_magic_frames_when_the_target_survives`；R62 `real_125_spawns_once_flies_hits_and_keeps_the_impact_ghost`、`each_projectile_plays_its_spawn_and_hit_cue_exactly_once`。
检查R61所有命中生命周期断言；原600ms一次大跳改为明确逐边界+1采样，保留跟随、死/未知目标无命中、每发出生/命中各一次。记录旧elapsed断言supersede，不删测试凑绿。

### G2：首批缓存窄修复

consume先执行已约定的跨代判定/复位，再填本帧可信位置，最后spawn事件；不改wire、total、游标和发号。
不更改R62「新代空首批延后到首批载荷」和旧形态重建语义；len=0且total>0的更广边界继续PARTIAL。

- 新增wire测试 `round63_first_payload_positions_survive_generation_reset`：新代首批有源7、目标8及无关可见9，125打8；下一同代快照移除9、新125打9，必须用首批缓存位置生成一次、无实体/命中音。
- 明确9从未被早先投射物命中过，避免spawn记录目标位置掩盖刷新顺序缺陷；9重现不补播旧125。
- 新增 `round63_generation_reset_drops_old_positions`：旧代见ID99，新代未出现但125打99，应零生成/零出生音；新代合法缓存有效，旧位置不可跨代复活。
- 用真实ObjectMonster/ObjectRemove/ObjectProjectile编解码→snapshot→UiState::sync；不直接填私有缓存。未改实现先得真实assert失败；不能复现就移出功能批、保留静态风险/PARTIAL。

### G3：容量与回收契约（仅文档）

对应上述缓存/125锚点。明确Crystal没有全局位置缓存LRU或回收ack字段；不发明服务端协议。
候选契约：保护活跃引用位置，只考虑淘汰无引用历史项；miss沿用不造坐标/不补播，并披露会少生成的风险。容量数值、LRU更新时机、换图/ID复用失效、保护项超额策略均待后续实证，不写随意常数或已限容结论。
物理裁剪须先定义同代绝对序号区间、所有消费者最低确认水位、慢消费者/克隆/重连/total饱和；无可靠本地确认时维持append-only/PARTIAL。仅写客户端确认/服务端生成边界契约，不改服务端。
文档验收：刑部核对无伪造字段/上限/回收通过结论，礼部运行链接和差异检查。

## 四、尚书派发DAG与白名单

严格 **A(bingbu)→B(gongbu)→C(xingbu)→D(libu)**；计划门下准入后尚书才建子单并delegate/todo/result。本阶段只建主单。

| 子任务 | 下发提示词要点 | 文件白名单 | 验收 |
|---|---|---|---|
| A兵部 | 按Effect逐行、先真红后绿实现G1/G2，Owner/兼容接口说明；不扩ActionFeed/LRU；原日志与报告 | `crates/mir-client/src/ui/projectile.rs`、`target/round63/a/**` | unit命令、diff |
| B工部 | 真实wire新增G2/570断言；公开supersede旧elapsed；实现缺陷退A，不改实现凑绿 | `crates/mir-client/tests/round63_projectile_wire.rs`、`crates/mir-client/tests/round59_projectile_render.rs`、`crates/mir-client/tests/round61_projectile_queue.rs`、`crates/mir-client/tests/round62_projectile_wire.rs`、`target/round63/b/**` | 四integration targets |
| C刑部 | 独立读代码复跑、核实真红与注入skip/生产false区别；缓存不能复现退回；统计与禁称核对 | `target/round63/c/**`；其余只读 | unit/integration、clippy/fmt/diff；禁跑ci |
| D礼部 | 接收C及Lead门禁事实，写G3与十二PARTIAL、supersede/manifest/hash，不预写PASS | `docs/testing/round63/**`、`docs/process/round63-*.md`、`docs/roadmap.md`、`docs/client.md`、`target/round63/d/**` | links/diff/JSON/hash |

实现白名单不含net.rs、app.rs、ui/mod.rs、mir-render、audio.rs或依赖文件；编译若必须越界，先给最小差异退门下裁决，不静默扩面。
D不得覆盖R59/R61/R62冻结JSON/日志；新manifest记录旧测试/语义→新断言/提交映射。

## 五、未来实施验收命令（本规划不执行）

仓库根目录运行；必要时沿用target/tmp可写TEMP/TMP。每次记录HEAD、UTC起止、命令、stdout/stderr、exit；不得空跑0 tests当PASS。

```bash
cargo test -p mir-client --lib ui::projectile::tests -- --nocapture
cargo test -p mir-client --test round63_projectile_wire -- --nocapture
cargo test -p mir-client --test round59_projectile_render -- --nocapture
cargo test -p mir-client --test round61_projectile_queue -- --nocapture
cargo test -p mir-client --test round62_projectile_wire -- --nocapture
cargo clippy -p mir-client --all-targets -- -D warnings
cargo fmt --all -- --check
py -3 tools/docs/check_links.py
git diff --check
```

红阶段仅加测试/断言，未改实现运行新测试过滤器；新字段尚无导致编译失败不算真红，改用公开行为断言取assert失败。
绿阶段A跑unit，B跑新wire及三历史targets，C独立复跑；无素材用例不能early-return伪绿，SKIP→PARTIAL并使总结果非全PASS，ignored须列旧/新数量及位置。
D如写run-checks仅调用定向命令，不嵌ci.ps1。新增manifest须JSON可解析；hash区分提交blob/工作树，不自引用。
计划验收UTF-8无BOM/LF/≤220行；新增受控文件≤1MiB；不入.env、resource、target原日志或大冻结JSON。

## 六、提交和唯一门禁顺序

1. 门下审计划，尚书准入派发A→B→C。C完成定向测试及独立审查后交Lead。
2. Lead按白名单先提交**功能批**（实现+新旧测试），记录提交号；本规划者不commit/push。
3. 仅Lead在该功能HEAD执行 `pwsh -NoProfile -File tools/dev/ci.ps1 -KillStale`；保存 `target/round63/gate/attempt-1.log`、exit、实际项数、SHA256；其它角色不得启动入口。
4. 失败保留原日志，退A/B→C→新功能提交→Lead另编号attempt-N，不覆盖失败证据、不借R62门禁。
5. 唯一门禁通过后D按事实完成**文档批**，Lead检查links/diff/hash后提交；独立门下终审→指定confirm_by确认→Lead决定push。
6. 门禁后任何功能/测试修改都使旧HEAD门禁失效，回到功能提交→Lead门禁；纯文档事实可后提交。

## 七、风险、不做、PARTIAL与禁称

- 严格时钟不保证墙钟600ms消失，稀疏tick延长表现；等号保持旧帧，必须修订历史错误注释/断言。
- 绘制不得二次推进，新生效果同sync不立即越帧；删除不再播命中cue。保留目标移动/消失/死亡既有规则，披露与Crystal移除对象模型差异。
- 缓存修复仅覆盖已观察位置；不闭合全局TargetPoint、LRU、换图/ID复用或新代len=0/total>0。
- ActionFeed仍PARTIAL：玩家非自机Count>0与怪物Count>1不可统一阈值；最新action/包数不是FIFO。
- R61原编号逐项承接：P01物理回收/clone/容量；P02生产SkipFrames；P03缓存容量/LRU；P04 ImageInfo.offset；P05设备/Live；P06削波；P07 105sfx；P08全局HiddenSpanSource；P09 CWD/素材；P10剩余会话边界；P11缺素材early-return；P12 R60 P-1..P-6。
- 音频证据 `docs/roadmap.md:225-231`：33路peak=22.844788；`crates/mir-client/src/audio.rs:716-741`逐源amplify入mixer仍在，不增limiter/重构音频。
- 不做下载/素材/数据库、新协议/依赖、完整动作系统及无测量性能宣称；不整项清零十二PARTIAL。
- 禁称：「R63已PASS」（实施前）、「125全链已证/完整」、「生产SkipFrames已接」、「缓存已限容/无内存增长」、「已物理回收」、「严格时钟=600ms必删」、「真实可听/Live已证」、「无丢失/无重复全部场景」、「十二PARTIAL全闭合」。

## 八、门下待裁决

1. 是否批准570严格时钟+缓存首批顺序为功能批，ActionFeed完整FIFO后续承接；不同意则退回规划，不临时扩面。
2. G2静态发现须公开wire真红证实；不能复现则移除功能范围、保留PARTIAL。
3. 容量/LRU和物理裁剪仅交契约、不指定任意容量、不发明服务端生命周期协议，继续PARTIAL。

当前为中书规划成稿，主单提交Menxia；审批、测试及门禁结论不由计划预写。CLI状态/validate及文件行数/SHA256另向Lead回报。
