# 经济与成长（金币 / 掉落 / 经验曲线）

本页记录第 26 轮 EQ-1（装备生效 → 耐久 → 修理）暴露的两块服务端口径：
**金币从哪来**、**升级要多少经验**。数据以 `resource/community/suprcode-crystal-database/Jev`
（`Server.MirDB` v88 + `Envir/`）为真值；`GameData::discover()` 的第一候选就是它。
NPC 商店的买卖/修理协议与验收清单见 [`npc-dialog.md`](npc-dialog.md)；本页只回答
「金币/经验从哪来、数值多少」。

## 1. 金币掉落（`DropInfo.Gold`）

| 怪 | 等级 | 经验 | `MonsterInfo.DropPath` | 掉落规则（文件原文） | 金币 |
|---|---|---|---|---|---|
| `Hen` | 2 | 9 | `Provinces\Hen` | `1/1 Chicken`、`1/20 Feather` | **无** |
| `Deer` | 12 | 18 | `Provinces\Deer` | `1/1 Venison`、`1/2 DeerMeat Q` | **无** |
| `Scarecrow` | 10 | 15 | `Provinces\Scarecrow` | `1/10 Gold 50` + 十几条物品 | `1/10`，基准 50 |
| `Scarecrow0` | 10 | 15 | `Provinces\Scarecrow0` | `1/1 Gold 2000` + 十几条物品 | `1/1`，基准 2000 |

- 全库 555 只怪：**等级 1..9 的怪没有任何一条 `Gold` 行**，最早掉金币的是 10 级 `Scarecrow`。
  因此「新号打鸡/鹿不掉金币」是**数据事实**，不是掉落链路的 bug。
- ⚠️ **文件陷阱**：`Envir/Drops/` 根目录还有 `Hen.txt`/`Deer.txt`/`Scarecrow.txt` 三个
  **空文件**，它们没有被任何 `DropPath` 引用；真实规则在 `Envir/Drops/Provinces/` 子目录。
  排查时按 `MonsterInfo.DropPath` 找文件，别拿根目录的同名空文件下结论。
- 数量公式：`DropInfo.Gold` 是基准 `base`，实掉 `[base/2, base + base/2)`（`DropTable::roll`，
  `crates/mir-db/src/drops.rs`）；超过 `MaxDropGold = 2000` 拆堆（`Settings.cs:228`，金币落在
  地上而不是直接进包：`Settings.DropGold = true`，`Settings.cs:229`）。
- 链路：`items.rs:2967` `drop_monster_loot` → `:3027` `drop_monster_gold` → `S.ObjectGold(65)`
  → `items.rs:1763` `pickup`（`C.PickUp`）→ `S.GainedGold(67)`。
  对应 Crystal `MonsterObject.cs:1093-1190`（`Drop`/`DropItem`/`DropGold`）、
  `PlayerObject.cs:7673-7685`（`GainGold`）。

## 2. 新号启动金币：卖店，不是捡钱

EQ-1 的 C 段是「修理要 2 金币、角色 0 金币」。在真实数据下，1~9 级怪不掉金币，
新号的启动资金来源只能是**把掉落物卖给 NPC**。1~9 级怪的全部可卖掉落（Jev v88）：

| 怪（等级） | 掉落物 | 掉率 | 物品类型 | 基础价 | 卖价（`Price()/2`） |
|---|---|---|---|---|---|
| Hen（lv2） | `Chicken` | 1/1 | `Meat`(15) | 80 | **32~40**（取决于掉落耐久） |
| Hen（lv2） | `Feather` | 1/20 | `CraftingMaterial`(16) | 80 | **40**（无耐久） |
| BugBat（lv5） | `BugShell` | 1/30 | `CraftingMaterial`(16) | 1000 | **500**（无耐久） |

> map `0`（比奇省）的 1..9 级刷怪点只有 Hen（5 组：中心 `(350,200)`/`(280,260)`/`(405,240)`/
> `(340,310)`/`(288,616)`；4 组各 5 只、最后一组 10 只，spread 均为 25）；
> BugBat 不出现在比奇省，列出仅作 1~9 级全量参考。


- 卖入口：`npc.rs:2412` `World::sell_item` → `npc.rs:2494` `GainGold(item_price / 2)`；
  Crystal `PlayerObject.cs:7988-8091`。
- 当前页必须是 `[@SELL]`/`[@BUYSELL]`，且 NPC 脚本 `[Types]` 允许该物品类型。

**新号 5 分钟内赚到 ≥2 金币的最短路径（BichonProvince = map `0`）**：

1. 出生点 `(330,270)`（`config.rs:35/118` `start_point`）。
2. 最近的 Hen 刷怪点中心 `(340,310)`（spread 25，5 只），实际可更近；击杀 1 只 Hen
   必掉 `Chicken`（1/1）。
3. 回城找买肉 NPC `Merchant_Sarah` `(313,271)`——脚本 `BichonProvince/BichonWall/Butcher1`，
   `[Types]=15`(`Meat`)、有 `[@BuySell]` 页；直线距离出生点约 17 格。
4. 开商店页卖掉 1 块 `Chicken` → **32~40 金币**（是 2 金币修理费的 16 倍以上）。

