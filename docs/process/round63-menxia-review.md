# R63 门下独立评审：PASS-WITH-CHANGES

结论：附条件准奏；必改 3 项。可转 Assigned，由尚书先落实 M1–M3 再派发；本裁决不代表功能通过或准许跳过真红。

## 基线核对与命令证据

- `pwd`：`/c/Users/diana/Documents/github/mir-next`；`git rev-parse HEAD origin/main` 均为 `f80724ee3e0d4242e7be1af60ece09ad8b48c432`。
- `git status --short`：仅 3 项未跟踪：[计划](round63-plan.md)、[冻结前件](../testing/monster-native/player-resource-freeze-before.json)、[冻结件](../testing/monster-native/player-resource-freeze.json)；受控文件无修改，不能将当前树称为全干净。
- Node `fs.readFileSync` + fatal UTF-8 TextDecoder + crypto SHA256：计划 13,148 B、123 行、UTF-8 无 BOM、无 CR（LF）、末尾 LF。
- 计划 SHA256=`cbd122e19df4fc1673904c9d84ca84a098b8718a43f9c53a2094405dfa4a1181`，与输入一致。
- `node C:/Users/diana/.dsh/skills/edict/scripts/edict.mjs show JJC-20261026-010 --json`：Menxia、todos=[]；`recall` 初始无记忆；`validate`：187 条符合 schema v3.1，exit 0。
- 文件工具按下表行区间独立读取原文；`git diff --check` exit 0。未运行 cargo/CI，以下为静态评审，G2 真红尚未发生。

## 锚点核对表

