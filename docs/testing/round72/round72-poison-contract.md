# R72 149 PoisonCloud 有限映射契约（pre-audit）

依据[获批计划](../../process/round72-plan.md)、[门下条件](../../process/round72-menxia-review.md)及[实现](../../../crates/mir-client/src/ui/spellfx.rs#L295-L315)；功能HEAD `2a31554`；D文档已提交（`d8b6157`），清单固化（`90a7516`），待独立终审后验收。

| 项 | 本轮语义/映射 | 边界 |
| --- | --- | --- |
| 准入 | `spell == Spell::PoisonCloud && event.from_object_spell`，仅149来源 | 非149不迁移 |
| 时间 | `age=now_ms.saturating_sub(event.started_ms)`；`0<=age<4000` | AREA_MS产品瞬态窗，不是Crystal对象寿命 |
| 相位 | `phase=(age/120)%20`；Magic2 `1650+phase` | 20帧elapsed循环，允许跳帧，非严格NextMotion |
| 寻址 | `FxKind::Swing`复用Magic2/start0 | 不改Swing公共语义；Bolt会额外+20 |
| 锚点 | `world_to_screen(center,event.location)`格原点 | 不按帧宽高居中，不复制格子 |
| 绘制 | `draw_effect_blend(FxKind::Swing, index, x, y, true)` | 当帧signed offset一次，先offset后裁剪 |
| 混合 | 复用R70 rate1 CPU Add | rate0.8及 `src*0.8*alpha` 未实施 |
| 事件 | 每149事件一张当前帧；缺帧/缺图库跳本事件 | 不退0/相邻帧、不吞后续事件；原色块/文字保留 |
| 不变量 | param两值/八方向不改映射 | 非149、1620起手、R71火墙与其他技能路径不变 |

| age(ms) | phase | Magic2索引 |
| --- | --- | --- |
| 0–119 | 0 | 1650 |
| 120–239 | 1 | 1651 |
| 2280–2399 | 19 | 1669 |
| 2400–2519 | 0 | 1650 |
| 2520 | 1 | 1651 |
| 3999 | 13 | 1663 |
| -1 / 4000 | 不绘制 | 无 |

## 证据与放行边界
- [九个测试](../../../crates/mir-client/tests/round72_poison_cloud_mapping.rs)：真实149编解码链、整帧oracle、边界/锚点/offset/Add/旧来源/缺帧/param方向；[C报告](round72-c-evidence.md)记录独立Python/Rust expected/生产actual三源一致，不代表真实素材视觉。
- [B补证](../../../target/round72/b/attempt-2/link.md) 撤回attempt-1「一一对应」；M2 Bolt红 `:939:5` (384,256) `[48,136,176,255]` vs `[0,0,0,255]`；M7去来源红 `:1491:5` (381,229) `[248,4,0,255]` vs `[0,0,0,255]`；B M4精确居中仍近似残余。
- [Lead裁定](../../process/round72-lead-ruling.md)两项必改CLOSED；[C2](round72-c-addendum.md) PASS-WITH-PARTIAL/必改0；[门禁meta](../../../target/round72/gate/attempt-1.meta.txt)记录HEAD `2a31554`、EXIT0、12OK/0FAIL，官方clippy不带legacy。

## 原号PARTIAL（不能写成完成）
- R71时钟/对象：严格NextMotion、初始100ms取整、每Process一步、持久场/26移除与对象寿命；4000ms仅产品瞬态窗。
- GAP-1/2 / R70-C2：rate0.8、`src*0.8*alpha`、DrawColour/INVLIGHT/完整DrawBlend；本轮rate1 CPU近似不证明Crystal等价。
- GAP-3/4/6：Light3、mask/tint/shadow、场景条件offset/AnimationOffset；GAP-5 / R70-C1其他26技能/1620/其他调用点；R70-C3全局尺寸居中。
- B plan-M4精确写法残余、legacy clippy既有lint债、真实素材/GPU视觉签核未做；R71-C7及更早历史原号继续保留。完整交付SHA与时间差异见[索引](README.md)，本契约不作全量或视觉等价签核。
