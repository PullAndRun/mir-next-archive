# R65-C 刑部独立核验

- Verdict：**PASS-WITH-PARTIAL；必改 0**。限定 G1 既有单 UI 生命周期的回归补证；不替代 Lead 功能提交、唯一门禁或最终门下裁决。
- 子单 `JJC-20261027-002-sub-8c780054`；operator `xingbu-r65-c / gpt-6.1-sol`；实际 HEAD=`2deacee47efc9e505d034fe87e58692dfeb50187`。
- 已全文读取 [dispatch](../../process/round65-shangshu-dispatch.md)、[plan](../../process/round65-plan.md)、[review](../../process/round65-menxia-review.md)。证据根 [attempt-1](../../../target/round65/c/attempt-1/runs.json)。
- 独立绿环境：`CARGO_TARGET_DIR=C:/Users/diana/Documents/github/mir-next/target/round65/c/cargo-target`；`TEMP=TMP=TMPDIR=C:/Users/diana/Documents/github/mir-next/target/round65/c/tmp`；未复用 Lead/A/B 二进制。
- 绿 UTC `2026-10-26T19:41:09.278Z`→`19:42:47.171Z`，窗口 97.893s；各命令起止、原始 EXIT、完整双流日志 bytes/SHA 见 [runs.json](../../../target/round65/c/attempt-1/runs.json)；成本 L2/L3，所要求独立运行无法以静态核对等价替代。

|核验项|结果|独立事实/证据|
|---|---|---|
|1 独立复跑|PASS|下表 13 命令全 EXIT=0，10 次测试共 60 次执行，0 failed/ignored，无 SKIP/0 tests|
|2 红证据真实性|PASS|A 3、B 2 变体原日志/补丁/宿主核对；C 另重放错前缀，EXIT101，见红表|
|3 Crystal/协议|PASS|下列所有锚点实读一致；[protocol diff](../../../target/round65/c/attempt-1/git-protocol.txt) 0B，无新增协议/容量/全局策略|
|4 契约口径|PASS/PARTIAL|1-based=`reclaimed+i+1`；三元组/5 cues/身份/重复 drain 行为齐；clone/饱和仅限定证明|
|5 范围/卫生/披露|PASS/PARTIAL|新增 ignore/allow=0；历史 ignore=7；A/B 源码范围正确；辅助证据文字/编码偏差见末项|
|6 实现/断言缺陷|PASS|本轮未发现真实实现或断言缺陷；无需代改；既有能力只补可失败回归|

|命令（统一前缀 `cargo`）|发现=通过/失败/ignored|EXIT|与自报比对|
|---|---|---|---|
|`test -p mir-client --lib r65_ -- --nocapture`|5/0/0|0|A 5 一致|
|`test -p mir-client --bin mir-client r65_ -- --nocapture`|1/0/0|0|A 1 一致|
|`test -p mir-client --test round65_projectile_lifecycle -- --nocapture`|5/0/0|0|B 5 一致|
|`test -p mir-client --lib r64_ -- --nocapture`|7/0/0|0|A 7 一致|
|`test -p mir-client --bin mir-client r64_ -- --nocapture`|1/0/0|0|A 1 一致|
|`test -p mir-client --test round64_event_reclaim -- --nocapture`|4/0/0|0|批准回归命中 4|
|`test -p mir-client --test round64_projectile_ack -- --nocapture`|3/0/0|0|B 3 一致|
|`test -p mir-client --lib ui::projectile::tests:: -- --nocapture`|26/0/0|0|B 26 一致|
|`test -p mir-client --test round63_projectile_wire -- --nocapture`|3/0/0|0|实际 target 命中 3|
|`test -p mir-client --test round62_projectile_wire -- --nocapture`|5/0/0|0|实际 target 命中 5|
|`fmt -p mir-client -- --check`；`fmt --all -- --check`|无格式差异|0/0|满足用户及计划口径|
|`clippy -p mir-client --all-targets -- -D warnings`|无 warning/error|0|A/B 一致|

