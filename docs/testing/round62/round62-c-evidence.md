# Round 62 C — 刑部独立核验与审计证据

- 子任务：`JJC-20261026-009-sub-7c491d88`（刑部，父 `JJC-20261026-009`）
- 执行成员：`xingbu-r62-c`（deepseek）
- 仓库：`C:\Users\diana\Documents\github\mir-next`，HEAD `1a6004d1ebf53904b22f9003a52e26187c2e359c`
- 核验时间：2026-10-26 21:53–22:15（Asia/Shanghai），时间预算 30 分钟
- 权限边界：本轮**只读实现代码、只写证据/审计文件**；未 commit/push，未跑 `ci.ps1`，未改 `crates/**`、R61 证据/冻结 JSON，未新增 `#[allow]`/`#[ignore]`。
- 结论：**PASS-WITH-PARTIAL**（C1–C5 门禁与语义均通过；3 个 PARTIAL 见 §7，均有证据与影响面，不构成 BLOCK）

---

## 1. 独立重跑（不信任报告）

环境：`TEMP=TMP=repo/target/tmp`，日志目录 `target/round62/r62-c/attempt-1/`，命令与 `timings.txt` 同步记录。

| # | 命令 | exit | 耗时 | 实测计数 | 日志 |
|---|------|------|------|----------|------|
| G1 | `cargo test -p mir-client --tests` | 0 | 43s | **44 targets，558 passed / 0 failed / 7 ignored** | `all-tests.log` |
| G2 | `cargo test -p mir-client --lib ui::projectile::tests::` | 0 | 1s | **18 passed / 0 failed**（295 filtered） | `lib-projectile.log` |
| G3 | `cargo test -p mir-client --lib` | 0 | 1s | **313 passed / 0 failed** | `lib-all.log` |
| G4 | `cargo clippy -p mir-client --all-targets -- -D warnings` | 0 | 1s | 仅 `Finished dev profile`，无 warning/diagnostic | `clippy.log` |
| G5 | `cargo fmt --check` | 0 | 3s | 无输出（0 字节） | `fmt.log` |

分 target 计数（G1 中按 `Running tests\<name>.rs` → `test result:` 对齐）：

| target | 计数 | 来源行 |
|---|---|---|
| `tests\round59_projectile_render` | 16 passed / 0 failed | `all-tests.log:714` 起 |
| `tests\round61_projectile_queue` | 11 passed / 0 failed | `all-tests.log:736` 起 |
| `tests\round62_projectile_wire`（新增） | 5 passed / 0 failed | `all-tests.log:767` 起 |
| `unittests src\lib.rs` | 313 passed / 0 failed | `lib-all.log` 尾 |
| `unittests src\main.rs` | 23 passed / 0 failed | `all-tests.log` 内 |
| 其余 39 个 target | 合计 190 passed / 0 failed | 同上 |

**与 B 报告计数对账**：44/558/0、lib 313、bin 23、round59 16、round61 11、round62 5、lib ui::projectile 18、clippy/fmt exit0 —— **全部一致**，bin/lib 无差异需解释。唯一事实偏差：B 报告称「另 2 个历史 `#[ignore]`」，实测 `ignored=7`（跨 6 个 target：`attack_variant.rs:528`、`attack_variant_live.rs:817`、`cli_monster_frames.rs:133`/`:336`、`monster_motion.rs:274`、`remote_motion.rs:907`、`remote_motion_live.rs:337`）。7 个均为 HEAD 已存在、本轮未新增（见 §2），属**报告口径偏差（P1）**，不影响 558/0/44 与验收结论。

复现命令：

```bash
cd C:/Users/diana/Documents/github/mir-next
export TEMP=$(pwd)/target/tmp TMP=$(pwd)/target/tmp
cargo test -p mir-client --tests > target/round62/r62-c/attempt-1/all-tests.log 2>&1
cargo test -p mir-client --lib ui::projectile::tests:: > target/round62/r62-c/attempt-1/lib-projectile.log 2>&1
cargo test -p mir-client --lib > target/round62/r62-c/attempt-1/lib-all.log 2>&1
cargo clippy -p mir-client --all-targets -- -D warnings > target/round62/r62-c/attempt-1/clippy.log 2>&1
cargo fmt --check > target/round62/r62-c/attempt-1/fmt.log 2>&1
```

---

