# R52 尚书省验收

日期：2026-10-26（Asia/Shanghai）；验收官：尚书省 / gpt-6.1-sol；主单 `JJC-20261025-004`。

**结论：ACCEPT-WITH-PARTIAL。** 功能门禁真实 PASS：attempt4 12/12、exit0，与当前提交树对齐；刑部 T3 为 PASS-WITH-PARTIAL。已批准 PARTIAL/R53 顺延可接受，无产品阻断必改。
验收基线 `main`，HEAD=origin/main=`43b7d866de79ff48deb44afdc6b25a67cad15c8b`；本次仅补过程文档与看板，不重跑全量门禁。

## 门禁与审计证据

下表 bytes/SHA-256 均本次从原字节独立复算；原日志与各自 meta 均保留于 ignored target。

| attempt / 原日志 | bytes | SHA-256 | head / exit / 结论 |
|---|---:|---|---|
| [1](../../target/round52/final-gate.log) | 140961 | `be54c5dbb5ac1bc2bc1759db3d456b7edb5e3af6a817106146a564ff6ac6d11c` | 0d7b4a1 / 1 / FAIL |
| [2](../../target/round52/final-gate2.log) | 192631 | `4da8ea80cc7a396e6690bac2ed68170a52a6ff824c7875577f4c9fd40d57bd42` | 46be80b / 0 / PASS 12/12 |
| [3](../../target/round52/final-gate3.log) | 192534 | `63f44e9ccd2a40ffe8aad35f2e01599ef691320632fe969efa9b42bbb7f4a538` | 86936d8 / 0 / PASS 12/12 |
| [4](../../target/round52/final-gate4.log) | 192533 | `26a70fa1f5750d41d2d3899e5883580bcaf5fb2b3ddedf618f7b209f4f04ef82` | 43b7d86 / 0 / PASS 12/12 |

attempt1 为 monster_combat 33 passed/1 failed（rng_is_reproducible_for_the_same_seed），另 i18n 103/2；不能写成四个 FAILED。修复 55e3593/46be80b 后 attempt2 通过；H 文档收口 dcbcca3/86936d8 后 attempt3 通过；43b7d86 纳入 T3 与总览/索引补遗后 attempt4 再通过。
[attempt4 meta](../../target/round52/final-gate4.meta.txt)：attempt=4、main、head=43b7d86、exit_code=0，UTC 2026-10-25 21:53:26–21:55:42，136s（上海 2026-10-26 05:53:26–05:55:42）；命令 `pwsh -NoProfile -File tools/dev/ci.ps1`。原日志为混合编码，UTF-8 read 拒绝；只读 GBK 解码后尾部 12 个 `OK   ` 与“全部通过（12 项）”断言通过，原字节未转写。
提交链：`6f975fa → 9a72d16 → f1f2e22 → 4efe600 → 6310329 → d06a916 → 0d7b4a1 → 55e3593 → 46be80b → dcbcca3 → 86936d8 → 43b7d86`。
[T3 v2 报告](../testing/round52/round52-final-audit-v2.md)：3371B / `c55e31ce645e697dce0716ea5ff10bd2b8bc45738043d5274fc79e67cce368d0`。
[T3 v2 清单](../testing/round52/final-manifest-v2.json)：5895B / `eec83060273e254f0eec78c4174749d88c4baef8ddc34e66663b9998e1d36280`。
T1/T2 历史 BLOCK 原件保留；唯一 G7 文档收口已完成，T3 接受。T3 当时基于 86936d8 与两处未提交文档补充；43b7d86 的四文件纯文档提交已纳入这些补充，attempt4 覆盖该提交树。T 子单结果保留旧 BLOCK，最新裁决以 T3 原件为准。

## 交付与看板 readiness

实际逐单 `show --json`，并从 `list --all --json` 复核：全部九个直系子单 Done、delegation_result 非空；仅 B1 有两项 todo，均 completed；父单验收前 Doing、todos 0/0。

