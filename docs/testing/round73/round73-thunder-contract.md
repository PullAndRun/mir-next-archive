# R73 非149 ThunderBolt 命中契约（pre-audit）

- 主单 `JJC-20261027-010`；功能 `6183394`，素材修复/HEAD快照 `3378303`；PASS-WITH-PARTIAL 候选、待独立终审。
- 实现 `crates/mir-client/src/ui/spellfx.rs` SHA256 `e0bcaef0ff6a6103079e5cdb3d31f16e2a2db918b3ae390babd7290d00d3fc1b`，21,862B。
- 测试 `crates/mir-client/tests/round73_thunderbolt_hit_mapping.rs` SHA256 `1ec3f6dbbf7368e7dbd82c78529f1e6605908b04c11164ba860ab28b42513c5e`，84,910B；9测试覆盖G1 UI到G5 param/direction。

|项|限定契约与边界|
|---|---|
|准入|`event.spell == Spell::ThunderBolt && !event.from_object_spell && event.param`；120 `S.Magic` / 123 `S.ObjectMagic` 的 Cast=true 映射为param=true；149 `S.ObjectSpell` 来源为from_object_spell=true，不进入此分支。|
|反例|Cast=false/param=false、149来源、Lightning走原路径；实际包矩阵120/123/149，“148”不存在。|
|年龄|`age=now_ms.saturating_sub(event.started_ms)`；负年龄不画；本命中分支 `age>=400ms` 不画。700ms文字/色块窗与旧失败闪光另行保留。|
|帧窗|**5帧×80ms**；`phase=(age/80).min(4)`；0–79→10、80–159→11、160–239→12、240–319→13、320–399→14；不循环、不外推第6帧。|
|图库|`index=10+phase`，复用 `FxKind::Swing` 的Magic2/start0寻址，非Bolt图库。|
|坐标|`world_to_screen(center, event.location)` 取包内目标格快照；不是施法者位置，也不随目标owner移动；不做尺寸居中。|
|offset与混合|`draw_effect_blend(FxKind::Swing, …, true)`；signed offset当帧只加一次、先offset后裁剪，缺帧跳过；rate1 CPU Add饱和混合。|
|素材|`3378303` 扩MAGIC2_FRAMES为 `[0..2]+[10..14]+[20..29]`；仅重拆magic2，旧帧逐项一致；生成的game-assets仅本地、不入库，非人工视觉签核。|

## Crystal 依据及勘误

- 雷电：`Client/MirObjects/PlayerObject.cs:2956–2963` → `Effect(Magic2,10,5,400,TargetPoint/ob)`；`Effect.cs:38–61` 给出80ms间隔，`:81–96` 处理严格帧时钟，`:113–130` 区分格点与owner锚点。
- `MirGraphics/MLibrary.cs:692–709`：先叠当帧offset再裁剪；`DXManager.cs:353–384`：SourceAlpha/One混合；本切片采用rate1 CPU近似。
- `SpellObject.cs:57–64` 是R72毒云Magic2/1650/20/120ms/Light3，非雷电五帧依据；`:323–339` 是149格锚点+DrawBlend的继承参照。120/123施法或火焰墙链不作为本雷电命中帧证明。
- `spellfx.rs:309` 注释条件有误：Crystal是 `Time<=NextFrame` 时return、`Time>NextFrame` 才推进；Lead判MINOR，R74承接。

## 证据权威与 PARTIAL

- A最终同测试SHA红0/9 EXIT101 → 绿9/0/0；B五变异build0 → RED101 → 恢复绿，Python oracle63/63；[C](round73-c-evidence.md)及[补记](round73-c-addendum.md)独立复跑，PASS-WITH-PARTIAL、必改0。
- [Lead裁定](../../process/round73-lead-ruling.md)：M3仅删截止、clamp4保留，以400ms泄漏frame14真红为权威；M5仅技能条件真红为权威；B M3/M5扩展仅作辅助，15诱饵夹具偏差保留披露。
- **elapsed PARTIAL**：80ms/400ms为产品精灵窗近似；R71 strict NextMotion/初始100ms取整/每Process一步/对象寿命及149持久场/26移除/4000ms窗均未闭合。
- R73-T1严格等号/跳帧、ActionFeed/施法动画结束触发、回执起算、owner跟随、Light6与声音未迁移。
- GAP-1/2/R70-C2：rate0.8与 `src*0.8*alpha` **近似未实施**；DrawColour/INVLIGHT/完整DrawBlend/API未收敛。
- GAP-3/4/6：Light3、mask/tint/shadow、场景offset/AnimationOffset；GAP-5/R70-C1：149域其余26技能、1620/起手20/3/300ms/其他调用点；R70-C3全局尺寸/高度/owner锚点继续PARTIAL。
- R72 B plan-M4残余/legacy lint、R71-C7/E1/R69条件5及R68/R49–R70/R61–R67历史原号承接；真实素材/GPU/设备视觉、全UI/跨后端/Live音频未签核。
- Lead门禁attempt-1素材缺帧失败、attempt-2 PostgreSQL池超时失败；attempt-3 HEAD3378303 EXIT0、12/12 OK；完整尝试历史见[索引](README.md)，不据此扩大本契约范围。