## 2. 改动边界与红线（`git diff` 实测）

`git status --porcelain` 实测（仅声明集合）：

```
 M crates/mir-client/src/app.rs
 M crates/mir-client/src/net.rs
 M crates/mir-client/src/ui/projectile.rs
 M crates/mir-client/tests/round59_projectile_render.rs
 M crates/mir-client/tests/round61_projectile_queue.rs
?? crates/mir-client/tests/round62_projectile_wire.rs
?? docs/process/round62-*.md
?? target/...（构建产物，忽略）
```

`git diff --numstat`（实测）：

| 文件 | + | − | 行数 | SHA-256 |
|---|---|---|---|---|
| `crates/mir-client/src/app.rs` | 13 | 6 | 4165 | `b06b48c3…` |
| `crates/mir-client/src/net.rs` | 62 | 1 | 9098 | `9bf15a26…` |
| `crates/mir-client/src/ui/projectile.rs` | 637 | 184 | 1676 | `ca8e4a47…` |
| `tests/round59_projectile_render.rs` | 153 | 27 | 761 | `dd555c1f…` |
| `tests/round61_projectile_queue.rs` | 108 | 30 | 722 | `9ed77bfb…` |
| `tests/round62_projectile_wire.rs`（新） | 529 行 | — | 529 | `adc92a53…` |

红线核验结果：

| 红线 | 方法 | 结果 |
|---|---|---|
| 仅动声明文件（B 范围） | `git status --porcelain` | ✅ 仅 `app.rs`/`net.rs`/`projectile.rs`/两个旧测试 + 新 round62 测试 |
| B 未把 promise 改成实现（C1） | 读 `projectile.rs` `advance_clock` | ✅ 真实实现严格 `NextFrame`（见 §5） |
| 无新增 `#[allow]`/`#[ignore]` | `git diff -U0 \| grep -E '^\+.*#\[(ignore|allow)'` | ✅ 无匹配（rc=1）；新 round62 文件亦无 |
| `app.rs` 只改测试 | `git diff` hunk 均位于 `mod tests`（HEAD `app.rs:3410`）之后（`:3691`、`:4113`） | ✅ 生产代码未动 |
| wire 包字段不变 | `git diff --stat crates/mir-protocol` 为空；读 `packets/server.rs:3905-3913` | ✅ `ObjectProjectile { Spell, Source, Destination }`，`INDEX=125`，编码顺序 byte/u32/u32 未变 |
| `session_generation` 不进序列化 | `grep -rn session_generation crates/mir-protocol` 无命中；`WorldSnapshot` 仅 `#[derive(Debug, Clone)]`（`net.rs:2719`），无 serde；`net.rs:2987` 注释明确「既不是 wire 字段」 | ✅ |
| 两冻结 JSON 未动 | `git status` 无 M；mtime 均为 2026-10-24（早于本轮） | ✅ |
| R61 证据/文档未动 | `git status --porcelain docs/testing/round61 docs/testing/round62` 空；`git diff --stat docs/` 空 | ✅ |
| `git diff --check` | 空白错误检查 | ✅ rc=0 |
| 新测试文件 ≤1MiB | `round62_projectile_wire.rs` 21,821 B | ✅ |

附注（报告纪律）：A 报告 §1 写 `projectile.rs +657/−184（共 825 行）`，实测 `+637/−184`（821 行变动）；B 报告引用 `+637/−184` 正确。属 **A 报告数字笔误（P0 无关）**，不影响实现与验收。

---

## 3. SHA-256 对账（独立 `sha256sum`）

| 文件 | B 报告声称（新） | 独立实测（新） | HEAD 旧值（实测） | 判定 |
|---|---|---|---|---|
| `tests/round59_projectile_render.rs` | `dd555c1f…` | `dd555c1fa0f29a40bda0fac7d45730ada9afbca8c4e9c6e1807724c0e21fc31c` | `99889b63…` | ✅ 一致 |
| `tests/round61_projectile_queue.rs` | `9ed77bfb…` | `9ed77bfbd29eacbba2ff8c48059d24f2120166fa7ec1213591cbcb61a26203b6` | `91389124…` | ✅ 一致 |
| `tests/round62_projectile_wire.rs` | `adc92a53…` | `adc92a53…` | 新增（无 HEAD） | ✅ 一致 |
| `src/net.rs` | `9bf15a26…` | `9bf15a26…` | `15e670e1…`（A 的中间态） | ✅ 一致 |
| `src/app.rs` | `b06b48c3…` | `b06b48c3…` | HEAD 基线 | ✅ 一致 |
| `src/ui/projectile.rs` | 仍 A 的 `ca8e4a47…` | `ca8e4a47fe485eb92f5ac18e069a1ffc5f46ea9e98a707c55c6cc2ac373eb8d1` | HEAD `c83279b1…` | ✅ B 未改实现，与宣称一致 |

