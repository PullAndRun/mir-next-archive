# R62 尚书省验收

- 验收者：gpt-6.1-sol；日期：2026-10-26（Asia/Shanghai）；只读复核，先落盘后补齐。
- **verdict：ACCEPT-WITH-PARTIAL**。放行 R62 限定交付；必改 0 项，PARTIAL 不自动关闭。
- 提交边界：`a7362fc7f93b50bfd0027dca5383e5eadd6c29bb` → `98aa9db0ee350c6b8480e50cf48be258297f1606`。

## 验收命令表

| 命令／复核动作 | 结果 | 退出码 | 证据／观察 |
|---|---|---|---|
| `git show --stat a7362fc` | PASS | 0 | 6 文件 +1502/−248；app/net/projectile 与 R59/R61/R62 测试 |
| `git show --stat 98aa9db` | PASS | 0 | 11 个 docs 文件 +821；无代码文件 |
| `git status --porcelain` | PASS | 0 | 仅本验收文件及下列两个历史冻结 JSON 未跟踪；无 tracked 变更 |
| `git diff a7362fc..98aa9db -- crates/` | PASS | 0 | 空；追加 `--exit-code` 也为 0 |
| `sha256sum target/round62/gate/attempt-1.log` | PASS | 0 | 日志哈希见下表；`wc -c`=209193，exit0 |
| `sha256sum docs/testing/round62/final-manifest.json` | PASS | 0 | manifest 哈希见下表 |
| `py -3 -c "import json;json.load(open('docs/testing/round62/final-manifest.json',encoding='utf-8'))"` | PASS | 0 | JSON 可解析，登记 6 源码 blob 与两项 supersede |
| `git show a7362fc:crates/mir-client/src/ui/projectile.rs \| sha256sum` | PASS | 0 | 与 manifest `git_blob_sha256` 一致 |
| `git show a7362fc:crates/mir-client/src/net.rs \| sha256sum` | PASS | 0 | 与 manifest `git_blob_sha256` 一致 |
| `py -3 tools/docs/check_links.py` | PASS | 0 | 1804 链接检查通过（终稿后再次核验） |
| `git diff --check` | PASS | 0 | 无输出；终稿后再次核验 |
| `bash docs/testing/round62/run-checks.sh --list` | PASS | 0 | 列出 tests/lib/projectile/clippy/fmt/links/diff 七项，未执行测试 |
| `git diff a7362fc^..98aa9db \| grep -nE '^\+.*#\[(ignore\|allow)'` | PARTIAL | 0 | 原始宽扫描非空：6 处全是 docs 引用/禁止属性文字，非新增绕过 |
| `git diff a7362fc^..98aa9db -- crates/ \| grep -nE '^\+.*#\[(ignore\|allow)'` | PASS | 1 | 无匹配；grep=1 是预期无命中，新增代码绕过=0 |
| 125 定义及 encode/decode 逐行读取 | PASS | 不适用（read） | `crates/mir-protocol/src/packets/server.rs:3903–3931`：仅 Spell/u8、Source/u32、Destination/u32 |
| 全 crates 搜索 `session_generation` 并读取 WorldSnapshot | PASS | 不适用（grep/read） | 仅客户端 net/UI/测试；protocol 无匹配；net.rs:2719 仅 Debug/Clone，无序列化派生 |

状态中的历史项为 `docs/testing/monster-native/player-resource-freeze-before.json` 与 `docs/testing/monster-native/player-resource-freeze.json`；扣除获准新增的本文件后，正好两项。
原始宽扫描命中 diff 行 2680/2703/2705/3119/3147/3195，分别是计划、门下裁决、C 报告文字；无属性声明，故不构成 BLOCK。
用户给出的协议路径简写已核正为 `crates/mir-protocol/src/packets/server.rs`；`session_generation` 位于 net.rs:2786，仅本地快照代号，不入 wire。

## 独立抽检 SHA256（原始字节，不是 Git object id）

| 对象 | SHA256 |
|---|---|
| gate attempt-1.log | `81e8ef05131460635adaee4bf02066d4e2187a2a852573a8ae7191a23adfd9ab` |
| final-manifest.json | `df70ce05fa4e2e7110cf6c46f2a7f782ce15cc97756fdb9da1822a2281029308` |
| a7362fc:crates/mir-client/src/ui/projectile.rs | `ca8e4a47fe485eb92f5ac18e069a1ffc5f46ea9e98a707c55c6cc2ac373eb8d1` |
| a7362fc:crates/mir-client/src/net.rs | `9bf15a268b1619d40149f9dcdc627189c53df9b117263b7c6575d15bc35a0ac8` |

## 门禁与已有独立证据

- PASS：`target/round62/gate/attempt-1.meta.txt` 的 HEAD/大小/哈希/exit=0/12 项与日志一致；日志 :3479 为“全部通过（12 项）”，:929 含 R62 测试。本审未重跑 ci.ps1。
- PASS：`docs/testing/round62/round62-c-evidence.md` C1–C5；既有复跑 exit0，44 targets 558/0/7，lib313/bin23/R59=16/R61=11/R62=5/projectile18；clippy0 诊断，fmt 空。
- PASS：`target/round62/r62-d/attempt-2/checks/summary.tsv` 七项 exit0、7 PASS/0 FAIL/0 SKIP、实测45s；本审只读取该历史结果。
- PASS：A 红→绿证据位于 `target/round62/r62-a/attempt-1/`（red exit101；green-unit18/0、green-lib312/0）；B 报告 `target/round62/r62-b/report.md` 由 C 对账。
- PASS：`git diff --numstat a7362fc^..a7362fc -- crates/mir-client/src/ui/projectile.rs` exit0 得 637/184；A 的 +657 笔误已在 README 勘误。B 历史 ignored=2 亦已被 C/manifest 更正为7。

## PARTIAL 与 R63 承接

- PARTIAL：事件表物理回收无服务端协议；known_positions 无容量上限/LRU；owner_skip_frames 生产固定 false、缺 ActionFeed；570 残影仍 elapsed（10帧/600ms）。四项保留至 R63。
- PARTIAL：7 个既有历史 `#[ignore]` 无新增；不把 ignored 当作通过用例。
- PARTIAL：R61 原编号 P01–P12 全部承接，来源 `docs/testing/round61/round61-final-audit.md:43–56`，映射 `docs/roadmap.md:217–232`。
- P01 回收/clone/容量；P02 生产 SkipFrames；P03 缓存容量/LRU；P04 ImageInfo.offset；P05 设备/Live 可听；P06 33路 peak=22.844788 削波风险。
- P07 105 sfx 未逐一验证；P08 HiddenSpanSource 未全局化；P09 CWD/素材；P10 新代 len=0 且 total>0 与回收协议；P11 缺素材 SKIP；P12 R60 P-1..P-6。
- R62 已证严格时钟、目标兜底、运动 offset 一次叠加与会话代号复位，属于上述条目的局部进展，不据此关闭整项；不宣称完整125、真实可听或 R62 整体完成。

边界结论：文档提交不改代码；哈希与已留存门禁吻合；本审**未改代码/未 commit/push**，未改原证据与历史文档；最终只新增本验收文件（UTF-8/LF，≤70行）。