|红证据（原件只读）|原始 EXIT / 失败点|独立核验|
|---|---|---|
|A trim-noop|101；half：`(0,4,4)`≠`(2,2,4)`；另 lifecycle 0≠2、TCP ack 0≠1|编译成功、命中5；3失败；[原日志](../../../target/round65/a/attempt-1/red-lib-trim-noop.log)|
|A ack-none|101；app `None`≠`Some(AckProjectileEvents { generation:41, through:2 })`|命中1且失败；[原日志](../../../target/round65/a/attempt-1/red-bin-ack-none.log)|
|A wrong-prefix|101；half 身份 1≠3，其余4绿|数量仍正确而身份失败；[原日志](../../../target/round65/a/attempt-1/red-lib-trim-wrong-prefix.log)|
|B trim-noop|101；G1.1:253/G1.2:333/组合:404/边界:469 均0≠2|5命中、4失败、1负向对照绿；[原日志](../../../target/round65/b/attempt-1/red-trim-noop.log)|
|B wrong-prefix|101；上述4测试均在身份 helper:184 得1≠3|非编译错/旧失败/空断言；[原日志](../../../target/round65/b/attempt-1/red-trim-wrong-prefix.log)|
|C 最小独立重放|101；G1.1 身份 helper:184，left1/right3，1发现/1失败/4过滤|[命令/meta](../../../target/round65/c/attempt-1/independent-red-meta.json)、[日志](../../../target/round65/c/attempt-1/independent-red-wrong-prefix.log)，UTC19:43:54.898Z→19:44:31.900Z，37.001s|