B 报告声明「round59/round61 旧 SHA 为 HEAD 换行/空格归一后的值」亦与 `git show HEAD:…` 实测一致（`99889b63…`、`91389124…`）。R62 manifest 的 SHA 重登记需按上述 6 值更新（P2，未在本轮写范围）。

---

## 4. A 红证据真实性（C5，只读核验）

文件：`target/round62/r62-a/attempt-1/red-baseline.log`（1006 B）、`red-r59-elapsed-evidence-superseded.log`（3622 B）。

**4.1 `red-baseline.log`（新 R62 C1 红）—— 真实断言失败，非假绿/非编译失败冒充**

```
running 1 test
test effect_clock_catch_up_on_jump_keeps_flight_alive ... FAILED
assertion `left == right` failed: one tick after a 29 s jump must still be in flight
  left: 0   right: 1
  at crates/mir-client/src/ui/projectile.rs:1240
test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 307 filtered out
EXIT=101
```

- 编译先 `Finished` 成功、随后 `running 1 test`，**是测试执行阶段的断言失败**（非编译错误、非 `0 tests`）。
- 红对象是 A 的旧实现 `projectile.rs:1240`（elapsed 公式在 29 s 大跳时 0 帧、`must still be in flight` 失败），证明实现语义确实被改变。
- 同一断言在 B 实现后转绿：当前 `projectile.rs:1658` 测试 `r62_clock_advances_at_most_one_frame_per_tick_not_elapsed_catch_up`，`:1668` 断言消息逐字为 `one tick after a 29 s jump must still be in flight`；G2 中 18/0 通过。

**4.2 `red-r59-elapsed-evidence-superseded.log`（R59 6 个旧语义测试红）**

- `running 16 tests`，**10 passed / 6 failed**，`EXIT=101`；6 个失败均为真实断言/panic：
  - `:290` `assert_eq!(frame, 4)` ← left `1`：旧 elapsed 公式期望 +150 ms 直接 frame 4，新严格每 tick 一帧得 1；
  - `:238` `assert_eq!(events_consumed, 0)` ← left `1`：旧「空表即复位游标」语义被 C4 代号语义替代；
  - `:429` panic `arrived projectile must be removed`（单次大跳不再到场）；
  - `:484` index out of bounds（len 0）；
  - `:552` 命中 cue left `[]` vs right `["M34-2.wav"]`；
  - `:632` 同类 cue 差异。
- 6 个测试名与 B 报告 §对照的表完全一致；对应改写后的测试在 G1 中 16/0 全绿。
- **判定：红证据真实**（有编译成功、有真实断言差异、有非零退出码、有同断言转绿对照），C5 通过。

---

## 5. Crystal 原件逐行对照（`github/Crystal`，本机 clone）

行号为本机文件实际行号（`awk` 编号实测），锚点区间与派发 §二 一致。

