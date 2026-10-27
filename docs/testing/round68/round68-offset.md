# R68 逐帧图像偏移契约

任务 JJC-20261027-005；功能提交 `c9e94c4`；门禁基线 `6ba5d45`。
阶段：D 文档批 + 待独立终审；PASS-WITH-PARTIAL 候选。

## 显式入口

`EntityRenderer` 与 `MapScene` 提供相同参数的入口：

```rust
pub fn draw_effect_with_offset(&mut self, frame: &mut Frame, kind: FxKind, index: u32, x: i32, y: i32) -> bool
```

[EntityRenderer:1847](../../../crates/mir-render/src/entities.rs#L1847) 按 `kind.library()` 与 `index + kind.start_frame()` 读取当前帧。
取得 `cached_info` 后，分别计算 `x.saturating_add(i32::from(info.offset_x))` 与 `y.saturating_add(i32::from(info.offset_y))`，再 `blit_cached`；偏移为有符号 i16，最终由 Frame 裁剪。
缺帧返回 false，不画占位；有效图像完全屏外仍返回 true。返回值表示帧存在性。
不改缓存 meta 或调用方坐标；不跨帧累积；运动量由调用方加一次，图像偏移由新入口加一次。
[MapScene:433](../../../crates/mir-render/src/scene.rs#L433) 转发；[ProjectileState:927](../../../crates/mir-client/src/ui/projectile.rs#L927) 飞行与 :932 命中调用新入口。
[旧 draw_effect:1816](../../../crates/mir-render/src/entities.rs#L1816) 保持绝对左上角语义；spellfx 的尺寸居中与旧入口保持原状。

## Crystal 锚点

- [MirGraphics/MLibrary.cs:880–890](../../../github/Crystal/Client/MirGraphics/MLibrary.cs#L880)：ReadInt16 读取 X/Y；[同文件:658–670](../../../github/Crystal/Client/MirGraphics/MLibrary.cs#L658)：先 point.Offset，再裁剪。
- [MirObjects/Effect.cs:112–132](../../../github/Crystal/Client/MirObjects/Effect.cs#L112)：Owner.DrawLocation 或地图投影后以 offSet=true 绘制；[同文件:201–216](../../../github/Crystal/Client/MirObjects/Effect.cs#L201)：Missile 逐帧索引与运动量之后使用图像偏移。
- [MirScenes/GameScene.cs:4687](../../../github/Crystal/Client/MirScenes/GameScene.cs#L4687)：125 投射物消费场景锚点。

## G1–G4 用例映射

| 关项 | 最终用例与断言 |
|---|---|
| G1 | [wire 测试](../../../crates/mir-client/tests/round68_image_offset_wire.rs)：`r68_g1_synthetic_wzl_signed_offsets_pixels_are_consumed`，真实 LibrarySource 读合成 WZL，逐帧 signed offset、整帧像素、重复绘制不累积。 |
| G2 | [wire 测试](../../../crates/mir-client/tests/round68_image_offset_wire.rs)：`r68_g2_legacy_draw_effect_ignores_offset_and_crop_semantics`，新入口零偏移/部分负裁剪/全屏外/缺帧/i32 极端，旧入口非零偏移仍落原点。 |
| G3 | [live 测试](../../../crates/mir-client/tests/round68_effect_offset_live.rs)：`r68_g3_real_125_flight_frames_apply_per_frame_offsets`，71/125 encode/decode→apply→UiState.sync→公开 draw；450→451，1032 时独立运动量(30,0)，飞行重复 draw 与换 center 的整帧 RGBA 比较。 |
| G4 | [live 测试](../../../crates/mir-client/tests/round68_effect_offset_live.rs)：`r68_g4_real_570_hit_per_tick_advance_consumes_offsets`，按 1032…1249 到达，1309 等号不推进、1310 进 571；覆盖 570–579，不用 elapsed helper 代替。 |

真红与 SHA 绑定见 [索引](README.md) 和 [C 最终核验](round68-c-final.md)。用例名以最终测试源码为准；绿命令启用 `--features mir-render/legacy`。

## 边界

夹具为合成 WZL/WZX，协议与渲染消费使用真实链路；测试名中的 real 指真实调用链。
这些证据不构成真实素材视觉签核。仅登记 R61 P04 的 125/570 图像偏移消费子项；全 UI offset、spellfx 居中迁移、DrawBlend/shadow/mask、素材/设备及 R49–R67 历史遗留继续 PARTIAL。
生产 wire/server 零改动；新增 ignore/allow 为 0，历史 ignore 7 不变。R69 目标留待独立规划。
