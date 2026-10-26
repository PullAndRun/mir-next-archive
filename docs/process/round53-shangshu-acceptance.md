# R53 尚书省验收

**结论：ACCEPT-WITH-PARTIAL。** 依据刑部终审 PASS-WITH-PARTIAL、A2 复审及合法门禁 attempt=1；核心 Frost 玩家链与 F2 跑步同步可接受，AOI 全量仍不可关闭。
- 验收 HEAD：`dd59581902b48fb968bafe6417cb2a8365477419`（dd59581）；门禁受测 HEAD：`eb72958170ac9d7de4fcf467137fff8d096efa09`。
- 实查 `eb72958..HEAD` 仅新增终审报告与清单两份审计文档，符合方案门禁后文档例外；无代码/测试/CI/配置变化，attempt=1 保持有效。
- 依据：[方案](round53-plan.md)、[门下裁决](round53-menxia-review.md)、[派发](round53-shangshu-dispatch.md)、[补派](round53-shangshu-rework-dispatch.md)、[终审](../testing/round53/round53-final-audit.md)、[清单](../testing/round53/final-manifest.json)。

## 交付核对表

子单前缀均为 `JJC-20261026-001-`；G 为同一最终门禁 attempt=1（exit0、12/12）。
| 交付 | 提交 | 子单 | 证据路径与判定 | 门禁/终审复跑 |
|---|---|---|---|---|
| F | 617e202 | sub-61e3f0eb | `target/round53/frost-pvp/00-meta.md`、`00-summary.md`；核心链接受，摘要“无PARTIAL”以终审纠偏 | G；Frost 7、R52 8、magic 43 |
| F2 | 48aaec0 | sub-7d64c20b | `target/round53/f2/17-sha256.txt`、`09-mutation-without-sync.log`；run 双向同步，删同步反证 2 failed/exit101 | G；AOI 8 |
| A1 | eb72958 | sub-0ac5e1d8 | [v1](round53-aoi-acceptance.md)、`target/round53/aoi-audit/evidence.json`；历史 BLOCK 保留 | G；不把历史 BLOCK 改为 PASS |
| A2 | eb72958 | sub-e8afe1c8 | [v2](round53-aoi-acceptance-v2.md)、`target/round53/a2/evidence.json`；3 PASS/5 PARTIAL/0 BLOCK | G；AOI 8/7/1；看板结果依授权补录 |
| T | dd59581 | sub-391bea87 | [终审](../testing/round53/round53-final-audit.md)、[清单](../testing/round53/final-manifest.json)、`target/round53/final-gate-summary.log` | G 原件核验；独立复跑 7/8/43/8/7/1 |

## 验收判据与证据边界

- 本次直接读门禁原字节：`target/round53/final-gate.log`，193575B，SHA-256 `fdd252e00918bab434f9eda526a515b297158c97050f178fe9582a64974ef2a4`；末尾 12 行 OK，末个非空行“全部通过（12 项）”。
- `target/round53/final-gate.meta.txt`：attempt=1、exit_code=0、UTC 2026-10-26 00:49:21–00:53:34、head=eb72958；命令 `pwsh -NoProfile -File tools/dev/ci.ps1 -KillStale`。逐项 OK 为脚本分类，不能冒充逐进程原始 exit；身份/总 exit/时间由 meta 提供。
- 终审在受测树独立复跑 Frost/R52/magic/AOI集成/AOI库/随机对拍，均 exit0、非零执行，分别 7/8/43/8/7/1；最后 1 是前述 7 中同例复跑，不能重复累加。本次未重跑测试或全量门禁。
- 实查 tracked 干净；新增仅本验收文档，另有两份历史未跟踪冻结 JSON，未读取、未纳入。无 PARTIAL 冒充 PASS；缺 Server.MirDB 提前 return 的四例不计通过，maps 20 reported 仅16合成例有效。

## PARTIAL 逐条承接 R54

