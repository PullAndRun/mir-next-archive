# 核心闭环独立审计（2026-10-22）

> 审计人：刑部（xingbu）。只读；使用现有日志与确定性单测，未改代码、未起服务端、未执行 git。
> 基线：`HEAD=04a853f`（只读取 `.git/refs/heads/main`）。
> 结论：**核心闭环“部分成立”**。quest1/TP-1/PS-1/确定性升级/出生武器/发布冒烟均有直接证据；
> 但 **EQ-1 的最终断言与原始金币行矛盾**、**buy1b 缺少声明中的 `S.GainedItem(973)` 原始行**、
> **live levelup 仍是 SKIP 且训练假人未被门禁完全排除**。不能写成“核心闭环全绿”。

---

## 0. 先放矛盾/缺口（按重要性）

### 0.1 EQ-1：原始金币行与最终断言互相矛盾

`target/sell-gate14.log` 的原始金币/修理行：

```text
93:   [金币] -2（累计 +38）
94:   [修理] unique_id=1879729405078536207 max=4000 current=4000（S.ItemRepaired 114）
95: [t=+11:41] [OK]   EQ-1 修理/扣款 —— uid=1879729405078536207 耐久 3978->4000/4000，金币 40->40（扣 0）
```

第 93 行明确是 **-2、累计 +38**（与任务声明的“金币 40→38”一致），但第 95 行最终断言写的是 **40→40（扣 0）**。同一份日志对同一笔修理给出了两个不同金币结论；此外第 48 行还有一次：

```text
48: [t=+06:40] [FAIL] 捡起掉落 —— 附近没有可拾取的地面物品（ground=0）
```

后续重试在第 67 行成功（`[OK] 捡起掉落 —— 850 x1`），最终 `结果：全部通过（0 跳过）`。但这仍是门禁中途出现 `[FAIL]` 的事实，不能隐去。

### 0.2 buy1b 缺少声明的 `S.GainedItem(973)` 原始行

`target/buy1b.log` 全文 21 行，能证明：

```text
15: [t=+00:20] [OK]   去书店 —— 0104 的 图书管理员·史蒂文 @ (5, 8)
16:   [商店] 货架 50 项（#973x1, #974x1, #975x1, #976x1, #977x1, #978x1），价格倍率 1.00
17:   [buy1] NPC=图书管理员·史蒂文 货架 50 件，选 #973 unique_id=201
19: [t=+00:38] [OK]   buy1 购买 —— #973 x1（货架 #973），金币 0->0（价格 0（免费物品））
```

但 **没有一行 `S.GainedItem(973)`**。`target/buy1.log` 反而是 FAIL（未收到 `S.LoseGold`）；`target/msg-buy1.txt:35` 声称 `S.NPCGoods→C.BuyItem→S.LoseGold(0)→S.GainedItem(973)`，与 `buy1b.log` 的原始内容不一致。当前只能确认“货架 50、uid=201、免费购买成功”，不能确认该声明的完整包序。

### 0.3 live levelup 仍是 SKIP；训练假人问题已有记录

`target/levelup3.log:272`：

```text
[t=+05:13] [SKIP] levelup1 升级 —— 300s 未到 2 级：level=1 exp=0 LevelChanged=[]（live 战斗随机，确定性证据由服务端单测覆盖）
```

`target/levelup4.log:279` 仍 SKIP（exp=27），第 265-277 行连续 `target=2253 dmg=0 type=1`。
`docs/testing/gate-lessons.md:78-88` 记录了同类问题：EQ-1 曾选中训练假人
（`target_hp=999999`、`AC=999`、`target_agility=255`），90 秒全是 `dmg=0`，没有 `ObjectDied`/`ObjectGold`，一度被误判为掉落链路坏了。

因此：**确定性升级测试通过，不等于 live 升级链路已验证**；live levelup 在本轮仍是跳过项，训练假人/不可击杀目标的排除规则仍是门禁风险。

---

## 1. quest1 全生命周期

**日志**：`target/asset-quest1.log`（31 行，UTF-8）。

原始关键行：

