# R70 混合偏移生产入口契约

本批 pre-audit；功能已提交、唯一门禁通过，独立终审待办。范围是 CPU Add 子集；[证据索引](README.md)。

## API 与共同实现

实际公开签名为 `draw_effect_blend(&mut self, frame: &mut Frame, kind: FxKind, index: u32, x: i32, y: i32, apply_offset: bool) -> bool`。
这里的 offset 指当前帧 `ImageInfo.offset_x/offset_y`，没有额外的外来 offset 参数。
[EntityRenderer:1874](../../../crates/mir-render/src/entities.rs#L1874) 与旧 `EntityRenderer::draw_effect_with_offset` 共用 [draw_effect_impl:1893](../../../crates/mir-render/src/entities.rs#L1893) 生产实现；新入口选择 Add，旧入口选择 Over。

|入口|apply_offset / additive|坐标与混合|
|---|---|---|
|draw_effect|false / false|绝对左上角，旧 source-over|
|draw_effect_with_offset|true / false|当帧offset一次，旧 source-over|
|draw_effect_blend|显式bool / true|可选当帧offset，Add|

先 ensure_image；缺帧 false。true 时读取当帧signed i16元数据，分别 `x.saturating_add(i32::from(info.offset_x))` / y同理，恰好一次，再交裁剪。
false 时保持绝对 `(x,y)`；有图完全屏外仍 true。缓存命中不克隆 Image、不新增逐帧分配、不改metadata、不跨帧累积。
Add 经 `blit_cached_add` 调既有 `Frame::blit_add`；RGB=`min(255,dst+round(src*alpha/255))`，alpha=0不变，否则目标alpha=255；rate固定1，不新增rate参数。
旧 NORMAL/Over入口保留原像素；Crystal NORMAL 的混合分支是 SourceAlpha/One，并非旧 Over；两种含义不能混称等价。

## 调用点与 R68

[MapScene:452](../../../crates/mir-render/src/scene.rs#L452) 同签名薄转发；[R70测试](../../../crates/mir-client/tests/round70_effect_blend_offset.rs) 直接调用真实 EntityRenderer/MapScene入口，以合成WZL/WZX解码及独立整帧RGBA oracle验收。
生产API已经落地；spellfx/UI/事件消费者迁移未做，不能称玩家已见完整混合动效。
R68 已接125飞行/570命中旧with_offset，仍走Over；运动偏移由调用方加一次，图像偏移由渲染入口加一次；两套R68各2例在B/C2回归通过。

## Crystal 与边界

[MLibrary:692–711](../../../github/Crystal/Client/MirGraphics/MLibrary.cs#L692) 偏移在裁剪前、默认false/rate1；[DXManager:353–389](../../../github/Crystal/Client/MirGraphics/DXManager.cs#L353) NORMAL=SourceAlpha/One、INVLIGHT另有模式。
[Effect:201–216](../../../github/Crystal/Client/MirObjects/Effect.cs#L201) 逐帧/运动位置后显式true；本轮只复用定位契约，不改magic映射/帧号/播放时序。
非 wire 功能；协议/server零改动。完整DrawBlend/rate/INVLIGHT/状态恢复、shadow/mask、GameScene条件、spellfx居中与全UI迁移仍PARTIAL。
ThunderBolt/Bolt/FireWall映射差异、真实素材视觉签核、GPU/设备与完整混合动效矩阵另案；本轮CPU合成像素不为其背书。
