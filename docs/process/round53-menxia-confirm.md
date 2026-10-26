# R53 门下省收口确认

**结论：PASS-WITH-PARTIAL 收口成立。** PvP Frost 真实玩家 cast→延迟结算→HP/毒→真实拒攻与恢复链可信；F2 跑步双向视野修复可信。AOI 为 **3 PASS / 5 PARTIAL / 0 BLOCK，AOI 全量仍不可关闭**；PARTIAL 逐条承接 R54。
## 抽核表

| 核对项 | 原件与实际抽核 |
|---|---|
| 尚书验收 | [验收](round53-shangshu-acceptance.md) 66行、7807B，原字节 SHA-256 `be00cf034f61d4776f2b19ed308b496a349f827d554945d8549be653200e53fd`，ACCEPT-WITH-PARTIAL，与终审口径一致。 |
| 终审/清单 | [终审](../testing/round53/round53-final-audit.md) PASS-WITH-PARTIAL；[manifest](../testing/round53/final-manifest.json) 绑定 eb72958，Frost/R52/magic/AOI集成/库/随机复跑为7/8/43/8/7/1，末1为库内同例复跑，不累加。 |
| 门禁原件 | [log](../../target/round53/final-gate.log) 原字节193575B，SHA-256 `fdd252e00918bab434f9eda526a515b297158c97050f178fe9582a64974ef2a4`；内存GB18030解码直接核对12行OK及末行“全部通过（12 项）”。 |
| 门禁身份 | [meta](../../target/round53/final-gate.meta.txt)：attempt=1、exit_code=0、12/12；UTC 2026-10-26T00:49:21Z–00:53:34Z；head=`eb72958170ac9d7de4fcf467137fff8d096efa09`。逐项OK是脚本分类，不冒充逐进程原始exit。 |
| HEAD绑定 | 当前HEAD与origin/main均为`dd59581902b48fb968bafe6417cb2a8365477419`；`eb72958..HEAD`仅终审与manifest两份文档；符合门禁后文档例外，无代码/测试/CI/配置变化，原门禁有效。 |
| 审议链 | 已读[方案](round53-plan.md)、[门下评审](round53-menxia-review.md)、[派发](round53-shangshu-dispatch.md)、[补派](round53-shangshu-rework-dispatch.md)、[AOI v1](round53-aoi-acceptance.md)与[v2](round53-aoi-acceptance-v2.md)；v1历史BLOCK保留，v2仅解除run阻断。 |
| R54承接 | 采用验收第29–47行全部19项，不删减、不将局部PASS外推；下列清单逐项对应。 |
## PARTIAL 承接 R54（与尚书验收一致）

1. R52-1：125无生产者，手造负包不闭合生命周期；149/26子项不外推。
2. R52-2：进程内World→编解码→快照已证；真socket/真实客户端窗口待验。
3. R52-3：原生dash视觉/音频/时序与Walking借帧等价性待闭合。
4. R52-4：真实PvP Frost HP/毒/拒攻恢复已证；精确+2正侧、同会真实施法护栏、完整PK矩阵待补。
5. R52-5：G1/G6总体open；根装备耗材/存档/快照只属子项，保留R51 B4补证。
6. R52-6：D3离线/异图清场、丢pending与Crystal留场停伤/保留ActionList差异继续追踪。
7. R52-7：D2 Trap外围坐标安全剪裁差异保留。
8. R52-8：FireWall26 B3具体ID替换/到期已证；C仅创建/新入场0补发，缺撤销断言。
9. R52-9：无InTrapRock；FireWall逐对象ProcessSpell/InTrapRock延后。
10. R52-10：B4先施法后change_map丢pending、750ms不传送，反序落当前图；89耗蓝=65+8×3；TemporalFlux30%另分支。
11. R52-11：guild、1965–1966嵌套槽来源、旧技能耗材顺延；EnemyGuild恒false/行会战不支持仍PARTIAL。
12. R52-12：C首次lifecycle/fmt红原件缺失，F1为反事实重建；B4首次fmt/clippy红日志被覆盖，当前绿不能恢复历史。
13. AOI-1进图：四类完整首帧、自身排除、分组/包序；先补具名断言，禁止0 tests关闭。
14. AOI-2离图：双方seen/索引精确清理、后续广播不可达、重复离图无重复remove。
15. AOI-4切图：三方visible_players精确集、旧NPC清空、旧图广播隔离；补真实资源四例。
16. AOI-5回收：普通物品/NPC/尸体回收后精确AOI/seen撤销；C9 None,1000是金币，不算普通物品。
17. AOI-6广播：跨图负例、精确接收者身份、完整包序及Crystal差异契约。
18. 文档顺序：[AOI:155](../world/aoi.md#L155)广播按查询(y,x,id)；[AOI:175](../world/aoi.md#L175)玩家差分先remove后add、批内ID序；首帧类型分组非Crystal插入序。
19. 路径漂移：F实际target/round53/frost-pvp/；T清单docs/testing/round53/final-manifest.json，摘要target/round53/final-gate-summary.log；沿用现有原件。
## 非阻断项与 R54 建议（六条）

1. 先补AOI五条PARTIAL的精确集合、身份、撤销与包序；AOI总项保持开放。
2. Frost补恰好+2正侧、同会真实施法拒绝，固定种子双侧计数并保持公式/RNG及怪物回归。
3. 门仅接受基础生命周期；补远端/精确5000ms/跑步撞门/初进图开门快照/同名多实例，资源就绪后跑四条maps例；20 reported仅16合成例有效。
4. 125生产者、真socket/窗口、EnemyGuild、离线异图差异列R54独立目标。
5. 历史覆盖原件继续披露；新attempt分别保存完整输出/exit/HEAD/时间/哈希，重建不冒充原件。
6. 勘误AOI顺序及派单路径；F world返回值范围调整已获独立评审与Lead接受，后续先同步写范围。
## 治理留痕

- 2026-10-26 09:36–09:39（Asia/Shanghai）；角色menxia，父单JJC-20261026-001；HEAD dd59581；先落盘本文后跑CLI，仅写本文，不改crates、不commit、不跑全量门禁。
- memo exit0：`📝 JJC-20261026-001 任务记忆：menxia`；state Done exit0：`⏳ JJC-20261026-001 高风险操作 Review→Done，已进入 PendingConfirm，需 门下省 执行 confirm approve`。
- show核实pending_confirm.confirm_by=menxia；confirm exit0：`✅ JJC-20261026-001 confirm approve → Done`；validate exit0：`✅ 看板校验通过：132 条任务符合 schema v3.1`；父单最终Done。首次文档写入因空justification参数零写入被拒，修正后已在CLI前落盘；CLI无异常。