```text
20:   [NPC对话] Jane @ (284, 606) 页行数=4
21:   [NPC对话] 首行='你好 PT950339, 这是你的首次冒险吗?'
22:   [NPC对话] 中文=True
23: [t=+04:01] [OK]   quest1 NPC对话 —— Jane 中文页 4 行：'你好 PT950339, 这是你的首次冒险吗?'
24:   [任务物品] #866x5 unique_id=1879711745578106886
25: [t=+04:02] [OK]   quest1 接取 —— Jane @ (284, 606)
26:   [quest1] task_list=['把食人花叶捎给工艺夫人', '前往 比奇省(0)']
27: [t=+04:02] [OK]   quest1 导航行 —— 前往 比奇省(0)
28:   [quest1] 任务物品栏=[(866, 5)]
29: [t=+04:09] [OK]   quest1 交付/奖励 —— completed_quests 含 1；经验 0->50（+50，cjlaaa），金币 0->0
31: 结果：全部通过（0 跳过）
```

**判定：PASS。** 有中文 NPC 页、任务文本、5 个任务物品、`completed_quests` 含 1、经验 0→50 的原始行。
`target/cjk-quest5.log:63` 的旧 `Jane 对话页原文=[]` 是更早一次采集；`asset-quest1.log` 已在第 20-23 行补上真实 4 行中文页，二者不冲突，但旧日志不能当最终证据。

---

## 2. TP-1：任务导航 + 门格 + 0→0122

**日志**：`target/reg-tp1c.log`（112 行，UTF-8）。

原始关键行：

```text
14:   [TP-1] task_list=['把食人花叶捎给工艺夫人', '前往 比奇省(0)']
15: [t=+03:52] [OK]   TP-1 导航行 —— 前往 比奇省(0)
16:   [TP-1] 门格 title=皇宫 pos=(347, 188)
109:   [地图] 切换地图 0@(346, 189) -> 0122@(11, 41)
110: [t=+12:27] [OK]   TP-1 到达 —— 0 -> 0122 @ (11, 41)（gate=皇宫）
112: 结果：全部通过（0 跳过）
```

**判定：PASS。** 导航行、门格 `(347,188)` 与真实跨图 `0→0122 @ (11,41)` 均有原始行。
`target/reg-tp1b.log:9` 的失败是旧脚本按英文名 `Assistant_Jane` 查找，而当前 NPC 显示名已中文化为 `助理·简`；后续 `reg-tp1c.log` 已按中文名/新路径通过，说明这是脚本适配问题，不是 TP-1 产品失败。

---

## 3. PS-1：重登 unique_id/耐久全等

**日志**：`target/reg-ps1.log`（25 行，UTF-8）。

原始关键行：

```text
12:   [PS-1] 重登前 gold=0 equipment=[] inventory=[(1879712712459550742, 221, 4000, 4000), (1879712712459550743, 317, 5000, 5000), (1879712712459550744, 318, 5000, 5000), (1879712712459550745, 658, 0, 0), (1879712712459550746, 722, 8000, 8000)] completed=[] spells=[]
19:   [PS-1] 重登后 gold=0 equipment=[] inventory=[(1879712712459550742, 221, 4000, 4000), (1879712712459550743, 317, 5000, 5000), (1879712712459550744, 318, 5000, 5000), (1879712712459550745, 658, 0, 0), (1879712712459550746, 722, 8000, 8000)] completed=[] spells=[]
20: [t=+00:00] [OK]   PS-1 重登 —— object_id=2306 pos=(330, 270)
21: [t=+00:00] [OK]   PS-1 逐字段比对 —— gold=0 equipment=0 completed=[] spells=[]
23: [t=+00:01] [OK]   PS-1 恢复后可玩 —— 移动 (330, 270)->(331, 270)，已发 C.Attack
25: 结果：全部通过（0 跳过）
```

**判定：PASS。** 五个物品 `(unique_id, item_index, current_dura, max_dura)` 重登前后逐项相同；`equipment`、`gold`、`completed`、`spells` 一致；恢复后可移动/攻击。

---

## 4. EQ-1：杀鸡→鸡肉→卖出→修理

**日志**：`target/sell-gate14.log`（97 行，UTF-8）。

原始关键行：

