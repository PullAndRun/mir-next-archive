# R59 终审报告（D6 / 刑部 xingbu）

- **结论：PASS-WITH-PARTIAL（放行；附 1 条 MINOR 必改，仅文档行号，不阻断代码/门禁判定）**
- 审计对象：`HEAD=edaa5cdf54d659cac4484ba250feb83478184485`（`origin/main` 同点）
- 交付边界：`7939133`（code 批 10 文件 +2840/−2）→ `edaa5cd`（docs 批 6 文件 +246/−8）
- 子单：`JJC-20261026-006-sub-d4af3456`；口径：**最小闭环 + PARTIAL**，禁称「125 全链」「渲染完整」「R59 完整」「G1–G7 全绿」「与 Crystal 逐字一致」「真实可听已证」。
- 方法：只读审计。未改 crates/docs 既有文件，未跑 `cargo`/`ci.ps1`，未 commit，未跑 ignored 测试；依据为提交树、门禁日志、D1–D5b/Lead 日志与源码/Crystal 源码逐行对照。

## 总表

| 项 | 判定 | 一句话依据 |
|---|---|---|
| A 门禁真实性 | **PASS** | meta 的 bytes/sha256/head/exit/起止/capture 与日志实测量全部一致；12 项逐项 OK；日志含 R59 专属测试；连接串密码已打码 |
| B 提交树绑定 | **PASS** | `git show HEAD:<path> \| sha256sum` 16/16 MATCH（含 `analyze_usage.py` 的 blob 66809 B / `8c6c8cae…`）；两提交 diffstat 与边界一致 |
| C G1/G2 与 Crystal 对齐 | **PASS** | 游标消费、无 `mem::take`/clone、未知 source/destination 处理、duration/count/dir16/index/offset 逐式一致；两处偏离已登记 |
| D G3 命中 570 | **PASS** | 570..579×60ms=600ms；dead/未知不播命中与 `+2`；每帧跟随目标；16 项 render 测试覆盖正负分支 |
| E G4 音效 | **PASS（逻辑）＋PARTIAL（真实可听）** | base 34、20341/20342、`M34-1/-2.wav`；fake sink 1/1；真实音频通道未接，文档已诚实标 PARTIAL |
| F 素材 106 帧 | **PASS** | 106 PNG、0 张 0×0；index 106/106、missing=[]、skipped_empty=0；两个 negative control 均按预期 FAIL(exit 1)；asset 测试缺素材 panic 不 SKIP |
| G 假绿/捷径扫描 | **PASS** | 无新增 `#[allow]`/`#[ignore]`/`cfg(feature)`；新测试无中文断言；run-checks 有 0-tests/门槛守卫；仅 R58 索引 HEAD 指针 1 行更新；新增文档无正面过度声明 |
| H 入库卫生与台账 | **PASS**（附 M-1） | 16 文件均 <1 MiB；树内无 target/resource/game-assets/.env；两份 44/45MB JSON 未入库；§3 台账内容与代码一致，但行号引用漂移（M-1） |

## A 门禁真实性 — PASS

- `target/round59/gate/attempt-1.meta.txt` 全部字段：`attempt=1`、`head=edaa5cdf54d659cac4484ba250feb83478184485`、`branch=main`、`start_utc=2026-10-26T09:24:22Z`、`end_utc=2026-10-26T09:27:11Z`、`exit=0`、`log_bytes=204916`、`log_sha256=0d96e0610a7a9ec34d42523cf38ffeaa9bb7742093d9e6e2f544966069ab9721`、`command=pwsh -NoProfile -Command "& ./tools/dev/ci.ps1 -KillStale *> target/round59/gate/attempt-1.log"`、`capture=full(*>)`。
- 实测：`wc -c` = 204916；`sha256sum` = `0d96e061…9721`；日志 mtime `2026-10-26 17:27:11.511 +0800`，meta mtime `17:27:11.729 +0800`，与 `end_utc=09:27:11Z` 吻合。
- 末行 `==> 全部通过（12 项）`；12 条 `OK`（fmt/clippy/duo_smoke/workspace test 136s/proto/docs links/TextKey/hardcode 70/hardcode 0/旧资源格式/物品图标/PG 串行）全部 exit 0；`FAIL` 计数 0。
- R59 专属证据：日志 843–870 行含 `round59_projectile_assets` **6 passed**、`round59_projectile_render` **16 passed**（16 个测试名逐个可见），不是旧轮冒名。
- 连接串：全日志仅 1 条 `postgres://postgres:***@nas:3001/mir2_test`，密码已打码；无 `PASSWORD=`/token/secret 明文。
- 提交时序：docs 提交 `edaa5cd` 时间 17:24:07，门禁 start 17:24:22，门禁确实覆盖 docs 批后的 HEAD。
- 观察（非阻断）：日志本身不打印 HEAD sha，HEAD 由 meta 绑定；`^OK` 计数 13 是因为 3333 行有一条无关的 `OK 所有保留物品模板都有图标`，12 项汇总仍是 3397–3408。

