# R62 飞行时钟与回收契约

结论：**PASS-WITH-PARTIAL**。本轮范围为 125 飞行状态时钟、动态目标缓存、运动 offset 一次叠加、会话代号与消费复位，以及对应回归证据。C1–C5 全 PASS；物理回收、生产 SkipFrames 与音频/素材边界仍保留 PARTIAL。

## 提交与门禁

| 字段 | 已核实事实 |
|---|---|
| 功能 HEAD | `a7362fc`，6 文件 +1502/−248；push 待 Lead 门禁/终审后统一执行 |
| 门禁 attempt-1 | `pwsh -NoProfile -File tools/dev/ci.ps1 -KillStale`，**EXIT=0，12/12 OK**；D 未重跑该入口 |
| 原始日志 | [attempt-1.log](../../../target/round62/gate/attempt-1.log)，209,193 B |
| 日志 SHA256 | `81e8ef05131460635adaee4bf02066d4e2187a2a852573a8ae7191a23adfd9ab` |
| UTC 起止 | `2026-10-26T14:08:59Z`–`2026-10-26T14:12:54Z` |
| A 报告勘误 | projectile.rs 为 **+637/−184**（C 独立核验），不是 +657；net.rs A 阶段 +20，最终合并 +62/−1（含 new_session/发号）；app.rs +13/−6 仅测试驱动 |

## 证据目录

| 角色 | 原件与用途 |
|---|---|
| A | [报告](../../../target/round62/r62-a/report.md)、[attempt-1](../../../target/round62/r62-a/attempt-1/)：实现与红→绿 |
| B | [报告](../../../target/round62/r62-b/report.md)、[attempt-1](../../../target/round62/r62-b/attempt-1/)：真实 125 编解码/消费路径与改写测试 |
| C | [独立证据](round62-c-evidence.md)、[attempt-1](../../../target/round62/r62-c/attempt-1/)：44 targets，558 passed/0 failed/7 ignored；lib 313、bin 23、R59 16、R61 11、R62 5、projectile 18 全通过；clippy 0 诊断，fmt 无输出 |
| D | [报告](../../../target/round62/r62-d/report.md)、[attempt-1](../../../target/round62/r62-d/attempt-1/)：文档、哈希与复跑记录 |

A 的 [red-baseline.log](../../../target/round62/r62-a/attempt-1/red-baseline.log) 为真实断言失败 exit101，同断言在 projectile.rs:1668 转绿；[green-unit.log](../../../target/round62/r62-a/attempt-1/green-unit.log) 18/0、[green-lib.log](../../../target/round62/r62-a/attempt-1/green-lib.log) 312/0（TEMP 重定向后；C 在 B 补单测后复跑 313/0）。[旧 elapsed 红证据](../../../target/round62/r62-a/attempt-1/red-r59-elapsed-evidence-superseded.log) 10 passed/6 failed、exit101，保留为语义更替前的真实红。

## SHA 对账与 supersede

以下为工作树原始字节 SHA256；完整路径、提交 blob SHA256/字节/行数在 [manifest](final-manifest.json)。提交树 SHA 以 `git show a7362fc:<path> | sha256sum` 独立生成，不能以工作树 SHA 代替。

| 六文件（均位于 crates/mir-client） | 行数 | 旧 → 新工作树 SHA256 |
|---|---:|---|
| src/app.rs | 4165 | 基线 → `b06b48c3797f6233b99d6fff7407032f76e5c299784a745e23ef6c987774484b` |
| src/net.rs | 9098 | A 中间态 `15e670e1…` → `9bf15a268b1619d40149f9dcdc627189c53df9b117263b7c6575d15bc35a0ac8` |
| src/ui/projectile.rs | 1676 | A 实现保持 `ca8e4a47fe485eb92f5ac18e069a1ffc5f46ea9e98a707c55c6cc2ac373eb8d1` |
| tests/round59_projectile_render.rs | 761 | `99889b63e3e68d84e15311cfa42753d2e1b880cef597c97271a798dd806b3bbf` → `dd555c1fa0f29a40bda0fac7d45730ada9afbca8c4e9c6e1807724c0e21fc31c` |
| tests/round61_projectile_queue.rs | 722 | `91389124d4dee7c08a5d18144c571c0decbf23967c78fd14f3bbbca4be289e1d` → `9ed77bfbd29eacbba2ff8c48059d24f2120166fa7ec1213591cbcb61a26203b6` |
| tests/round62_projectile_wire.rs | 529 | 新增 → `adc92a535bc344643b7ebcb10504b060d58a1f68969563d1d52f9832c66a397b` |