| # | Crystal 原件（行号 + 关键代码） | 本项目实现（行号 + 关键代码） | 判定 |
|---|---|---|---|
| 5.1 时钟 | `Client/MirObjects/Effect.cs:81` `if (CMain.Time <= NextFrame) return;`；`:83` `if (Owner != null && Owner.SkipFrames) CurrentFrame++;`；`:85` `if (++CurrentFrame >= Count)`；`:96` `else NextFrame = Start + (Duration / Count) * (CurrentFrame + 1);` | `ui/projectile.rs:319-338` `advance_clock`：`if now_ms <= self.next_frame_ms \|\| now_ms <= self.last_tick_ms { return false; }`；skip 先 +1，再普通 +1；`>= frame_count` 才 true；`next_frame_after`（`:149-152`）= `start + interval*(frame+1)` | ✅ 严格 `<=` 返回、到期后至多 +1、大跳不追补、边界取整 `Duration/Count` 一致 |
| 5.2 动态参数 | `Effect.cs:176` `if (Target != null) Destination = Target.CurrentLocation;`；`:185` `Duration = Functions.MaxDistance(Source, Destination) * 50;`；`:186` `Count = Duration / Interval;`；`:187` `if (Count == 0) Count = 1;`；`:189` `base.Process();` | `projectile.rs:737-784` `advance`：先逐 view 用 `object_position` 刷新 destination（缺失保留旧值 + 写 `known_positions` 缓存），再重算 `duration_ms`/`frame_count`，最后 `advance_clock`；`advance` 只在 `advance_clock` 返回 true（越过 NextFrame）时结算/删除 | ✅ 顺序一致：缓存刷新 → 重算 → 时钟；参数收缩后 `finished()` 可为真但删除仍等下一 tick 越过 NextFrame（与 `Effect.cs:81` 先行 return 一致） |
| 5.3 客户端缓存语义 | `Client/MirObjects/PlayerObject.cs:3546` `MapObject ob = MapControl.GetObject(targetID);`；`:3548` `var targetPoint = TargetPoint;`；`:3550` `if (ob != null) targetPoint = ob.CurrentLocation;`；`:3552` `duration = MaxDistance(CurrentLocation, targetPoint) * 50;`；`:3554` `new Missile(..., targetPoint) { Target = ob }` | `projectile.rs:694-703` spawn：destination = `object_position(snapshot, dest_id)`，否则 `known_positions.get(dest_id)` 兜底；仍无 → **return，不生成实体、不播出生音、不改 wire**；`known_positions` 由 `consume`（`:629-632`）与 `advance`（`:756`、`:772`、`:797`）按对象 ID 缓存最后已知位置 | ✅ 优先对象当前位置，缓存兜底；未知目标绝不造假坐标 |
| 5.4 125 三点式 | `Shared/ServerPackets.cs:3564-3578` `public Spell Spell; public uint Source; public uint Destination;` 读 `ReadByte`/`ReadUInt32`/`ReadUInt32`，写同序 | `crates/mir-protocol/src/packets/server.rs:3905-3913` 同三字段 + `INDEX 125`；编码/解码顺序 byte/u32/u32 未改；round62 测试经 `through_wire()`（`:106-114`）走真实 `encode_frame`/`decode_server_packet` | ✅ 包结构/编号未变，无点坐标、无 `session_generation` |
| 5.5 效果管理 | `Client/MirScenes/GameScene.cs:4687` `ObjectProjectile`；`:4697` 出生音 `PlaySound(20000 + GreatFireBall*10 + 1)`；`:4699` `source.CreateProjectile(410, Libraries.Magic, true, 6, 30, 4, targetID: p.Destination)`；`:4701-4711` `Complete`：`:4707` dead 目标 return，`:4708` `Effects.Add(new Effect(Libraries.Magic, 570, 10, 600, Target))`，`:4709` 命中音 `+2` | `projectile.rs:683-724` spawn（source 无坐标即 return；出生音在 push view 前 `:706-709`）；`:737-783` advance 结算时 `hit_target_position`（`:468-478`，dead 返回 None）存活才 `HitEffectView`（570，10 帧，600 ms）+ 命中音；`HitEffectView:370-398`；`advance_hit_effects:788-800` 到期删除 | ✅ 三态（无源/无目标/存活/死亡）音效与 570 残影节奏一致，play 一次队列 `:576-588` |

补充实测：`frame_at`（`:162-169`）仅作历史公式保留、生产路径不调用（`grep` 生产调用点仅定义与 round59 测试）；`frame_interval_ms` 取整除法与 Crystal `Duration / Count` 一致。

---

## 6. 13+2 改写测试抽查（旧断言 → 新断言，5 例 + 汇总）

B 报告称「round59 / round61 只换驱动（逐 tick），未删断言/覆盖面」——抽查如下：

