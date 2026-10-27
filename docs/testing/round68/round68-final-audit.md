# R68 刑部独立终审
verdict：**PASS-WITH-PARTIAL（必改 0）**；仅认可 125 飞行 / 570 命中逐帧偏移切片，不构成整轮或真实素材验收。
审查基线：`cb7bc764ac90418192cd176170b93e47337a2611`（cb7bc76）；审计日期 2026-10-27 UTC。

| 项目 | 独立核验结果 |
|---|---|
| 提交链 / 范围 | `git log --oneline -3`、三提交 `git show --stat` 已核：c9e94c4 为 5 文件 +1394/−8；6ba5d45 仅门禁脚本 +3/−1；cb7bc76 实为 12 文档文件 +537。 |
| 门禁后功能冻结 | `git diff c9e94c4..HEAD -- crates` 空；生产 protocol/server diff 空；旧 draw_effect 保持，spellfx 仍旧入口，新入口与两处 projectile 接线一致。 |
| freeze / 属性 | 两 freeze JSON 均未跟踪且未入库；与 6ba5d45 比对 ignore/allow 无新增；mir-client 历史 ignore=7（全 crates 实际 34），均保持。 |
| 门禁原字节 | attempt-2 log 210789B，SHA256 `54cc09020c6b724ffa606a22a1634cac4429d1130c73e853e55c2d9f79da3f38`；原字节末尾汇总重算 12 OK / 0 FAIL。 |
| meta / 失败披露 | attempt-2 meta 13行/687B，SHA `413dade505b63a5312d9b905b3661233b2312e4b11de0b22b85d43f3034e13b5`；EXIT0，02:31:50–02:34:56Z，与 manifest 相符。 |
| attempt-1 / 修复 | README 与 manifest 均保留 c9e94c4 的 EXIT1：未启用 legacy，合成 magic.wzl 不可读；6ba5d45 开启 workspace 测试 legacy 后成功；meta 的 post-gate 补全已明确披露，日志未改。 |
| manifest 复算 | 17/17 项 path、bytes、lines、SHA256 独立复算一致，mismatch=0；gate 段与实测相符；未改 manifest。 |
| 外置摘要 | manifest 原字节 SHA `87d9ebef57fdd076cf4a0d6fda153bac6f188d166b41e9696f15a9394ac3211d`，与 manifest.sha256 匹配。 |
| A 最终真红 | 只读核 A attempt-3 manifest / red-failures / 原始红：最终 live SHA，编译成功、实际 2 用例、EXIT101、0 passed/2 failed；G4/G3 均首败 live:283:9，有目标像素 left/right。 |
| 红绿同 SHA / B | A 恢复绿 R68 4 + R62 5 + R63 3=12；live 红绿同最终 SHA；wire 红引用同 SHA 的 A attempt-2；B attempt-4 最终 live 的 helper 直接比较 RGBA 字节，绿 12/0/0，未伪称重做最终三变异红。 |
| 文档诚实性 | README、offset、client §5.33、roadmap 清楚限定合成夹具、未做真实素材/设备视觉签核、回归仅 R62/R63；候选/待终审措辞明确，无正面宣称整轮验收完成或生产全链通过；C 三轮闭环与七项 PARTIAL 完整承接。 |
| 文档 / 卫生 | `py -3 tools/docs/check_links.py` EXIT0（2208 相对链接、2 篇 README 引用）；`git diff --check` EXIT0；独立 fmt EXIT0。 |

独立全新环境：CARGO_TARGET_DIR=`target/round68/t/attempt-1/cargo-target`；TEMP=TMP=TMPDIR=`target/round68/t/attempt-1/tmp`，执行时均用绝对路径。
命令 / UTC / EXIT / 原始日志见 [runs.json](../../../target/round68/t/attempt-1/runs.json) 与 [静态复算](../../../target/round68/t/attempt-1/logs/static-audit.json)、[门禁解码计数](../../../target/round68/t/attempt-1/logs/followup.json)。

| 独立复跑命令 | UTC 起止（2026-10-27） | EXIT / 计数 |
|---|---|---|
| `cargo test -p mir-client --test round68_image_offset_wire --test round68_effect_offset_live --test round62_projectile_wire --test round63_projectile_wire --features mir-render/legacy` | 02:48:41.438–02:49:34.994Z | 0；live2 + wire2 + R62=5 + R63=3 = **12 passed / 0 failed / 0 ignored** |
| `cargo clippy -p mir-client -p mir-render --all-targets -- -D warnings` | 02:50:54.612–02:51:17.574Z | 0 |
| `cargo fmt --all -- --check` | 02:49:54.754–02:49:57.913Z | 0 |
| `py -3 tools/docs/check_links.py` / `git diff --check` | 02:49:57.914–02:49:58.764Z | 0 / 0 |

PARTIAL 原样承接：A 旧 manifest:394 措辞勘误；B 共享 isolated 旧日志归档限制；真实素材/设备视觉签核；全 UI offset；spellfx 居中迁移；DrawBlend/shadow/mask；R49–R67 历史遗留（含原编号清单，未宣布关闭）。
偏差披露：交接称文档批“10文件”，实际提交 12 文件；README 所引 ci.ps1:118 是新增说明起点，实际 workspace 测试命令位于 :120，不影响命令事实。
额外配置探针：初轮 clippy 另加 `--features mir-render/legacy`，EXIT101，报既有 minimap.rs:353 与 tiles.rs:311 的 `clippy::needless_return`；两文件本轮零 diff。规定无额外 feature 的 clippy 独立执行 EXIT0，失败日志仍保留；额外配置未纳入已通过范围。
审计脚本静态阶段已写 JSON/范围日志后，控制台 GBK 输出曾触发 UnicodeEncodeError；已检查落盘内容并用独立后续脚本完成计数，不影响原始测试结果。
最终测试指纹：wire 569行/23355B/`c330a4b628749184461b595fe0681fbf161f042b578586ebd02483e390908c73`；live 751行/30666B/`39a032c74d71b52f23f06be653e31cbb81c96967691cea384064bd5965116038`。
本终审仅写本报告和 `target/round68/t/attempt-1/**`；未改实现/测试/其余文档，未运行 ci.ps1，未 commit/push；post-audit manifest 刷新由 Lead 承接。