全程按 Crystal `MoveDelay = 600ms`（`HumanObject.cs:186`/`:2494`）估算，
步行 40 格约 24 秒，加打鸡/捡物/对话远小于 5 分钟（实际走位与寻路由 playtest 另验）。
其他买肉点：`Merchant_John` `(292,603)`（also `[Types]=15`）、
`MaterialDealer_Jeffery` `(303,255)`（`[Types]=16`，收 `Feather`）。

**结论：1~9 级不存在「打怪直接掉金币」的路径；EQ-1 C 的启动金币依赖「商店售卖」这条
端到端链路**（打怪 → 捡肉 → 找买肉 NPC → 开 `[@BuySell]` 页 → 卖 → `S.GainedGold` → 修理）。
这条链路属下一轮 NPC 商店端到端验收范围，本轮只做数据与公式取证。

## 3. 经验曲线（`ExpList.ini`）

运行时来源：`config/ExpList.ini` → 素材索引里的 `Configs/ExpList.ini` → 近似曲线
（`crates/mir-server/src/exp.rs:59-66`）。本仓库没有 `config/ExpList.ini`，实际加载
`resource/community/suprcode-crystal-database/cjlaaa/Configs/ExpList.ini`；
它与 Jev 的 `Configs/ExpList.ini` **500 行逐行相同**。`server.rs:443` 在生产路径显式装表。

`ExpTable::exp_for_level(level)` = 从 `level` 升到 `level+1` 所需经验（`exp.rs:257-260`）：

| 等级 | 1→2 | 2→3 | 3→4 | 4→5 | 5→6 | 6→7 | 7→8 | 8→9 | 9→10 | 10→11 |
|---|---|---|---|---|---|---|---|---|---|---|
| 经验 | 100 | 200 | 300 | 400 | 600 | 900 | 1200 | 1700 | 2500 | 6000 |

到 4 级累计 **600**（100+200+300）。solo、无 buff、`ExpRate = 1` 时：

| 怪（DB 等级） | 单杀经验 | 1→4 级所需击杀 |
|---|---|---|
| Hen（lv2） | 9 | **67** |
| Deer（lv12） | 18 | **34** |
| Scarecrow（lv10） | 15 | **40** |

Crystal 对照：`Server/Settings.cs:839-848`（`LoadEXP` 读 `Configs/ExpList.ini`，缺项沿用上一项、
默认 100）→ `PlayerObject.cs:9629`（`MaxExperience = ExperienceList[Level-1]`）→
`PlayerObject.cs:846-931`（`GainExp`：累加、`S.GainExperience`、`while` 跨级）→
`HumanObject.cs:2365-2381`（`ReduceExp`，`Level < targetLevel + 10` 不衰减）；
`Settings.cs:98` `ExpMobLevelDifference = true`、`Settings.cs:112` `ExpRate = 1F`。
本作对应：`world.rs:3485-3500`（`win_experience`）、`world.rs:5382-5395`（`reduce_exp`）。
**结论：与 Crystal 一致**（同一张 `ExpList.ini` + 同一套衰减/升级循环）。

注意：DB 里 Hen 是 2 级、Deer 12 级、Scarecrow 10 级（不是 1 级）；1 级角色打 12 级 Deer
在数学上没有等级差衰减，但实际命中/生存不现实。缺 `resource/` 时退回
`exp = level^3 * 100` 的近似曲线（1→4 要 3600 经验），只用于「没有素材也能起服」。

## 4. 掉落表路径：Windows 反斜杠在 Unix 上会全表丢失

`Server.MirDB` 由 Windows 工具生成，`MonsterInfo.DropPath` 用反斜杠
（Jev 的 449 只带 DropPath 的怪里 **446 只**如此，如 `Provinces\Hen`）。`Path::join` 在
Unix 上不把 `\` 当分隔符，修复前这 446 只怪的掉落表全部找不到 → `drop_table` 返回 `None`
→ 物品与金币**都不掉**（Windows 验收机不受影响，Linux/Docker 部署等于掉落系统整体失效）。
现由 `gamedata.rs:711` `normalize_drop_path` 在 join 前统一换成 `/`
（`gamedata.rs:518` `drop_table`；两种写法命中同一缓存项）。`#INSERT` 由 `mir-db` 内部 join，
本地 12,378 个样本掉落文件中 0 个使用 `#INSERT`，未来若用到需在 `mir_db::drops::DropTable::load`
做同样归一。

## 5. 验证

- `crates/mir-server/tests/gold_drop.rs`：
  `real_jev_data_hen_and_deer_have_no_gold_but_scarecrow_does`、
  `killing_a_real_scarecrow0_drops_gold_then_pickup_credits_and_broadcasts`、
  `killing_a_real_hen_drops_items_but_never_gold`。
- `gamedata.rs` 单测：`drop_paths_are_normalized_for_unix_and_keep_forward_slashes`、
  `windows_style_drop_path_loads_the_real_table`（用 Jev 的 `Provinces\Hen` 实测两种写法）。
- 命令：`cargo test -p mir-server --test gold_drop`、`cargo test -p mir-server --lib gamedata::tests`。