## B 提交树绑定 — PASS

- 以 `git diff --name-only 3cb1df1..HEAD` 的 16 文件逐一对 `git show HEAD:<path> | sha256sum`：**16/16 MATCH**，且字节数与 `target/round59/commit-verify/01-tree-hashes.txt` 一致（最大 `ui/mod.rs` 220172 B）。
- 关键值：`projectile.rs` 37167 B `556f48ed…`；`round59_projectile_render.rs` 22769 B `9235a633…`；`round59_projectile_assets.rs` 19447 B `2cfff50c…`；`sound.rs` 26439 B `eafbdf23…`；`client.md` 163030 B `f69eeabd…`；`analyze_usage.py` 66809 B `8c6c8cae…`（完整 16 项见 `final-manifest.json`）。
- 两提交边界与 diffstat：`7939133` 10 files +2840/−2；`edaa5cd` 6 files +246/−8；合计 16 文件 +3086/−10。
- 工作树仅 2 个未跟踪历史冻结 JSON：`player-resource-freeze-before.json` 44,844,165 B、`player-resource-freeze.json` 45,219,533 B；无其它脏文件。
- 观察（非阻断）：`01-tree-hashes.txt` 在审计期间被更新（mtime 17:29:27），当前自身 sha256 `3ba2fff63c06cb2bb06d5c5016c5c8ccb13463b5d48d3ebe0a602f4287c5e78e`（1858 B）；当前版本是 blob sha、16/16 可复现，建议后续把清单自身哈希一并冻结。

## C G1/G2 与 Crystal 对齐 — PASS

- **消费语义**：`Projectiles::sync`（`crates/mir-client/src/ui/projectile.rs:422-439`）按 `consumed` 位置游标推进；`consume_pending`（`:446-462`）用 `while self.consumed < events.len()`，事件表缩短时游标归零；全文件无对 `projectile_events` 的 `mem::take`/clone（`:444-457` 注释明确禁止）。未知 `source` 在 `spawn`（`:465-497`）直接 return；未知 `destination` 同样 return，不生成 view、不播出生音（D-R59-1）。
- **公式逐式核对（`crates/mir-client/src/ui/projectile.rs`）**：
  - `duration_ms`（`:98-100`）= `max_distance * 50`（`CELL_FLIGHT_MS=50`，`:73`）；`max_distance`（`:92-94`）= 切比雪夫，对齐 `Functions.cs:312`。
  - `frame_count`（`:104-108`）= `max(1, duration/30)`；`FRAME_INTERVAL_MS=30`（`:71`）。
  - `direction16`（`:122-150`）逐句移植 `GameScene.cs:12044-12065`：`a=(c.X,0)`、`bc=ac=Distance(c,b)`、`b.Y-=c.Y; c.Y+=bc; b.Y+=bc`、余弦定理、`c.X<b.X` 时 `360-angle`、`+11.25`、`/22.5` 截断；零距离固定 0（Crystal NaN 未定义）。
  - `sprite_index`（`:163-165`）= `410 + frame%6 + dir16.min(15)*10`；`BASE_INDEX=410`、`FRAME_COUNT=6`、`DIRECTION_STRIDE=10`（`:65-69`），值域 410..=565，结构上排除 416–419/566–569。
  - `draw_offset`（`:186-194`）= `Δ*(48,32)*frame/count`，`frame` clamp 到 `count`，C# 整数向零截断语义一致。