| 例 | 旧（HEAD） | 新（工作树） | 判定 |
|---|---|---|---|
| E1 C4 游标（R59 `cursor_resets…`） | `round59:226` 测试；`:236-238` 重建空表后 `assert_eq!(events_consumed(), 0)`（空表即复位） | `round59:243`；新增同代号裁剪空表**不**复位（`:266` 消息 `a trimmed empty table of the same generation must not rewind the cursor`）、新代空首批**不提前**复位（`:299` 消息）、新代非空首批恰复位一次并清旧状态；旧「重播不重复播放」断言保留 | ✅ 断言数 126→141，语义替换有依据（C4 决定），非删断言 |
| E2 时钟边界（R59 `flight_duration…`） | `round59:253`；`:285-290` 单次 `sync(+150)` → `frame==4`、sprite 断言；`+248` 在飞 / `+249` 删除 | `round59:336`；`:376-378` 同刻 `+31` 不推进；`:382-387` **单次 +150 大跳只到 frame 1**（R62 C1）；`:389-392` 逐 tick +151/+152/+153 到 frame 4 + sprite 断言；`:400-414` 再 1 s 大跳只 +1 帧、`+2000/+2001/+2002` 逐帧到 7、`+2003` 删除（保留 `arrived projectile must be removed` 原文） | ✅ 纯公式断言（`frame_at`/`frame_interval_ms`）原样保留；驱动改为逐 tick，覆盖不降反增 |
| E3 R61 五格边界 | `round61:550` `five_grid…`；旧用 `+155` 一次跳到 frame 4、`+249` 删除 | `round61:613`；`:628-629` `+31` 相等不推进；`:630-644` 表驱动 `(32,1)…(218,7)`「每 tick 恰过一个边界」；`+249` 结算 | ✅ 覆盖 8 个边界全部逐个验证，旧边界值全覆盖 |
| E4 R61 动态参数 | `round61:437` `dynamic_flight…`；旧 `+200` 单次 → frame 6、`+400` 单次 → frame 13 且近移同 tick 结算 | `round61:482`；`:527-548` 逐 tick 到 `+400` 后仅 `finished()==true`、`:549-558` `+421` 下一 tick 才结算；近移不瞬移、不提前补播 | ✅ 语义按 C1 收紧（删除等 NextFrame），未放宽 |
| E5 命中残影（R59 `hit_effect_plays_all_ten…`） | `round59:413`；旧单次 `sync(+100)` 驱动；断言 570..=579 全 10 帧、600 ms 删除 | `round59:528`；改用公共 helper `settle_two_cell_flight`（`:128`）逐 tick 驱动；`:560-578` 仍断言帧序列恰为 `570..=579`、`:580-589` 600 ms 后删除 | ✅ 断言原样（10 帧序列 + 删除时机），仅换驱动 |

汇总实测：`assert` 宏计数 round59 旧 126 → 新 141；round61 旧 124 → 新 125；round62 新增 112。**无断言被删**，新增集中在 C1/C4 的新语义边界。R59 的 6 个改写测试与 A 红日志（§4.2）一一对应，即改写由「旧语义在新实现下真实失败」驱动，不是为绿而改。

---

## 7. C1–C5 逐条结论

| 项 | 结论 | 依据（摘要） | PARTIAL/风险 |
|---|---|---|---|
| **C1 严格 NextFrame 每 tick 至多一帧** | **PASS** | §5.1/§5.2；`advance_clock:319-338` 严格 `<=` return、skip+1/普通+1、`>=` 才结束、大跳不追补；`last_tick_ms` 同刻幂等；固定预期表测试 `projectile.rs:1030` 起（5 格=250ms/8 帧/31ms） | `owner_skip_frames` 生产来源固定 false（P3，R59 已登记）；570 残影仍按 elapsed 公式与 C1 时钟解耦（`HitEffectView` 规格固定 10 帧/600 ms，与 Crystal `new Effect(..., 10, 600)` 一致，非削弱） |
| **C2 动态目标/缓存兜底、不造假坐标** | **PASS** | §5.2/§5.3；unknown 目标不生成、不 play、不改 wire；缓存由对象位置刷新；round62 测试 ③ `:362`（未知目标不生成、重现不补播）+ R59 `:205/:221` 未知源/目标测试全绿 | `known_positions` 无容量上限（P3：按对象 ID 只增，会话复位清空；同会话内对象数有界，暂不影响正确性，R63 建议加 LRU/上限或复用 views） |
| **C3 落点偏移只叠加一次** | **PASS** | `draw_location:348-354` 只加一次 offset；唯一生产调用 `:811`；单测 `:1300-1338`「一次调用==连续两次、不累积」；G2 18/0 | 无 |
| **C4 代号回收/复位契约** | **PASS** | `consume:619-666`：同代号空表/裁剪不重置（`:634-641`），重建同代号（`dropped==0 && len>0 && consumed>len`）才复位游标；新代号空首批 `len==0 && total==0` 直接 return（不提前复位）；新代号首批非空 `reset_for_new_generation`（`:669-675`）清 views/hit_effects/known_positions/cursor 恰一次；事件绝对游标 + `dropped` 头跳只前进不回放（不存在「丢最旧冒充回收」骗过消费的口径：`cursor=max(consumed,dropped)` 后按绝对语义消费）；`sync:576-588` 新代号清旧 cue 留新 cue；round62 测试 ② `:259` 五段断言全绿、R59 `:243` 绿 | 事件表**物理内存回收**无协议（R61 起 append-only，P3，A 报告已登记）；新代空快照「暂不复位」依赖 `len==0 && total==0`，如果生产端发「新代 len0 但 total>0」会走复位分支（当前 net.rs 新会话 `total=0`，不可达） |
| **C5 独立复跑 + 红/绿证据真实** | **PASS** | §1（44/558/0、18/0、313/0、clippy/fmt exit0）；§4 红证据为真实断言失败且有非零退出码与同断言转绿对照；`git diff --check` rc=0；链接检查见 §8 | **P1**：B 报告「另 2 个历史 ignore」实为 7 个（既有、无新增，报告口径需更正） |