- C 红副本只写 `target/round65/c/red-copy/`；另用全新 `target/round65/c/red-cargo-target`，同 C tmp；复制 B 获批错前缀源码，未改测试或生产树。net SHA=`02d311416f670ea2e7d34be96ad7e8dd558ecb1bc83848b0c6702135757d8375`。
- [static-audit.json](../../../target/round65/c/attempt-1/static-audit.json)：A manifest `f57e36de…` 内55个带path哈希逐项一致；6绿块完整属性/函数边界及宿主切片匹配，11红块与各自红宿主切片、绿块 SHA 全等；五个桩 diff 均只涉及获批函数体。
- B 主树/src-tree/noop/wrong-prefix 四份全字节 SHA=`dfef7383843f9e72c97f6999c058281c0d61ddf5f00b64d878d001c9667333e2`，均649行/28113B；[auxiliary-checks.json](../../../target/round65/c/attempt-1/auxiliary-checks.json)复算 B 两清单35文件无差。C 红测试同 SHA。
- [net](../../../crates/mir-client/src/net.rs#L3030):3030/3046/3070、[projectile](../../../crates/mir-client/src/ui/projectile.rs#L602):602/715–761、[app](../../../crates/mir-client/src/app.rs#L210):210 均是 R64 已有；net:3869–3897 等待同 Receiver、predicate 满足返回且不ack；app:2807 同步后选ack；net:4300 gate、4592–4604 ack本地返回后才可能send。

|Crystal 锚点（本地原件）|核对结果|
|---|---|
|[Effect.cs](../../../github/Crystal/Client/MirObjects/Effect.cs#L144):144/191–195|PASS；活动 Missiles 构造加入、Remove移除；无历史消费注册表|
|[GameScene.cs](../../../github/Crystal/Client/MirScenes/GameScene.cs#L4687):4687–4709|PASS；缺源return；出生音；目标存在且未死才命中570/音|
|[PlayerObject.cs](../../../github/Crystal/Client/MirObjects/PlayerObject.cs#L750):750/966–967/3546–3550|PASS；非自机ActionFeed>0；FIFO；当前目标位置优先，否则TargetPoint|
|[MonsterObject.cs](../../../github/Crystal/Client/MirObjects/MonsterObject.cs#L346):346/528–529|PASS；ActionFeed>1；FIFO；不外推生产SkipFrames已接|
|[ServerPackets.cs](../../../github/Crystal/Shared/ServerPackets.cs#L3564):3564–3578|PASS；125仅Spell/Source/Destination读写，无ack/序号字段|
|[Map.cs](../../../github/Crystal/Server/MirEnvir/Map.cs#L2375):2375–2382|PASS；地图分类/cell对象移除，不是125回收协议|

|裁决映射|结果/落点|
|---|---|
|C1 / M3|PASS；B:211–310三状态、e1→e5视图/身份、累计cue5、重复sync/drain0；实际宿主及命中见复跑表|
|C2 / C6 / M2|PASS；五补丁仅函数体；红原始EXIT/左右值及SHA同一性成立；水位含本条，零基偏移与其分开|
|C3|PASS/PARTIAL；B:315–376逐项clone独立/不钉回收；第二UI离线；total饱和只证不溢出|
|C4|PASS；A net:9616/9700真实TCP gate+零字节/Turn正对照与等待predicate不自动ack；app:3902命中1|
|C5 / C7|PASS/PARTIAL；B:456–649重复/后退/夹尾/旧代拒绝/新代空批延迟复位及views/570/cue/cache行为；未扩禁区|
|C8|PASS；C独立构建/tmp/UTC/EXIT/完整双流/SHA齐；无过滤空跑；唯一门禁留Lead|
|M1|PASS；A +413 net/+129 app 均在cfg(test)模块，B仅新测试；C只写本报告与C证据|
|M4|PASS/PARTIAL；锁定历史材料未改；以下原编号风险继续承接，D最终文档仍待实际交付核对|
|M5 / M6|PASS/PARTIAL；G2整项移出；C后Lead提交/门禁再D，未预签未来HEAD/门禁结果|

- PARTIAL台账：R61 P01 clone/全局最低水位/长期内存；P02生产SkipFrames；P03全局TargetPoint/容量/失效；P04 ImageInfo/渲染；P05/P06设备Live及33路peak22.844788削波；P07/P08/P09音效逐项/全局HiddenSpanSource/CWD来源；P10饱和恢复；P11/P12缺素材/R60六项，均保留。
- R62四项：仅单UI本地物理回收子项补证、服务端未做；known_positions容量/LRU未做；生产owner_skip_frames=false；570局部时钟不外推素材/全局。
- R63 P1动作链、P2限容/服务端全局回收、P3位置缓存容量、P4真实素材可听渲染、P5局部时钟/R61承接、P6历史阶段≠终态、P7旧gate meta缺EXIT/HEAD，全部保留。
- R64六项：多消费者/clone最低水位、事件容量、known_positions容量/LRU、服务端回收、定向≠门禁、历史原编号承接；G2未做；未确认积压与clone可继续增长，慢消费者无共享历史保证，同步等待器可取快照不ack，饱和恢复/唯一序号未证。
- 非阻塞证据偏差：A三份patch.json实际为GB18030可解析，非UTF-8；旧encoding-normalization记录非最终这些文件的现值。C保全原字节并复算宿主/补丁，未据该旧转码表认定SHA。B red-evidence:52应为“删尾后仍e1”，不是“变e3”，原日志left1/right3正确；B披露旧v1日志被覆盖，不能作为首败保全证据，当前最终字节红/绿及C独立重放有效。A“无PARTIAL”仅解释为其本地子项，不能冲销本台账；B饱和直写字段、clone离线、app红桩宿主不可见披露属实。
- 历史ignore共7且均存在HEAD：[attack_variant](../../../crates/mir-client/tests/attack_variant.rs#L528):528；[attack_variant_live](../../../crates/mir-client/tests/attack_variant_live.rs#L817):817；[cli_monster_frames](../../../crates/mir-client/tests/cli_monster_frames.rs#L133):133/336；[monster_motion](../../../crates/mir-client/tests/monster_motion.rs#L274):274；[remote_motion](../../../crates/mir-client/tests/remote_motion.rs#L907):907；[remote_motion_live](../../../crates/mir-client/tests/remote_motion_live.rs#L337):337。新增ignore/allow=0，禁称词仅否定/风险引用，未正面宣称相关能力。
- 边界：未改crates/旧证据/历史锁定材料，未commit/push，未运行 `tools/dev/ci.ps1`。起止源码SHA一致；[status](../../../target/round65/c/attempt-1/git-status.txt)/[stat](../../../target/round65/c/attempt-1/git-stat.txt)区分既有A/B和既有未跟踪文档/冻结JSON，不归咎本轮C；C报告是唯一新增受控文档。