- **Crystal 对照（clone `0e315fe3`）**：`GameScene.cs:4697` 出生音 `20000+34*10+1`；`:4699` 无条件 `CreateProjectile(410, Magic, true, 6, 30, 4, targetID: destination)`；`:4701` 才按 `missile.Target != null` 挂 Complete；`:4708` `Effect(570,10,600,target)`；`:4709` `+2`；`:12044-12065` Direction16；`Effect.cs:185-186` `Duration=MaxDistance*50`、`Count=Duration/Interval`；`:201` index；`:211` offset；`PlayerObject.cs:3552` duration。公式无自创。
- **已登记偏离**：D-R59-1（无落点不生成，Crystal 无条件生成并兜底 TargetPoint）；P-3 冻结时长（Crystal `Process` 每帧重算）。另有实现层浮点路径差异：`direction16` 的 `bc` 以 f64 计算后落 f32，C# 为 float 中间量 + double sqrt；整数格点抽样与现有射程未触发 22.5° 边界翻转，不构成公式偏离。

## D G3 命中 570 — PASS

- `HIT_EFFECT_BASE_INDEX=570`、`HIT_EFFECT_FRAME_COUNT=10`、`HIT_EFFECT_DURATION_MS=600`、`HIT_EFFECT_FRAME_MS=60`（`projectile.rs:79-85`）；`hit_effect_index`（`:175-179`）= `570 + (elapsed/60)%10`，600ms 后 `finished` 删除（`:281-284`、`:541-548`）。
- `advance`（`:505-537`）到达后经 `hit_target_position`（`:349-360`）解析目标：目标未知或 `dead` 返回 None → 不生成 HitEffect、不播 `M34-2`；目标存活 → 生成 `HitEffectView` 并播命中音。`advance_hit_effects`（`:541-548`）每帧用 `snapshot.object_position(target_id)` 更新位置，实现跟随。
- 测试真实覆盖（`round59_projectile_render` 16 passed）：`hit_effect_plays_all_ten_magic_frames_when_the_target_survives`（570..579×60ms、600ms 移除）、`hit_effect_follows_the_surviving_target`（跟随移动）、`dead_target_skips_impact_and_hit_sound`（无 570、无 `M34-2`）、`unknown_destination_event_is_consumed_without_spawning_a_view`（零 view/零音效）、`views_follow_the_target_current_location_but_keep_the_frozen_duration`。
- 观察（非阻断）：D4 未做“先红后绿”的变异运行，覆盖为负例 + 白盒断言；负例足以证伪“dead 也播命中/未知也生成”的假绿路径。

## E G4 音效 — PASS（逻辑）＋PARTIAL（真实可听）

- `crates/mir-render/src/sound.rs:234` `PROJECTILE_SOUND_BASE=34`；`:236` `20_000+34*10+1=20341`；`:238` `20342`；`ProjectileSoundPhase`（`:242-266`）Spawn/Hit；`projectile_sound`（`:276-281`）仅 `Spell::FireBounce` 返回 `Some(phase.file())`，文件名为 `M34-1.wav`/`M34-2.wav`（`:259-265`）。未用 154 拼 `M154`；测试 `projectile_sound_uses_the_great_fire_ball_base_m34` 显式断言 `≠M154-1.wav` 且 `FireBall→None`。
- fake sink：`spawn_and_hit_sounds_use_m34_once_each` 断言出生 1 次 `M34-1.wav`、命中 1 次 `M34-2.wav`，重放不重复；`production_sound_cues_are_drained_once_per_projectile` 断言每投射物各消费一次。`mir-render --lib sound` 9 passed（`target/round59/d2/attempt-1/04-tests.log:27,38`）。
- 素材：`game-assets/audio/sfx/M34-{0,1,2}.mna` 存在，`M34-1.mna` 29,614 B、`M34-2.mna` 21,807 B；`audio verify` → `files=106 bytes=4.25MB`、exit 0（`target/round59/d4/attempt-1/08-audio-verify.log`）。
- **真实可听 PARTIAL 属实**：UI 只持 `&WorldSnapshot`，`take_sound_cues()`（`projectile.rs:413`）未接 `app.rs` 音频通道；README G4/P-5、client.md §E 均明确写 PARTIAL，未把 cue 断言冒充可听。

