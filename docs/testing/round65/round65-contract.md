# R65 本地契约：投射物事件消费与快照边界

## 1. 范围与语义

本文件记录本仓库既有本地契约与 R65 回归口径；G1 是单 UI 生命周期补证，不定义新的生产策略。G2（`known_positions` 与事件表容量政策）整项移出本轮。

- **代数不变量**：`total == len + reclaimed()`。`total` 是累计计数，不因前缀裁剪而减少；饱和计数仅证明不溢出，不能外推恢复、全局唯一序号或跨消费者安全。
- **绝对水位**：对于剩余事件向量的零基下标 `i`，其 1-based 事件序号/消费水位为 `reclaimed() + i + 1`。水位表示已确认事件条数并包含本条；`reclaimed()+i` 只能称零基偏移，不能称 1-based 水位。
- **前缀裁剪**：只删除事件序号 `<= through` 的已确认前缀。重复、后退或已回收范围内的确认不改变状态；`through` 超过现有尾部时夹到当前可裁剪长度。代号不匹配的旧代确认不裁剪新代事件。裁剪不减少 `total`。
- **ack 生成**：快照同步后，只有非零且严格前进的水位才生成本地 ack；零、重复或后退值不生成 ack，也不推进已确认位置。该本地 ack 不扩展 wire 协议。
- **clone**：`WorldSnapshot`/事件向量的 clone 是值副本；权威事件表裁剪后，已存在的 clone 内容保持不变。clone 不注册消费者，也不钉住权威前缀；离线读取旧 clone 不证明生产多消费者可安全共享历史。
- **等待器**：`wait_for_snapshot` 按 predicate 等待并返回符合条件的快照，不自动 ack；它不登记为全局最低水位成员。它与 app 的快照消费点从同一 Receiver 竞争，故不得把等待器描述成第二个已支持的并发消费者。

## 2. 本轮排除项与增长风险

- `known_positions` 与事件表容量政策 G2 整项移出本轮：不增容量配置，不实现运行时 LRU、驱逐或容量常数，不据此改写 roadmap/client。
- 默认仍为无界增长；未确认 backlog 不丢最旧是待评审契约，不是已实现容量策略。clone 同样可能延长旧值存活，但不阻止权威表裁剪。
- 全局最低水位、多消费者/并发消费语义、慢消费者退出或租约、容量与 LRU、饱和恢复、服务端回收、素材/音频仍为 PARTIAL。
- B 的饱和用例为构造边界，直接写 `projectile_events_total = u64::MAX` 后再验证 125 wire push；不得描述为 wire 可生成该累计值。

## 3. Crystal 对照锚点

下列路径以仓库内 `github/Crystal/` 为根；对照仅陈述可见客户端行为，不外推仓库级水位、容量或回收策略。

| Crystal 锚点 | 对照事实与边界 |
|---|---|
| [Client/MirObjects/Effect.cs:144,191–195](../../../github/Crystal/Client/MirObjects/Effect.cs#L144) | Missiles 表反映活动效果；构造加入、移除时删除，是活动生命周期，不是消费注册表或永久历史。 |
| [Client/MirScenes/GameScene.cs:4687](../../../github/Crystal/Client/MirScenes/GameScene.cs#L4687) | 投射物查找源对象；源缺失即返回；出生音在生成分支；目标存在且未死亡才进入命中特效/音效分支。 |
| [Client/MirObjects/PlayerObject.cs:750,966–967,3546](../../../github/Crystal/Client/MirObjects/PlayerObject.cs#L750) | `SkipFrames` 依赖非自机与 `ActionFeed.Count > 0`；动作队列 FIFO 出队；目标当前位置优先、缓存点回退不构成全局容量或 LRU 依据。 |
| [Client/MirObjects/MonsterObject.cs:346,528–529](../../../github/Crystal/Client/MirObjects/MonsterObject.cs#L346) | `SkipFrames` 依赖 `ActionFeed.Count > 1`；队列 FIFO 出队；不证明本项目生产 ActionFeed 已接通。 |
| [Shared/ServerPackets.cs:3564](../../../github/Crystal/Shared/ServerPackets.cs#L3564) | 125 包字段为 Spell、Source、Destination；没有确认 ack 或序号字段。 |

## 4. 禁称与阶段口径

- 禁止使用“已限容”“有界”“多消费者已支持”“全链已闭”等正面能力表述；也不得将单 UI 回归、clone 离线对照、饱和直写或局部门禁外推为全局能力。
- 只可如实陈述：本地单 UI 生命周期有回归证据；多消费者/全局水位、容量/LRU、服务端回收及素材/音频仍 PARTIAL。
- 唯一门禁 EXIT=0 与刑部独立核验 PASS-WITH-PARTIAL（必改 0）均不代替尚书验收、门下确认或看板终态；本轮不得表述为已最终通过或收口。
