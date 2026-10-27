# R68 尚书省验收
verdict：**ACCEPT-WITH-PARTIAL（必改 0）**；验收基线 HEAD `f5857f2`，2026-10-27 UTC。
**限定 125 飞行/570 命中逐帧偏移切片，不等于整轮/真实素材验收。**

| 核验项 | 独立结果 |
|---|---|
| 提交链 / 范围 | `c9e94c4`（5文件 +1394/−8）→`6ba5d45`（ci.ps1 +3/−1）→`cb7bc76`（12文件 +537）→`48fb28c`→`f5857f2`；log -5 与三提交 stat 已核。 |
| 功能冻结 / 白名单 | `git diff c9e94c4..HEAD -- crates`、工作树 crates 与功能前至 HEAD 的 protocol/server diff 均空；19个提交路径均为约定实现/测试、文档/清单与门禁脚本。 |
| freeze / 工作树 | 两历史 player-resource-freeze JSON 仍未跟踪，精确路径 ls-files / 全分支 log 均空；开始时仅这两项未跟踪；本验收只新增本报告及指定 target 目录。 |
| 门禁原字节 | attempt-2 log 210789B，SHA `54cc09020c6b724ffa606a22a1634cac4429d1130c73e853e55c2d9f79da3f38`；GB18030解码末尾重算12 OK / 0 FAIL，全部通过12项。 |
| meta / UTC / EXIT | meta 13行/687B，SHA `413dade505b63a5312d9b905b3661233b2312e4b11de0b22b85d43f3034e13b5`；head 6ba5d45、EXIT0、02:31:50–02:34:56Z与清单一致；UTC/进程EXIT来源为meta，日志汇总佐证12/0；post-gate补全披露存在。 |
| 失败门禁保全 | attempt-1 log 70912B，SHA `54716e56907a07ea9e84ba05cf4b2a1167d0c3cddd3c4773767d0c90061906eb`；README/manifest/终审均披露EXIT1和缺legacy特性的原因及修复。 |
| manifest / 外置摘要 | 独立18项 path/bytes/lines/SHA mismatch=0；清单SHA `41d14de66f93a3c1a66e0b12f86cce864752d27c2e3516ad362ad51c2537a94a`与manifest.sha256一致；gate head/exit/items/bytes/sha/UTC与原件一致。 |
| tracked blob 来源 | 全部已登记blob逐项rev-parse匹配；README、offset、plan等cb7bc76来源一致；audit blob `b03b82d393b583451c19606d4bbc20706a0dec49`与48fb28c一致。 |
| 最终测试指纹 | wire 569行/23355B/`c330a4b628749184461b595fe0681fbf161f042b578586ebd02483e390908c73`；live 751行/30666B/`39a032c74d71b52f23f06be653e31cbb81c96967691cea384064bd5965116038`。 |
| A/B/C抽核 | A attempt-3最终live SHA红EXIT101，原日志0/2，G4/G3均live:283:9且有目标RGBA left/right；B attempt-4原绿12/0/0，源码helper直接比较pixels字节；C三轮报告存续、闭环至必改0。 |
| 文档诚实性 | README/offset/client §5.33/roadmap均限定合成WZL与真实调用链，无整轮/真实素材签核正面宣称；七项PARTIAL保留。 |
| T偏差承接 | 文档批实际12文件；ci.ps1:118为说明起点、实际命令:120；额外legacy clippy探针既有needless_return告警（minimap.rs:353、tiles.rs:311）不计已通过范围，终审已披露。 |

独立环境：`CARGO_TARGET_DIR=target/round68/shangshu/attempt-1/cargo-target`；TEMP/TMP/TMPDIR均为同attempt下全新tmp，运行使用绝对路径。

| 独立抽证命令 | UTC 起止（2026-10-27） | EXIT / 实际计数 |
|---|---|---|
| `cargo test -p mir-client --test round68_image_offset_wire --test round68_effect_offset_live --lib r68_ --features mir-render/legacy` | 03:01:29.060–03:02:25.462Z | 0；live2+wire2=4；lib匹配0、332 filtered out，不计通过项。 |
| `cargo test -p mir-client --test round62_projectile_wire --test round63_projectile_wire --features mir-render/legacy` | 03:02:25.465–03:02:28.131Z | 0；R62=5、R63=3；合计12 passed/0 failed/0 ignored。 |
| `cargo fmt --all -- --check` | 03:02:28.133–03:02:31.319Z | 0。 |

必改清单：空（0）。
PARTIAL原样承接：A 旧 manifest:394 措辞勘误；B 共享 isolated 旧日志归档限制；真实素材/设备视觉签核；全 UI offset；spellfx 居中迁移；DrawBlend/shadow/mask；R49–R67 历史遗留。
看板自查：主单Doing；A/B/C/D均Done，todos空且delegation_result非空。A/B/C回填摘要停在早期SHA/BLOCK；验收以最终原证据与C final闭环为准，历史回填未篡改。
done回执：`node C:/Users/diana/.dsh/skills/edict/scripts/edict.mjs done JJC-20261027-005 --as shangshu`；UTC 03:04:23.665–03:04:23.765Z，**EXIT0，Doing→Review**；回显“执行完成，已提交尚书省审查”；仅执行一次，未请求Done/代confirm，validate EXIT0。
证据入口：[runs.json](../../target/round68/shangshu/attempt-1/runs.json)、[清单复算](../../target/round68/shangshu/attempt-1/logs/static-audit.json)、[抽核记录](../../target/round68/shangshu/attempt-1/logs/evidence-followup.json)；原始命令日志均在同attempt的logs目录。
未commit/push，未改crates、测试、manifest、门禁产物；未重复运行ci.ps1；尚书验收不代门下对口确认。