```text
42:   [sell1] attempt=7 kills=0/20 目标=鸡 id=2305@(350, 312) d=2 score=(0, 2)
43:   [DR-1] 命中 target=2305（S.ObjectStruck 74）
...
67: [t=+06:55] [OK]   捡起掉落 —— 850 x1
68:   [sell1] attempt=9 kills=2 ground=[('鸡肉', (360, 314))]
69: [t=+06:55] [OK]   sell1 捡鸡肉 —— attempt=9 杀鸡=2 item_index=850 count=1 unique_id=1879729405078536212
...
74:   [sell1] S.SellItem uid=1879729405078536212 count=1 success=True
75:   [金币] +40（累计 +40，余额 40）
76: [t=+07:29] [OK]   sell1 卖出 —— Chicken 卖出，金币 0->40（+40）
...
91:   [EQ-1] @main 页面=['你好，请问你需要些什么？', '', '<浏览/@BuySell> 商店.', '<修理/@Repair> 武器.', '', '<关闭/@exit>']
92:   [EQ-1] @Repair 页面=['你想要修理武器?', '请把它拿来让我看看.', '', '<返回/@main>']
93:   [金币] -2（累计 +38）
94:   [修理] unique_id=1879729405078536207 max=4000 current=4000（S.ItemRepaired 114）
95: [t=+11:41] [OK]   EQ-1 修理/扣款 —— uid=1879729405078536207 耐久 3978->4000/4000，金币 40->40（扣 0）
```

**判定：部分成立，证据有矛盾。** 杀鸡 2 只、鸡肉 850、卖出 +40、修理 3978→4000 均有原始行；但第 93 行是 `-2、累计 +38`，第 95 行却写 `40->40（扣 0）`。因此“金币 40→38”的事实由原始行 93 支持，但测试自己的最终断言文本是错的；此外第 48 行还出现过一次 `[FAIL] 捡起掉落`，后续重试才成功。门禁不能把该日志当作无矛盾的 clean pass。

---

## 5. buy1：0104 书店 / 货架 50 / uid=201 / 免费

**日志**：`target/buy1b.log`（21 行，UTF-8）。

原始关键行已列在 §0.2。可确认：

```text
15: 去书店 —— 0104 的 图书管理员·史蒂文 @ (5, 8)
16: [商店] 货架 50 项（#973x1, ...），价格倍率 1.00
17: [buy1] NPC=图书管理员·史蒂文 货架 50 件，选 #973 unique_id=201
19: [OK] buy1 购买 —— #973 x1（货架 #973），金币 0->0（价格 0（免费物品））
```

**判定：部分成立。** 0104、货架 50、uid=201、1 级免费购买均有原始行；但 **`S.GainedItem(973)` 没有原始行**，而 `target/buy1.log` 的旧版因未收到 `S.LoseGold` 失败；`msg-buy1.txt:35` 的包序声明不能作为原始日志证据。

---

## 6. 升级（确定性）

### 6.1 运行结果

```powershell
cargo test -p mir-server --lib level_up -- --nocapture
```

输出：

```text
running 1 test
test world::tests::level_up_from_99_plus_five_matches_crystal ... ok
test result: ok. 1 passed; 0 failed; 0 ignored
```

`level_up` 过滤只命中 `level_up_from_99_plus_five_matches_crystal`；另一条用精确名运行：

```powershell
cargo test -p mir-server --lib 'world::tests::multiple_levels_take_the_new_threshold_after_each_level' -- --exact --nocapture
```

输出：

```text
running 1 test
test world::tests::multiple_levels_take_the_new_threshold_after_each_level ... ok
test result: ok. 1 passed; 0 failed; 0 ignored
```

测试位置：`crates/mir-server/src/world.rs:6118`、`:6173`。

### 6.2 live 升级仍 SKIP

见 §0.3：`target/levelup3.log:272`、`target/levelup4.log:279` 都是 `[SKIP]`；`levelup4.log:265-277` 连续 `target=2253 dmg=0`，与 `docs/testing/gate-lessons.md:78-88` 的训练假人描述一致。**确定性测试 PASS 不能替代 live 升级 PASS。**

---

## 7. 穿装

