# 掉落表（`Envir/Drops/*.txt`）

Crystal 的掉落**不在 `Server.MirDB` 里**：怪物模板只记一个文件名
（`MonsterInfo.DropPath`），真正的规则在 `Envir/Drops/<文件名>.txt` 文本里
（`Server/MirDatabase/MonsterInfo.cs:372-435` 的 `Load` / `FromLine` / `ParseGroup`，
掷骰在 `:489-560` 的 `AttemptDrop`）。本项目的解析与掷骰实现见
`crates/mir-db/src/drops.rs`。

## 文件格式

```text
;分号开头是注释，空行忽略
1/100 金创药(小量)        ; 1/100 概率掉这件物品
1/50  Gold 1000          ; 掉金币
1/20  GROUP*             ; 成组掉落，* 标记「随机」（Crystal 解析但不使用）
1/20  GROUP^             ; 成组掉落，^ 标记「命中第一条就停」
{
    1/1  物品A
    1/2  物品B
}
1/30 任务物品 Q          ; 行尾 Q = 需要任务
#INSERT [00Pots.txt]     ; 把另一个文件的全部内容追加进来
```

要点：

| 规则 | 说明 |
| --- | --- |
| 概率写法 | `1/N`，**前两个字符被直接砍掉**（`parts[0].Substring(2)`），所以必须保留 `1/` 前缀，数字才能被解析成 `N` |
| 金币 | `Gold <基准值>`，实际掉落区间 `[基准/2, 基准 + 基准/2)`（`AttemptDrop`） |
| 分组 | 组内的行是**独立再掷一次**的规则；`^` 命中第一条即停止，`*` 不影响行为 |
| `Q` 标记 | `QuestRequired`：没有对应任务时不掉（服务端实现掉落时要检查） |
| `#INSERT` | 被包含文件的行会**追加到末尾**，并且是相对 `Envir/Drops/` 的路径 |
| 排序 | Crystal `Load` 末尾会按「金币最后、其余按物品类型」排序（`MonsterInfo.cs:421-435`），这只影响掉落顺序，不影响概率 |

## 掷骰语义（`AttemptDrop`）

```text
chance = Chance / DropRate            // DropRate 是全局掉率倍率，默认 1
若 itemOffset > 0: chance -= chance * itemOffset / 100
chance = max(chance, 1)
若 Random.Next(chance) != 0 → 不掉
金币: gold = Random.Next(lower, upper)     // lower = Gold/2, upper = Gold + Gold/2
物品: 直接掉
分组: 逐条递归 AttemptDrop，累加结果；First 为真时命中一条就 break
```

注意 `Random.Next(chance)` 是**上界开区间**，与我们的 `MirRng::next_range(0, chance-1)` 等价；
随机数顺序也必须一致（先判概率、再抽金币数量），否则前后端的掉率表现会飘。

## 数据从哪来

索引里有现成的掉落文件：

```bash
python tools/asset-index/find_asset.py --name Drops --top community
# resource/community/clgforever910-mir2database/<版本>/Envir/Drops/*.txt
# resource/community/angelk727-mir2-serverdb/Envir/Drops/*.txt
```

服务端按 `MonsterInfo.DropPath`（为空时退回怪物名）去 `Envir/Drops/` 找文件，
找不到就是「这只怪不掉东西」，不报错。

## 还没做的

- `QuestRequired` 的任务判定（掉落时会直接跳过带 `Q` 的行，见 `docs/world/items.md`）；
- 全局掉率倍率 `Settings.DropRate`、会员掉率偏移的配置来源（现在是常量 `DROP_RATE = 1`）。

已经做完的（原本列在这里的）：

- **掉落物落地成地面物品与拾取**：见 `docs/world/items.md`（`World::drop_monster_loot` /
  `World::pickup`，`crates/mir-server/src/items.rs`）；
- 金币落地的拆堆规则：`Settings.MaxDropGold = 2000`（`Settings.cs:228`）。