## F 素材 106 帧 — PASS

- `tools/assets/analyze_usage.py:327`：`MAGIC_FRAMES = [i for d in range(16) for i in range(410 + d*10, 416 + d*10)] + list(range(570, 580))`，即 16×6+10=106，显式排除 416–419/566–569；注释与 D3 negative control 均说明空帧原因。
- 实测 `game-assets/gfx/sprites/effect/magic`：106 张 PNG、0 张 0×0、0 字节 0；目录 107 文件（106 PNG + index.json）。index：`written_png=106`、`missing=[]`、`skipped_empty=0`、`indices_out_of_range=0`、`source_bytes=6275819`、`source_sha256=3b8a2dba…ab25`。
- D3 独立复算：`frame_check.py` PASS 0 失败，帧集指纹 `e465026a50ee6f86d6762e3959386fa0132ffabf63da9c92afeebdf3d52a1e98`；`wzx_vs_png.py` 裸解析 wzx 4010 项、与 PNG/index 逐字段 PASS；负面对照：旧范围 10≠106（exit 1）、裸 range 410–565 96≠106（13 项失败、exit 1），证明检查不是恒真。
- `round59_projectile_assets.rs` 6 passed，`read_or_panic` 缺文件直接 panic（`:89-95`），无 SKIP/`#[ignore]`；覆盖 index 清单、逐 PNG sha+IHDR、四帧抽样、帧集指纹、`magic.wzl` 源哈希、内嵌 FIPS SHA-256 自检。

## G 假绿/捷径扫描 — PASS

- 新增 Rust 无 `#[allow]`、无 `#[ignore]`、无 `cfg(feature)`（仅正常 `#[cfg(test)]` 测试模块）；R59 两个测试文件无 `#[ignore]`。
- 断言语言：新测试全英文；全仓 sound.rs 仅有既存地图名中文断言（`music_for_map`），非新增、非 UI 硬编码。i18n：Lead `12-i18n.log` 66 处（33 文件），阈值 70，两次扫描 66/0，exit 0。
- `docs/testing/round59/run-checks.sh`：9 项，`ZERO_TESTS_RE='running 0 tests|test result: ok\. 0 passed'`（:245），发现数门槛/缺测试文件 FAIL 不 SKIP（:295-315、:356），D4 `04-script-run.log` 9/9 PASS、总 EXIT=0。
- R58 目录：本轮仅 `docs/testing/round58/README.md` 第 3 行 HEAD 指针 `1a9aeb3→2f3ab9f`（1 行，事实更正，文档批已声明），未改任何 R58 日志/JSON 证据。
- 文档过度声明：新增行只有“禁称/不得”约束；未新增「125 全链」「渲染完整」「R59 完整」「G1–G7 全绿」正面断言。`docs/client.md:1390` 的「460 用例全绿」是 R7 既存历史文本，不在本轮 diff。

## H 入库卫生与 PARTIAL 台账 — PASS（附 M-1）

- 16 文件全部 <1 MiB（最大 `ui/mod.rs` 220172 B）；树内无 `target/`、`resource/`、`game-assets/`，无 `.env`/密钥（`deploy/mir2.env.example` 为既存模板，非本轮新增）；两份 44/45 MB 冻结 JSON 未跟踪。
- README §3 台账内容与代码一致：D-R59-1（无落点不生成/不播 +2）、P-2（`ProjectileEvent` 无 ID，`net.rs:1503` 结构、`EFFECT_EVENT_LIMIT=16` 于 `net.rs:166`）、P-3（冻结 duration/count）、P-4（draw 不叠加逐帧 offset）、P-5（可听 PARTIAL）、P-6（无 Missile/Effect 类）、P-7（无 Hero/真 socket）。
- 唯一文档缺陷：**行号引用漂移**，见 M-1；内容结论本身成立。

