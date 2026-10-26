# 第 61 轮（R61）验收索引：125 积压不丢、动态飞行参数与逐 cue 离线混音

> 代码提交① `ef317b0`「feat(client): R61 125 积压不丢、动态飞行参数与逐 cue 离线混音（第 61 轮）」；8 文件，+2216/−232。
> 本文只记录已落盘事实；终审仍由 D4/尚书/门下完成，不把定向证据写成整体终审 PASS。

## 1. 目标与事实锚点

| 目标 | 当前状态 | 实现/测试/证据 |
|---|---|---|
| G1 125 积压不丢 | 已落盘，长期回收 PARTIAL | `net.rs`/`ui/projectile.rs` append-only；`round61_projectile_queue` 11 passed；D1 `00-summary.md` |
| G2 动态飞行参数 | 已落盘，完整时钟仍 PARTIAL | 每帧目标后重算 duration/count；`ui::projectile` 13 passed；D1 `00-summary.md` |
| G3 125 逐 cue 入口 | 已落盘 | 专用入口绕过 100ms，普通入口不变；`round61_projectile_sound` 8 passed；D2 `00-summary.md` |
| G4 离线真实混音 | 已落盘，设备/Live PARTIAL | M34-1/2 真实 decoder→amplify→rodio mixer；D2 `03-sample-energy.md` |
| G5 文档/MF | 已落盘待抽核 | MF-1 当前注释位于 `app.rs:3800` 附近；MF-2 修正本文件相关历史引用 |
| G6 脚本/证据 | 本文件与脚本执行中 | `run-checks.sh`、树哈希、`target/round61/d3/attempt-1/` |

Crystal 事实锚点：`github/Crystal/Client/MirObjects/Effect.cs:81-100,185-187` 的严格 NextFrame 与每帧 Duration/Count；`github/Crystal/Client/MirSounds/SoundManager.cs:94-105` 每次创建 OneShotProvider；`github/Crystal/Client/MirScenes/GameScene.cs:4697-4709` 的 FireBounce 出生/命中分支。

## 2. M1–M6 闭合表

| 项 | 闭合事实 | 仍需保留的边界 |
|---|---|---|
| M1 | 17/33/257 两条消费路径均覆盖；输入/保存/view/cue/命中均为 N，重复 sync/drain 不新增；len/cap 为 17/32、33/64、257/512 | append-only 无回收；snapshot clone 随总量增长；后续需确认游标、安全裁剪边界、会话复位、落后消费者不丢不重 |
| M2 | 6→9 格 450/15，移至 3 格 150/5，重合 0/1；目标更新后重算 | 完整 NextFrame/SkipFrames、TargetPoint、像素偏移增量仍 PARTIAL |
| M3 | 125 专用入口同刻 2/33 均提交；普通 play/play_event/play_spell 的 0/99/100ms 契约保留；交错不污染 | 不按 M34 文件名全局豁免 |
| M4 | M34-1/2 走真实解码、增益、离线 rodio mixer；gain 0.25 RMS/能量比 0.25/0.0625，3 叠加比 3/9 | 无声卡真实可听/Live 未验证；33 叠加 peak=22.844788，可能削波 |
| M5 | `gate_head=ef317b0`；门禁 attempt-1 12/12、EXIT=0；日志字节与 SHA 已登记；清单自身 SHA 不列自身 | commit② 文档候选以生成时点标注；D4/Lead 仍需提交后回验 |
| M6 | MF-1 注释措辞已修；client 历史 P-6→P-4 | 不改 R60 冻结计数/哈希 |

## 3. 门禁与测试事实

唯一门禁（Lead 已执行）：`pwsh -NoProfile -File tools/dev/ci.ps1 -KillStale *> target/round61/gate/attempt-1.log`，HEAD `ef317b0`，EXIT=0，12 项全 OK；日志 207,593 B，SHA256 `6ff5f68dd112020e20d42ddf8d6f835be7099dab999e1a411d1b9b4cff77ad96`，UTC 2026-10-26T12:06:56Z–12:09:29Z。原始文件为 `target/round61/gate/attempt-1.log` 与 `.meta.txt`。

D1/D2 已落盘计数：queue 11、sound 8、`ui::projectile` 13、`net::` 31、mir-client lib 307、bin 23、R59 render 16、R58 wire 7、mir-render sound 9、mir-client audio 21；各 EXIT=0。红→绿：D1 queue 首次 5 passed/6 failed，D2 red-01/red-03 EXIT=101，最终均为绿。

## 4. 内存口径与 PARTIAL 台账

只报告 Vec `len/capacity`，不是进程 RSS：17→17/32，33→33/64，257→257/512；257 不是生产上限。append-only 无回收和 snapshot clone 成本留待后续回收契约；完整 NextFrame/SkipFrames、TargetPoint、像素偏移增量仍未实现。真实设备可听与 Live 未验证；33 同刻叠加不归一化且 peak=22.844788；105 个 sfx 未逐一验证；`HiddenSpanSource` 只覆盖本 crate 的 `decoder_for` 路径；普通 CWD/素材来源边界仍保留。

其它 PARTIAL：缺素材只能 SKIP→PARTIAL；无会话代号的旧游标歧义；R60 P-1..P-6 不自动清零。术语禁用：不得把本文写成「125 全链」「渲染完整」「真实可听已证」「R61 完整」「全链」「无上限安全」。

## 5. 复跑

```bash
bash docs/testing/round61/run-checks.sh --list
bash docs/testing/round61/run-checks.sh
py -3 tools/docs/check_links.py
git diff --check
bash docs/testing/round61/gen-tree-hashes.sh
```

脚本固定 `TMP/TEMP/TMPDIR=target/tmp`；`--list` 不执行测试、不写证据。测试发现数为 0、实际 `running 0 tests`、`0 passed`、任意 `SKIP:`/`PARTIAL:` 或任一 EXIT 非 0 均使汇总 EXIT=1。门禁脚本不由 D3 重跑。

## 6. 证据路径

- D1：`target/round61/d1/attempt-1/00-summary.md`、`d1-contract.txt` 及逐命令日志。
- D2：`target/round61/d2/attempt-1/00-summary.md`、`03-sample-energy.md` 及逐命令日志。
- D3：`target/round61/d3/attempt-1/` 下 `00-summary.md`、逐命令 log/EXIT、tree-hashes 日志、链接检查与差异检查。
- 门禁：`target/round61/gate/attempt-1.log`；只引用，不改写。
