# R64 门下省独立确认

**结论：PASS-WITH-PARTIAL，限定交付收口成立。** A-M1/A-M2 CLOSED；A-M3 与历史 PARTIAL 保留，不提升为全项 PASS。
确认者：gpt-6.1-sol；父单 `JJC-20261027-001`；核验 HEAD=`3378fc55898b0828791d8898a0da56ec7caf814c`。
证据根：`target/round64/menxia/attempt-1/`；独立脚本 `verify.cjs`、重算结果 `independent-audit.json`、三个命令原输出及逐命令 UTC/exit 均在该目录。

| 六项抽核 | 判定 | 独立证据与适用边界 |
|---|---|---|
| 1 门禁原字节/范围 | PASS | 重读原字节 205352B、SHA256 `3e228cef3ce730eac76ce936679b8557308d8d4f20ed1d766dbfb7a0b92b2604`；meta 12 字段逐键全符，EXIT=0、HEAD=063c6f6、UTC17:31:43Z–17:35:16Z、12 OK/0 FAIL、command/operator齐；指定 `7ace28f..3378fc5 -- crates`、`063c6f6^..3378fc5 -- crates/mir-protocol` 均0字节。operator仅核meta署名，不证明现实身份。 |
| 2 三产物/manifest | PASS | acceptance=53行/6160B/SHA256 `1827f5d7b6fa3d05a969706516417d25966d9df0abced5dd3616703849d6cd50`；18 artifacts 的字节/LF行数/SHA/HEAD blob/tracked逐项重算，mismatch=0；audit=46行/6868B/SHA256 `9245b0663140f15454d0bcc9f62ad7fe0f586b576777a66347c02e4ae631e7be`。 |
| 3 A-M修复与披露 | PARTIAL | README:43–47、C §9、run-checks.sh:14/18/28已读；18路径/计数/负控一致。独立 `--verify manifest`、`--verify negative` 均exit0，后者拒绝坏字节/同长SHA/meta/18项哈希错误。A-M1/A-M2 PASS；A-M3明示未新增测试、未实跑、不称已覆盖。 |
| 4 范围与诚实性 | PASS | `git show --stat bec6415`仅5个round64文档，JSON记完整stat及路径；文档批后crates零diff，新增ignore/allow=0；README:3、manifest.stage、roadmap:240均未写R64全项PASS。生成器exit0，其输出与8行树清单逐字节相同，SHA256 `802a11ec2d5e8f28ea729fec85a837f3366e229f637a2495f8b9fb02711710e1`。 |
| 5 独立复跑快证 | PARTIAL | 门下未重跑cargo，按授权复用尚书原证：summary.tsv=20 PASS/0 FAIL/0 SKIP，UTC18:05:08Z–18:07:13Z、exit0；projectile-ack-recheck.log及meta=3 passed/0 failed/0 ignored，UTC18:08:42Z–18:08:43Z、exit0。均位于target/round64/shangshu/attempt-1/，未冒称本次实跑。 |
| 6 六项残留承接 | PARTIAL | 对读acceptance:40–47、audit:25/35/42/44、README:47/49–56、roadmap:217–251；残留未清零，详见下列逐条承接；A-M3明确由README及acceptance承接R65，roadmap §十四未单列该场景，不误报其已逐字列齐。 |

README当前64行/6427B/SHA256 `64afe5dc3ff8116d2e8b35b8b3f49575aec2d2e9ea822d323ecfe8a68be2e560`；C当前93行/9389B/SHA256 `77e4b11b6b29726f7533ef2abbc8d014670b45823790d07fab1dff3813bcaa14`，均与manifest一致。
修复前audit的BLOCK是历史结论；当前A-M1/A-M2闭环由上述独立核验支持，不改写原审计。核验命令UTC为2026-10-26T18:14:22.307Z–18:14:47.145Z（JSON另记完整核验区间）。

## 快照与计数差

提交链 `063c6f6 → 7ace28f → bec6415 → dc094a3 → 3378fc5`；尚书基线dc094a3到本次HEAD仅新增验收书1文件/53行，功能与18项manifest内容未变。
C原证85行/8780B/30d08ae3…f416为历史快照；现补录版93行/9389B，净增8行/609B。README仍64行；树清单仍8行；manifest仍18项（不含自身与新增验收/确认书），未混用快照。
“六项”并非跨文档同号映射：README第五项是定向绿/门禁边界，尚书第五项是A-M3；两项均保留，以内容对应，不以编号假称新增或清零。

## 残留逐条承接（R65及历史台账）

1. 多消费者/克隆快照全局最低水位未做；一个生产UI消费者不等于全仓唯一Snapshot接收点（audit:35另记同步wait_for_snapshot）；roadmap §十四第1项承接。
2. 事件表未限容、未确认backlog可增长；roadmap §十四第2项承接，不以局部前缀裁剪证明内存有界。
3. known_positions容量/LRU仅契约；roadmap §十四第2项承接，不发明Crystal容量常数。
4. 服务端回收协议未做，本地NetCommand非wire ack；roadmap §十四第3项独立设计/评审/兼容验证承接。
5. A-M3半消费→半ack→再push UI组合未实跑；README:47及acceptance:46承接R65，新增测试后须按门禁规则重新验证。
6. R61 P01–P12原编号承接：回收/clone/容量、生产SkipFrames、缓存、ImageInfo.offset、设备/Live、33路削波、105sfx、全局HiddenSpanSource、CWD/素材、会话边界、缺素材early-return、R60 P-1..P-6；roadmap:217–232及§十四第4/6项不整项清零。

边界：仅写本文及获准证据目录；未改看板、未commit/push、未运行ci.ps1、未运行refresh；未触碰既有两份冻结JSON。看板Review按Lead最新交接记录，本次未独立查询或执行状态确认，不宣称已操作父单Done。
本次未发现必改项；上述PARTIAL与来源限制随限定收口继续保留。