两项改写测试 **supersede R61 manifest 中对应 R59/R61 条目**：旧 elapsed 大跳直达/空表即复位改为严格逐 tick/会话代号复位；保留测试名与断言覆盖。R59 assert 宏 126→141，R61 124→125；R62 新增 112。R61 冻结文档和 JSON 保持历史事实，不覆写。

## C1–C5 语义结论

| 项 | 结论与对照 |
|---|---|
| C1 PASS | Crystal Effect.cs:81/83/85/96 ↔ projectile.rs:319–338：`now <= NextFrame` 不进；skip 到期额外 +1，再普通 +1；普通 tick 至多一帧、大跳不追补、同刻幂等。Effect.cs:176/185–187/189 ↔ advance:737–784：先目标刷新→重算→时钟。 |
| C2 PASS | PlayerObject.cs:3546–3556 ↔ spawn:694–703：对象当前位置优先，known_positions 最后可信位置兜底；两者皆无则不生成、不播出生音、不改 wire。ServerPackets.cs:3564–3578 ↔ mir-protocol server.rs:3905–3913，125 仍只有 Spell/Source/Destination，无点坐标。 |
| C3 PASS | draw_location:348–354 与唯一生产调用 :811：运动 offset 只叠加一次；不等同 ImageInfo.offset 修复。 |
| C4 PASS | consume:619–666：同代号裁剪空表不复位；新代号空首批延后，首批非空恰一次清旧 view/570/cache/cursor/cue；绝对游标跳过 dropped，不重放。物理回收协议仍 PARTIAL。 |
| C5 PASS | 独立复跑与真实红→绿成立；GameScene.cs:4687–4710 ↔ spawn/570/HitEffectView，出生/存活命中 cue 恰一次；死/未知目标不生成命中效果。 |

## 复跑

```bash
bash docs/testing/round62/run-checks.sh --list
bash docs/testing/round62/run-checks.sh
py -3 tools/docs/check_links.py
git diff --check
python -c "import json;json.load(open('docs/testing/round62/final-manifest.json'))"
bash docs/testing/round62/gen-tree-hashes.sh
```

[run-checks.sh](run-checks.sh) 顺序执行 tests/lib/projectile/clippy/fmt/links/diff 七项，记录原始 exit、PASS/FAIL/SKIP 和耗时；SKIP 导致总 exit 非零。它设置可写 TEMP/TMP 到 target/tmp；不会调用唯一 CI 入口。[gen-tree-hashes.sh](gen-tree-hashes.sh) 默认 stdout，参数指定输出文件；[commit-tree-hashes.txt](commit-tree-hashes.txt) 六源码用提交 blob，四份待提交文档用工作树原字节，显式区分口径。manifest 自身 SHA 仅写在该清单，避免自引用；清单本身不入自身。

## PARTIAL 与 R63 承接

- **P1（报告纠正）**：B 写 2 个历史 ignored，实为 **7**，本轮无新增：attack_variant.rs:528、attack_variant_live.rs:817、cli_monster_frames.rs:133/336、monster_motion.rs:274、remote_motion.rs:907、remote_motion_live.rs:337。C 证据和本 manifest 已采用 7。
- **P2（登记已落盘，待终审核对）**：本 manifest 登记 6 新 SHA、两项 supersede，并分列 git blob/工作树口径。历史 R61 manifest 保留不变。
- **P3→R63**：①事件表物理回收仍无服务端协议；②known_positions 无上限/LRU；③owner_skip_frames 生产固定 false，Crystal PlayerObject.cs:750/MonsterObject.cs:346 ActionFeed 来源未接；④570 残影仍 elapsed 公式（10 帧/600ms），严格时钟 C1 不覆盖它。
- R61 的 **12 项 PARTIAL 逐项承接**至 [路线图](../../roadmap.md) R63 候选；R62 已证子项仅作进展，不抹去音频、素材、缓存/回收等剩余边界。
- 禁称：不得以本轮门禁、离线混音或合成协议测试宣称「125 全链已证」「真实可听」「R62 完整」。无真实设备音频/Live 新证据，105 sfx、HiddenSpanSource 全局化、CWD/素材与 R60 P-1..P-6 不自动闭合。