| 交付 | 子单后缀 | 已交付与证据 |
|---|---|---|
| A 现状纠偏 | sub-8dc283b5 | 文档及证据索引；[独立评审](../../target/round52/a-review/review.md) |
| B1 耗材原语 | sub-a2b51ae2 | 根装备槽序、Count/UUID、双材料原子、生产存档重登；[评审](../../target/round52/b1-review/review.md) |
| B2 毒雾 | sub-5e9ebbde | 材料失败矩阵、Walk 有效格、tick/PK/清理、149/26，8 named；[三审](../../target/round52/b2-review/review-v3.md) |
| B3 Trap | sub-aaff7a87 | 3×2/3×3、Shock 与 AI 入口、八场/入视野、FireWall 差分，7 named；[三审](../../target/round52/b3-review/review-v3.md) |
| B4 R51 回归 | sub-dd381bee | 默认记录/SHA、Frost/Freezing/RNG、StormEscape，8 named，引用修订；[摘要](../../target/round52/b4/00-summary.md) |
| C 客户端 | sub-fa4e23ed | 装备 DeleteItem、149/26/入场、152/153 回纠、真 World 编解码与五变异；[复审](../../target/round52/c-review/review-v2.md) |
| H 文档 | sub-a48c1ccb | G7 与 L3=10000 勘误、生产 debug_assert 翻译披露；[摘要](../../target/round52/h/00-summary.md) |
| T 独立终审 | sub-b6dcd00e | 历史 BLOCK → H 收口 → [T3 PASS-WITH-PARTIAL](../testing/round52/round52-final-audit-v2.md) |
| 协调 | sub-f0e732fd | 派发与最终门禁协调已回填；[派发契约](round52-shangshu-dispatch.md) |

H 原检查 links/i18n/no-test/diff-check 全 exit0；i18n 66≤70、0≤0。H 看板摘要链接数 1466 属历史时点；链接计数随后置文档增长，最终以本次轻量检查回执为准。

## R52 → R53 PARTIAL 逐条承接

1. 125=ObjectProjectile 无服务端生产者；手造负向包只证明不误删场，不闭合正向生产生命周期；149 生成/26 撤销已实证。
2. World→encode/decode→snapshot 为进程内链；真 socket、真实客户端窗口仍待验收。
3. 原生 dash 视觉/音频/时序、Walking 借帧未完成等价验收。
4. PvP Frost 玩家目标仍为 PoisonState 注入后真实 attack 拒攻；未证明真实施法命中玩家。
5. G1/G6 总体 open；根装备耗材/存档与已证实快照子项不自动关闭全部边界；R51 B4 补证已交付。
6. D3 离线/异图安全清场、丢 pending 与 Crystal 留场停伤/保留 ActionList 不等价；保留已批准偏差。
7. D2 Trap 外围坐标安全剪裁保留差异，不扩大为完全等价声明。
8. FireWall26 是 B3 服务端具体 ID 的替换/到期证据；C 仅创建及新入场 0 补发，未断言 FireWall26 撤销。
9. 无 InTrapRock 系统；FireWall 逐对象 ProcessSpell/InTrapRock 延后，不被毒雾/Trap 绿灯吞并。
10. B4 可接受 deviation：先施法后 change_map 丢 pending、750ms 不传送；先换图后施法落当前图。89 耗蓝=65+8×3，TemporalFlux30% 是另一分支。
11. guild、1965–1966 嵌套槽来源及旧技能耗材仍顺延；本轮仅两新技能接耗材。
12. C 首次 lifecycle/fmt 原失败日志未保全；F1 reconstructed-failure 是反事实重建、非历史原件。B4 首次 fmt/clippy 红日志被覆盖；当前绿日志不恢复历史原件。

风险：上述为已接受的证据/等价性/体验残余，R53 按项补证，不作为 R52 产品阻断；不得将局部 PASS 夸大为完整 Crystal 等价。历史大冻结 JSON 保持未跟踪，target/.env 不纳入提交。门禁后的本文件仅为过程文档，补做 links/diff 静态检查。

## 复现（仓库根，均只读；不重跑全量）

```bash
git rev-parse HEAD origin/main
git log --reverse --format=%h 6f975fa^..HEAD
git show --stat 43b7d86
sha256sum target/round52/final-gate{,2,3,4}.log docs/testing/round52/round52-final-audit-v2.md docs/testing/round52/final-manifest-v2.json
wc -c target/round52/final-gate{,2,3,4}.log
py -3 -c 'from pathlib import Path; s=Path("target/round52/final-gate4.log").read_bytes().decode("gbk",errors="replace"); assert "全部通过（12 项）" in s; assert sum(x.startswith("OK   ") for x in s.splitlines()[-14:])==12'
node C:/Users/diana/.dsh/skills/edict/scripts/edict.mjs show JJC-20261025-004 --json
node C:/Users/diana/.dsh/skills/edict/scripts/edict.mjs validate --as shangshu
py -3 tools/docs/check_links.py
git diff --check
```