## Findings

### 必改（MINOR，文档；不阻断代码/门禁判定，但应在 R59 收口/尚书验收前修正）

- **M-1 行号引用与 Crystal 快照不一致**：
  - `docs/testing/round59/README.md:21` 写 `ui/projectile.rs:240-301`；实际 `sync` 在 `:422-439`、游标消费在 `:446-462`（`:240-301` 落在 `HitEffectView`/绘制区）。
  - `docs/testing/round59/README.md:58`、`docs/client.md:1198/1202/1265` 写 `projectile.rs:273-284`；实际 `ProjectileView` 字段在 `:207-246`、spawn 在 `:465-497`，`:273-284` 是 `HitEffectView::sprite_index/finished`。
  - 对 `github/Crystal` clone `0e315fe3`：`Effect.cs:174-181` 实为 Process `172-191`、`Duration/Count` 在 `185-186`；`Effect.cs:196` 实为 `201`；`Effect.cs:206-212` 实为 `211`；`PlayerObject.cs:3546-3547` 实为 `3552`。`GameScene.cs:4697/4699/4701/4708/4709/12044-12065` 则逐字命中。
  - 建议：保留公式与测试证据，仅把上述引用改为实际行号；“逐字一致”仍不得作为整体宣称（P-3/D-R59-1 仍偏离）。

### 非阻断

- **N-1** 门禁日志不打印 HEAD sha，仅 meta 绑定；建议 `ci.ps1` 在日志头打印 `git rev-parse HEAD`。
- **N-2** D4 无“先红后绿”变异运行；现有负例（dead/未知/无落点）+ 16 项白盒断言可证伪主要假绿路径，仍建议 R60 加一次 mutation/negative harness。
- **N-3** Lead `02-lib-sound.log` 首跑 7 passed/1 failed，失败为 `settings::tests::old_settings_files_without_sound_fields_still_load` 的临时目录 `PermissionDenied`（`settings.rs:335`，`Os { code: 5 }`），重跑 `02-lib-sound-retry.log` 8 passed；属已知环境问题，非本单代码失败。D2 的 mir-render 4 failed 同因（entities/text/tiles）。
- **N-4** `01-tree-hashes.txt` 审计期间更新（mtime 17:29:27）；当前 16/16 正确，建议将清单自身 sha 写入 D6 封存记录。
- **N-5** R58 索引 HEAD 指针在 docs 提交中改动 1 行（事实更正，已声明）；不属于“手改 R58 证据数据”。

## 提交建议

1. **接受** `7939133` + `edaa5cd` 作为 R59 交付边界；`HEAD=edaa5cd` 与 `origin/main` 同点。
2. **不重写历史**（已推送）：M-1 用后续 docs-only commit 修正行号；修正后再对外引用交付结论。
3. 门禁/证据：保持 `gate/attempt-1.log` 与 `01-tree-hashes.txt` 的当前哈希；R60 起把 HEAD 写入门禁日志头、把证据清单自身哈希纳入清单。
4. 对外只允许表述：**「R59 最小闭环 + 已知偏离/PARTIAL；G4 真实可听 PARTIAL；D-R59-1 非对齐」**。

## 证据哈希

- `target/round59/gate/attempt-1.log`：204,916 B，`sha256=0d96e0610a7a9ec34d42523cf38ffeaa9bb7742093d9e6e2f544966069ab9721`。
- `target/round59/commit-verify/01-tree-hashes.txt`：1,858 B，`sha256=3ba2fff63c06cb2bb06d5c5016c5c8ccb13463b5d48d3ebe0a602f4287c5e78e`。
- 16 文件 bytes+sha256 见 `docs/testing/round59/final-manifest.json`；关键：`projectile.rs` 37167 B `556f48ed…`、`round59_projectile_render.rs` 22769 B `9235a633…`、`round59_projectile_assets.rs` 19447 B `2cfff50c…`、`sound.rs` 26439 B `eafbdf23…`。
- D4/Lead 日志计数：render 16、assets 6、mir-render sound 9、wire 7、server magic 43、mir-client lib projectile 8、lib sound 8；run-checks 9/9。
