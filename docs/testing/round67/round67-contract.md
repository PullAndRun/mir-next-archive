# R67 测试私有淘汰提案契约

阶段 **pre-audit**；功能 HEAD=`78a8613`；本项目待裁决测试提案，不进入生产或玩法。生产 known_positions 仍无界无淘汰。证据和最终五行矩阵见[本轮索引](README.md)。

## 类型与入口

接口位于 [integration-test support](../../../crates/mir-client/tests/support/round67_positions.rs#L24)，policy 与 boundary 两个测试target各自 path 引入，不导出到库。

```rust
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct EvictionPlan {
    pub evict: Vec<u32>,
    pub remaining: usize,
    pub over_budget: usize,
}
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum PlanError { DuplicateId }
pub(crate) fn plan_evictions(
    entries: &[(u32, u64)],
    protected: &[u32],
    budget: usize,
) -> Result<EvictionPlan, PlanError>
```

- entries 是唯一 `(id,last_used_tick)` 集合；先检查整个集合，任意重复 id 返回 `Err(PlanError::DuplicateId)`，即使该 id 受保护也 fail-closed；只读输入不变。
- protected 为显式测试假设；重复/未知 id 无副作用，不增加条目。未实现生产引用收集、touch或时钟。
- 候选只含未保护条目，按 `(tick,id)` 升序。删除最少前缀直到 `remaining <= budget` 或候选耗尽；evict 按此顺序输出id。
- `remaining = entries.len() - evict.len()`；`over_budget = remaining.saturating_sub(budget)`。保护项超额允许 remaining 大于 budget，不以预算丢弃保护项。
- budget=0 仍保留受保护条目；budget≥len 不删；相同tick按id确定顺序，输入排列不影响结果。这里的稳定指结果确定，不要求排序实现稳定性。
- 预算与tick均为显式测试输入，没有生产默认容量；不造坐标、不裁未确认事件。

## G1.1–G1.4 实际口径

|项|输入与断言|限制|
|---|---|---|
|G1.1|[(10,1),(20,2),(30,3),(40,4)]、保护10、预算2→[20,30]/2/0；反序同值；并列tick预算1→淘汰较小id10|仅参考候选|
|G1.2|全保护3预算2→[]/3/1；预算0仅保护10→[20,30]/1/1；预算≥len、未知/重复保护、重复entry错误且输入不变|保护可超额，不承诺硬内存上限|
|G1.3|真实71预热源7与目标1..4；显式保护7/1、预算2；完整历史与候选历史接收同序125 [1,2,3,4,999]→view四身份/四坐标/cue4对[1]/(11,10)/cue1；consumed5与重复0|同代0 default夹具、历史筛选为测试模拟；非生产淘汰|
|G1.4|目标11经真实71更新19，隐藏后125回退19；新代首批非空清旧缓存→view/cue0、游标1；本代13→cue1，重复0|隐藏实体和代号赋值非wire；generation不等于map epoch|

实际测试名见[四测试清单](README.md#四个测试实际名称)；计划G1.3/G1.4的原拟名以实际源码名称为准，断言口径按门下修订。

## M1–M6 落实

|要求|本轮落实与证据|
|---|---|
|M1|WorldSnapshot::default 同代0历史夹具，断言UI/stream代号0、consumed5；未用new_session导致首125清预热；真71/125编解码与非wire夹具分列|
|M2|单换当前/cache优先可能被cache_visible_entities先刷新与advance覆写而幸存，不计红；隔离删generation clear与禁可见刷新分别真实UI红|
|M3|只在隔离support保护判断忽略ID1，不改测试字面量；算法推导保留[7,4]，实际UI [4]≠[1]失败；实现变异与首败索引见README|
|M4|门下预算A/B各≤1.25h、合≤2.5h硬上限3h；初核A约21min含m4/B约15min；后续A rev2摘要披露约55min，B追加工时未在D独立计量，不拿初核36min充最终总工时，交Lead最终核账|
|M5|七份rev2红分别独立源码/target/tmp、绝对cwd/manifest，编译来源red-copy，无硬链接，测试SHA与绿一致；C追加复核原件，不伪称重跑全部红|
|M6|README五行矩阵与红绿表列非wire代号、可能幸存原因/替代红、support实现变异及实际左右值；两个target各2 passed、ignored0|

## Crystal 依据与失效边界

[PlayerObject.cs:3546–3566](../../../github/Crystal/Client/MirObjects/PlayerObject.cs#L3546)：先取 TargetPoint，目标对象存在时优先 CurrentLocation，构造 Missile 并加入 Effects；**没有全局位置缓存容量、LRU或自动保护集依据**。不能从活动引用须保存落点推出原作容量常数或本提案排序政策。
[125字段](../../../github/Crystal/Shared/ServerPackets.cs#L3564)只有 Spell/Source/Destination，无点坐标。71/125包链只覆盖测试所述编解码→apply→UI；非socket、非对象Remove包闭环；cue不证明可听。
生产 [known_positions](../../../crates/mir-client/src/ui/projectile.rs#L764)可见刷新、换代清缓存；本轮未改该逻辑。map epoch、同代ID复用、自动保护集、多消费者、全局最低水位、服务端回收和真实限容接线继续 PARTIAL。保护超额与历史淘汰减少未来出生共同说明：提案不能直接当生产硬限容策略。
门禁 PASS 与 C addendum PASS 不代表最终验收/收口；历史PARTIAL按[原编号台账](README.md#partial-原编号台账与-r68-候选)承接。