```powershell
cargo test -p mir-server --lib starter_weapon -- --nocapture
```

输出：

```text
starter weapon: item_index=221 accuracy=7 dc=2..=4 hits=7225 rate=0.7225 damage=2..=4
test world::tests::starter_weapon_equip_grants_dc_and_accuracy ... ok
test result: ok. 1 passed; 0 failed; 0 ignored
```

测试位置：`crates/mir-server/src/world.rs:6199`。

---

## 8. 发布冒烟

```powershell
pwsh -NoProfile -File tools/dev/release-smoke.ps1
```

输出（节选）：

```text
release-smoke: packs=45 bytes=433976575 entries=151581 generator=mir-tools 0.2.0 deterministic=True
release-smoke: manifest.packs.json sha256=0be09eedf94e8caf44df0840a68d2974735a96d88bd5bafd65f42c57f90f0226
release-smoke: library-map libraries=67（期望 67）
release-smoke: release_smoke：只读 packs 加载/渲染，并与散装 dev 逐像素对比 …
  smoke: open=330ms render=191ms pixels=480000 diff_vs_dev=0 tiles_decoded=115/148 monster_decoded=6/14
  smoke: OK
release-smoke: OK（packs=45 ...）
```

退出码：**0**。判定：**PASS**（只证明发布包加载/渲染，不证明游戏业务闭环）。

---

## 9. 纯自有资源：launcher 日志不应出现 `resource/` / `Server.MirDB` / `.map`

```powershell
cmd /c findstr /N /C:"resource/" /C:"Server.MirDB" /C:".map" target/launcher-server.log
```

实测输出：**空**，`FINDSTR_EXIT=1`（无匹配）。`target/launcher-server.log` 最后写入时间为 15:52:55，本次检查的时间窗为该文件当前全部内容；未发现文件系统资源路径。

---

## 10. 未覆盖项（本轮不宣称）

以下链路没有在本次审计中取得端到端原始日志证据：

- 技能学习/施法（书 → `S.NewMagic` → 施法包）
- 组队（邀请/接受/经验共享）
- 交易（双方物品/金币交换）
- 行会（创建/加入/权限）
- 地图切换的自然门/传送点（TP-1 只覆盖了 `0→0122` 一条）
- 商店销售/回购/修理的负例（金币不足、满耐久、非可修物品）
- 训练假人/不可击杀对象的门禁排除（有 `gate-lessons.md` 记录，但本轮 live levelup 仍被其影响）

---

## 11. 最终判定

| 声明 | 判定 | 证据 |
|---|---|---|
| quest1 全生命周期 | **成立** | `asset-quest1.log:20-29` |
| TP-1 导航/门格/0→0122 | **成立** | `reg-tp1c.log:14-16,109-110` |
| PS-1 unique_id/耐久全等 | **成立** | `reg-ps1.log:12,19,21` |
| EQ-1 杀鸡/捡肉/卖/修理 | **部分成立，断言矛盾** | `sell-gate14.log:67-95`，第 93 vs 95 矛盾；第 48 行中途 FAIL |
| buy1 0104/货架50/uid201/免费 | **部分成立，缺 GainedItem 原始行** | `buy1b.log:15-19`；无 `S.GainedItem(973)` |
| 确定性升级两条 | **成立** | 两条单测均 1 passed |
| live 升级 | **未成立（SKIP）** | `levelup3.log:272`、`levelup4.log:279` |
| 穿装 DC/Accuracy | **成立** | `starter_weapon...` 1 passed |
| 发布冒烟 | **成立（仅发布包加载/渲染）** | `release-smoke.ps1` exit 0 |
| 纯自有资源（launcher 日志） | **成立（0 匹配）** | `findstr` exit 1 |
| 核心闭环整体 | **部分成立** | 上述边界；EQ-1/buy1 证据不完整、live 升级未证明 |

**总判定：核心闭环部分成立，不能写成“全绿”。** quest1/TP-1/PS-1、确定性升级、出生武器、发布冒烟有可复核证据；EQ-1 的最终金币断言与原始日志矛盾，buy1b 缺少声明的 GainedItem 行，live levelup 仍是 SKIP，训练假人问题仍未从门禁中消失。