**总判定：PASS-WITH-PARTIAL。** 三个 PARTIAL：
- **P1（报告口径，必须更正）**：B 报告 ignored 计数 2→7；`docs/process`/manifest 若不改，后续轮次会继续错。
- **P2（流程未完项）**：R62 manifest 需登记本报告 §3 的 6 个 SHA 并注明 supersede R61 manifest（B 报告 §七.2 已声明不在其写范围，属 Lead/中书收口事项，非代码缺陷）。
- **P3（实现承接，R63 建议）**：`owner_skip_frames` 生产来源缺口；事件表物理回收无协议（内存只增）；`known_positions` 无上限；570 残影仍用 elapsed 公式（与规格一致，仅提示 C1 未覆盖该时钟）。

---

## 8. 遗留风险与 R63 建议

1. **R62 manifest 未登记新 SHA/supersede**（P2）→ Lead 在中书阶段按 §3 六值登记；否则 R61→R62 的「新语义 superseded」链不完整。
2. **事件表内存只增**（P3，R61 起登记）→ R63：服务端协议加 `recycle/trim` 语义（带 `dropped` 与代号），`dropped` 路径已有测试覆盖（`dropped > 0` 时消费不乱序），只需协议侧接线。
3. **`known_positions` 无上限**（P3 新登记）→ R63：按会话代号分区 + 对象 ID 上限（如 4×views 容量）或随 views 生命周期回收。
4. **SkipFrames 生产来源缺口**（P3，R59 起登记）→ R63：Crystal `ActionFeed`（`PlayerObject.cs:750`、`MonsterObject.cs:346`）→ 本地技能动作状态透传进快照，替换 `owner_skip_frames` 常量 false。
5. **新代空快照语义边界**（低风险）→ 若未来服务端可能发「新代号 + `projectile_events_total>0` 但 `len==0`」，把 `consume` 的提前 return 条件从 `len==0 && total==0` 收紧为「新代号尚未见过非空表」的显式状态，避免误复位；当前 net.rs `new_session()` 下不可达。
6. **投递风险**：R62 改动尚未提交，B 报告 SHA 均以工作树计算——提交前需再跑一次 `sha256sum` 对账（本报告 §3 值可直接复算）。

---

## 9. 原始证据清单

`target/round62/r62-c/attempt-1/`：

- `all-tests.log`（G1，44 target 完整输出）
- `lib-projectile.log`（G2，18 passed/0 failed）
- `lib-all.log`（G3，313 passed/0 failed）
- `clippy.log`（G4，仅 Finished 行）
- `fmt.log`（G5，0 字节）
- `timings.txt`（exit/secs/cmd 记录）

只读引用：`target/round62/r62-a/attempt-1/red-baseline.log`、`target/round62/r62-a/attempt-1/red-r59-elapsed-evidence-superseded.log`；`target/round62/r62-a/report.md`、`target/round62/r62-b/report.md`；`docs/process/round62-shangshu-dispatch.md`、`docs/process/round62-plan.md`、`docs/process/round62-menxia-review.md`。