| 核对项 | 独立证据与判断 |
|---|---|
| 570 建立与时钟 | [Effect](../../github/Crystal/Client/MirObjects/Effect.cs#L38-L48) 构造 Start+(600/10)；[Process](../../github/Crystal/Client/MirObjects/Effect.cs#L81-L96) 等号不进、skip 额外+1再普通+1、到10移除、按 Start 重算，无 elapsed 强删。 |
| 目标与绘制 | [GameScene](../../github/Crystal/Client/MirScenes/GameScene.cs#L4701-L4710) 完成时目标非 Dead 才挂570与+2音；[Draw](../../github/Crystal/Client/MirObjects/Effect.cs#L116-L119) 使用 Owner.DrawLocation，570 Owner 必须是目标。 |
| Rust 差异 | [projectile](../../crates/mir-client/src/ui/projectile.rs#L319-L397) 飞行已有严格时钟，570 sprite/finished 仍为 elapsed；G1 必要。同刻/回退守卫是客户端附加约束，计划已披露。 |
| 历史断言 | [R62 测试](../../crates/mir-client/tests/round62_projectile_wire.rs#L229-L253) 等号取新帧、600ms移除与 Crystal 冲突，必须 supersede；保留名称、跟随、死/未知目标、每发 cue 与十帧覆盖。 |
| G2 根因 | [缓存/consume](../../crates/mir-client/src/ui/projectile.rs#L506-L674)：629–632先填、640复位、672 clear，首批未被 spawn 引用的对象位置确被清。spawn:693/703、advance:755–756会回填源/目标，故普通目标8会掩盖问题，对象9设计有辨识力。 |
| G2 公开路径 | [R62 helper](../../crates/mir-client/tests/round62_projectile_wire.rs#L98-L130) 已有 new_session、encode/decode、apply_packet_at、UiState::sync；新测须让 Monster/Remove/125 全走编解码，不能原样复用仅125编码的 helper 冒称全 wire。 |
| TargetPoint/回收 | [Crystal](../../github/Crystal/Client/MirObjects/PlayerObject.cs#L3546-L3556) 对象位置优先、否则 TargetPoint，不证明全局LRU；[生产事件](../../crates/mir-client/src/net.rs#L7287-L7311) total饱和自增且 append-only，游标不等于物理回收。 |
| ActionFeed 来源 | [Player](../../github/Crystal/Client/MirObjects/PlayerObject.cs#L750) 非自机 Count>0，966–967 FIFO；[Monster](../../github/Crystal/Client/MirObjects/MonsterObject.cs#L346) Count>1，528–529 FIFO；[Rust](../../crates/mir-client/src/net.rs#L1149-L1153) 覆盖动作，5253–5274与5336–5345无等价FIFO，生产 skip=false 合理保留 PARTIAL。 |

## 三条门下决定

1. 批准 G1 为功能批核心，G2 为须证实的候选；ActionFeed 完整 FIFO 后续承接。按包数/时间猜 skip 无原版依据，不能为单轮凑齐而混入。
2. 批准 G2 公开 wire 真红前置；静态根因成立且对象9用例可执行，不是无根因扩面。未取得真实 assert 失败，移出功能批，继续 PARTIAL；编译失败不算红。
3. 批准容量/LRU/物理裁剪只交契约；不指定任意容量、不造协议。活跃引用保护、miss少生成、最低水位与慢消费者/clone/重连/饱和均应保留未决。

## 必改与建议

- M1（事实，计划:6）：初始干净为规划时点陈述，本次见三项未跟踪。最小修法：尚书准入记录补当前 status 与三文件归属；只显式暂存白名单，冻结 JSON 不得混入提交，不删除既有文件。
- M2（红证据时序，计划:53、64、68–70、92）：A 已改实现后 B 才新增 wire，当前描述不足以取得“未改实现”红。最小修法：保留串行；B 在其 target/round63/b 白名单内从 f80724e 重建隔离基线、只叠加新公开行为测试，过滤新测取 assert 红，再在功能树同一测试绿；记录基线SHA/测试SHA/命令/exit/断言，C核验。不得改回生产树或以旧断言报错替代新G2红。
- M3（可执行预期，计划:50）：明确“生成一次、无实体/命中音”中的实体类别。最小修法：对象9在 WorldSnapshot 缺席；第二条125新增投射物1、出生cue1、落点等于首批9位置；完成后570与命中cue均0。9重现不补播旧125；负例99不生成且无出生cue。
- 建议 S1：G1 既测1060/1061等边界，也对最终1600/1601与大跳后11001列完整状态表；勿把离线 helper 的十帧公式当生产时钟绿证据。
- 建议 S2：D显式列缓存缺陷修复前后范围；P03不能缩写成只有容量/LRU便声称全局TargetPoint已闭合。

## 派发准入条件与门禁

- 尚书落实 M1–M3 并将本裁决写入 A/B/C 提示词后准派；A→B→C→D 串行，C与D之间须有Lead功能提交和唯一门禁。
- 白名单足够：A仅实现模块，B新wire与R59/R61/R62历史测试，C只写target证据，D文档。编译需要 net/app/ui/mod 等越界时退门下，不能默扩；D不得覆盖本独立裁决或历史冻结证据。
- B改写历史测试按计划:42–43保留覆盖、公开 supersede，并在新manifest映射旧测试/语义→新断言/提交；C独立复跑unit+四integration targets并核对注入skip与生产false。
- 计划:81–89定向命令完整；0 tests不能PASS；SKIP→PARTIAL且总结果非全PASS，ignored列位置及旧/新数，R62既有7须核对，不能由本审推定R63数量。
- 顺序批准：功能批commit→Lead唯一 ci.ps1（功能HEAD、attempt日志/exit/项数/hash）→D按事实文档批；失败保留、另编号重试；门禁后功能/测试变化必须重新门禁；D不得预写PASS。
- [R61 P01–P12](../testing/round61/round61-final-audit.md#L43-L56) 与计划:112逐项对应齐全；计划:115禁称覆盖全链/生产skip/限容/回收/600ms必删/Live/全场景无丢重/十二项全闭合。
- 下发前须补事实：M1当前树归属与显式暂存清单、M2基线红复跑责任人与路径、M3精确计数；红绿日志/实际ignored/功能HEAD/CI结果只能实施后据实补。

## 边界声明

本审仅新增本文并用 edict CLI 留裁决、转 Assigned、validate；未修改 crates 或其他文档，未commit/push，未运行cargo/ci.ps1。Assigned表示附条件范围准入；G2 wire、功能绿与唯一门禁均未在本审验证。
