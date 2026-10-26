# R66 最低水位契约（待审计）

限定完成（测试/契约层）。模型只在 integration-test support 中使用。
生产多消费者、全局水位、服务端回收仍 PARTIAL；不作本轮终态声明。

1-based 含本条水位：`水位 = reclaimed + i + 1`，其中 i 是表内零基下标。
`reclaimed = total - len`；夹具采用小计数、不饱和，0 可为合法慢成员。

`minimum_confirmed` 的精确测试接口：

```rust
fn minimum_confirmed(
    generation: u64, reclaimed: u64, total: u64,
    consumers: &[(u64, u64)],
) -> Result<u64, ReferenceError>
```

校验必须逐阶段覆盖全集合，成员排列不能改变错误结果：
`InvalidBounds → NoConsumers → GenerationMismatch → BelowReclaimed`
`→ BeyondTail → min`。对应条件为 floor>tail、空集、任一异代、任一
水位<floor、任一水位>tail；五类 Err 均不得调用真实裁剪。成功才取 min。
单成员4→4，重复2→2，快4/慢2→2；floor0 时慢0→0，阻止回收。

|对象|钉住回收|自动 ack|现状缺口|
|---|---|---|---|
|单 app UI|单水位决定前缀|非零前进时本地上报|未聚合多成员|
|旧 clone|否；旧值独立|否|不能挽回权威裁后历史|
|同步 waiter|否|否|同 Receiver 取走事件；无广播|
|未来显式消费者|模型中 min 可保护|否；显式输入|注册/退出/租约/补发未接生产|

矩阵落点：[app:210](../../../crates/mir-client/src/app.rs#L210)、
[clone:496](../../../crates/mir-client/tests/round66_consumer_boundary.rs#L496)、
[waiter:9850](../../../crates/mir-client/src/net.rs#L9850)、
[模型:41](../../../crates/mir-client/tests/support/round66_watermark.rs#L41)。

G1.1：min/0/单成员/重复成员、五 Err、混合非法输入的全集合优先级。
Err 时真实表的身份、total、reclaimed、len 不变；不是生产注册系统。
G1.2：真实71种子源7/目标1..5，真实125 e1..e4，慢读S2、快读S4。
min2→真实 ack 后 `(2,2,4)`，保留[3,4]；追加e5→`(2,3,5)`。
慢UI新增[3,4,5]/出生cue3；重复sync新增view/cue均0。
G1.3：现状单UI ack4→`(4,0,4)`，追加e5→`(4,1,5)`。
慢UI仅新增[5]/cue1，水位仍到5；差分1对3，旧S4仍有1..4。
G1.4：floor2/tail4，晚加入0/旧代/空集均拒绝且裁剪调用0；合法2/4→2。
G1.5：预送S1/S2，同Receiver命中S2；随后try_recv=None、commands=Empty。
有限2秒timeout、无线程/睡眠；不证明并发或fan-out。
G1.6：四行矩阵与范围检查；net仅尾部cfg(test)+138，无生产/wire/server改动。
UI时钟冻结1000ms，先drain初始cue并记view基数，只比较新增出生cue。

Crystal [GameScene.cs:4687](
../../../github/Crystal/Client/MirScenes/GameScene.cs#L4687)
的空源返回、410/570和出生/命中音是表现锚点；
[PlayerObject.cs:3546](
../../../github/Crystal/Client/MirObjects/PlayerObject.cs#L3546)
仅支持TargetPoint回退、对象当前位置优先，不提供容量/LRU依据。
模型是本项目提案，不能归因于Crystal已有全局水位。

PARTIAL：注册原子性/退出/补发/背压、容量/LRU、服务端回收、饱和恢复、
素材与真实可听/Live，以及历史原编号台账，详见[交付说明](README.md)。
禁称覆盖生产多消费者、全局水位、服务端回收三项的实现完成断言，
以及任何R66最终验收断言；门禁PASS只证明记录的功能HEAD。