以下 R52 编号沿终审逐项承接，不以本轮绿灯抹除；第4项核心链已修，不再写成“仅注入毒”。
1. R54←R52-1：125 ObjectProjectile 无服务端生产者，手造负向包不闭合生命周期；149/26 子项不外推。
2. R54←R52-2：进程内 World→编解码→快照已证，真 socket 与真实客户端窗口仍待验收。
3. R54←R52-3：原生 dash 视觉/音频/时序、Walking 借帧等价性未闭合。
4. R54←R52-4：真实 PvP Frost 的 HP/毒/拒攻恢复已证；精确等级 +2 正侧、同会真实施法护栏及完整 PK 矩阵仍有覆盖缺口。
5. R54←R52-5：G1/G6 总体 open；根装备耗材/存档/快照仅子项，R51 B4 补证保留。
6. R54←R52-6：D3 离线/异图安全清场、丢 pending，与 Crystal 留场停伤/保留 ActionList 的差异继续追踪。
7. R54←R52-7：D2 Trap 外围坐标安全剪裁差异保留。
8. R54←R52-8：FireWall26 B3 具体 ID 替换/到期已证；C 仅创建/新入场0补发，缺撤销断言。
9. R54←R52-9：无 InTrapRock；FireWall 逐对象 ProcessSpell/InTrapRock 延后。
10. R54←R52-10：B4 先施法后 change_map 丢 pending、750ms 不传送；反序落当前图；89耗蓝=65+8×3，TemporalFlux30% 另分支。
11. R54←R52-11：guild、1965–1966 嵌套槽来源、旧技能耗材顺延；EnemyGuild 恒false/行会战不支持仍 PARTIAL。
12. R54←R52-12：C 首次 lifecycle/fmt 红原件未保全，F1 为反事实重建；B4 首次 fmt/clippy 红日志被覆盖，当前绿不可恢复历史。
13. R54←AOI-1 进图：四类完整首帧、自身排除、分组/包序；先补具名断言再跑，禁止0 tests关闭。
14. R54←AOI-2 离图：双方 seen/索引精确清理、后续广播不可达、重复离图无重复 remove。
15. R54←AOI-4 切图：三方 visible_players 精确集、旧 NPC 清空、旧图广播隔离；补真实资源四例。
16. R54←AOI-5 回收：普通物品/NPC 回收、尸体回收后精确 AOI/seen 撤销；C9 None,1000 实为金币，不算普通物品。
17. R54←AOI-6 广播：跨图负例、精确接收者身份、完整包序与 Crystal 差异契约。
18. R54←文档顺序：[aoi.md:155](../world/aoi.md#L155) 广播应按查询 `(y,x,id)`；[aoi.md:175](../world/aoi.md#L175) 玩家差分先 remove 后 add、批内 ID 序；首帧类型分组非 Crystal 插入序。
19. R54←路径漂移：F 实际为 `target/round53/frost-pvp/`，无 `f/`；T 清单实际为 `docs/testing/round53/final-manifest.json`，摘要为 `target/round53/final-gate-summary.log`，取代旧派单 target 清单/summary 路径，禁止另造原件。

## 残留与 R54 建议（六条）

1. AOI 全量仍不可关闭；优先补五条 PARTIAL 的精确集合、身份、撤销及包序断言，再逐路由复验。
2. 补 Frost 恰好 +2 正侧和同会真实施法拒绝专场；固定种子双侧计数，保留公式/RNG及怪物回归。
3. 门只接受已测基础生命周期；远端、精确5000ms、跑步撞门、初进图已开门快照、同名多实例另补；准备真实样本后跑四条 maps 资源例。
4. 125 生产者、真 socket/窗口、EnemyGuild 与离线异图差异按 R54 独立子目标承接，不以局部绿灯外推等价。
5. B4/C 与 F 首轮被覆盖原件继续披露；新 attempt 独立保全完整输出/exit/HEAD/时间/哈希，重建不得冒充原件。
6. 勘误 AOI 顺序及派单路径；F 超初始表列的 world 返回值调整已由独立评审和 Lead 接受，后续先同步写范围。

## 治理留痕

- 2026-10-26 09:29–09:34（Asia/Shanghai）；骨架先落盘后 CLI；角色尚书省，父单 `JJC-20261026-001`，HEAD dd59581；仅写本文，无 commit、无 crates 改动；links 校验 exit0（1663条相对链接/2篇README引用）、`git diff --check` exit0。
- 工具链：终审记录 rustc 1.99.1 (58cf8f9c3 2026-10-13)、cargo 1.99.1 (5f94df478 2026-08-27)、host x86_64-pc-windows-msvc、LLVM23.1.1；本次 Node v24.18.0，edict.mjs。
- 看板实查：F/F2/A/T 已 Done，A2 待补录；另有原尚书派发子单 `sub-85087a84` 仍 Doing、无结果，若 done readiness 拦截须原文上报，不擅自补单或绕过。
- CLI 实际回执：delegate-result exit0，stdout `✅ 委派结果 JJC-20261026-001-sub-e8afe1c8 → 父任务 JJC-20261026-001`；A2 已 Done。
- memo exit0，stdout `📝 JJC-20261026-001 任务记忆：shangshu`。
- done exit3，stdout为空，stderr `❌ JJC-20261026-001 委派 JJC-20261026-001-sub-85087a84 尚未完成或结果未回填，禁止收口`；未重试或绕过，随后 show 实证父单仍 Doing。
- validate exit0，stdout `✅ 看板校验通过：132 条任务符合 schema v3.1`；结构校验通过不替代收口。Lead 处理原派发子单真实结果后再按制度推进 Review/门下确认。
