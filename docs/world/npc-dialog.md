# NPC 对话 / 商店 / 修理：协议与验收说明

日期：2026-10-21｜参考实现：`github/Crystal`（下文 `文件:行号` 均相对该目录）。
Rust 侧：`crates/mir-server/src/npc.rs`（页面执行/买卖修）、`crates/mir-db/src/npc_script.rs`
（脚本解析）、`crates/mir-server/src/session.rs`（`C.CallNPC`/`C.BuyItem`/`C.SellItem`/
`C.RepairItem`）。

> **默认数据集 = `cjlaaa`（简体中文库，v105）**，`Jev` 仅作英文兜底与数据契约测试。
> 本文的协议/流程不依赖具体库；涉及脚本/商品/NPC 坐标的例子若取自 Jev 会单独标注。
> 两库差异见 [`data-languages.md`](data-languages.md)，经济数值见 [`economy.md`](economy.md)。

> 与任务书的一处**协议核对**：`S.NPCResponse` 实际**只有 `page: Vec<String>`（原文行）**，
> Crystal **没有**「版本」字段，也**不携带最近 NPC**（`ServerPackets.cs:2893-2910`；
> Rust `server.rs:3085`）。「最近 NPC」由单独的 `S.NPCUpdate{NPCID}`（187，
> `ServerPackets.cs:5038`；Rust `server.rs:5624`）与客户端 `GameScene.NPCID` 维护；
> 对话页状态在服务端 `Player::npc`（本作）里。下文按真实协议写，不按传闻补字段。

## 1. 页面模型

### 1.1 `S.NPCResponse`（93）

| 字段 | 类型 | 说明 |
|---|---|---|
| `page` | `Vec<String>` | 页面**原文行**；客户端逐行渲染，按钮嵌在行内 |

- Crystal：`NPCResponse` 只有 `List<string> Page`（`Shared/ServerPackets.cs:2893-2910`）；
- 服务端在页面执行完后发，包含 `GOTO` 产生的中间页（`NPCScript.Response`，
  `Server/MirObjects/NPC/NPCScript.cs:895-900`）；本作 `npc.rs` 每个页面一条；
- 行内的按钮语法：`<文本/@key>`；`@exit` 由客户端自己关窗（`NPCDialog.cs:288-293`）；
- 富文本 `{文本/颜色}` 由客户端解析；本作客户端暂不支持，服务端 `render_say_line` 会剥掉
  颜色外壳只留文字（`npc.rs`，与服务端 `strip_color_markup` 对应），按钮原样保留。

### 1.2 对话状态与「最近 NPC」

- `S.NPCUpdate`（187）只有 `NPCID` 一个字段（`ServerPackets.cs:5038`），
  Crystal 在 `PlayerObject.cs:7887` 发；客户端用它记住当前对话 NPC；
- 本作把对话状态放在 `Player::npc`（`NpcState`：`npc_object_id` / `page_key` /
  `shown_buttons` 等），`S.NPCUpdate` 由客户端/服务端按需使用；
- **没有「版本」字段**——页面是否过期靠「当前页 key + 展示按钮白名单」判断，不靠版本号。

### 1.3 按钮白名单

`<文本/@key>` 的 key 解析：`<` 后第一个 `/@…` 到 `>`，遇 `/` 截断
（Crystal `NPCScript.cs:524,570-573`）。服务端只接受「当前页实际展示过」的按钮，
否则记日志并拒绝（`NPCScript.cs:831-857`；本作 `World::call_npc` 同）。

## 2. 交互流程

```text
玩家点 NPC
  → C.CallNPC{ObjectID, Key="[@Main]"}        # GameScene.cs:11301
  → 服务端校验：活着 / NPC 在 16 格内 / 可见 / 脚本存在
  → 执行 NPCScript.Call（:827-893）
  → 收到 S.NPCResponse{page:[...]}            # :895-900
点页面按钮 <Repair/@Repair>
  → C.CallNPC{ObjectID, Key="[@REPAIR]"}      # NPCDialogs.cs:288-299
  → 服务端检查白名单 → 执行 [@REPAIR] → 回新页面
点 <Close/@exit>
  → 客户端本地 Hide()，不发包                   # NPCDialogs.cs:288-293
```

要点：

1. **打开对话**：`C.CallNPC` 的 `Key` 是 `[@Main]`（Crystal 客户端固定大写下发，
   `GameScene.cs:11301`）；服务端把 key 归一到大写并补方括号
   （`mir_db::npc_script::normalize_key`）——`@main`、`[@Main]`、`@MAIN` 等价。
2. **点按钮**：客户端发 `C.CallNPC{ObjectID, Key=$"[{action}]"}`（`NPCDialogs.cs:299`），
   action 来自 `<文本/@action>`；服务端只认当前页展示过的 key。
3. **`@exit`**：Crystal 客户端**不发包**直接关窗；本作服务端也接受
   `C.CallNPC{Key="[@exit]"}`，清对话状态并回一条**空正文**的 `S.NPCResponse`
   （`npc.rs` call_npc 的 exit 分支，对应 Crystal `DelayedAction(NPC,-1)` +
   `CallNPCNextPage`，`NPCSegment.cs:3684-3689`、`PlayerObject.cs:7930-7942`）。
4. **空 key**：`normalize_key("")` 得到 `[@]`；若当前没有对话状态
   （`Player::npc.npc_object_id` 不匹配）→ `NpcError::NoDialog`；若已有对话但 `[@]`
   不在当前页展示按钮里 → `NpcError::NotACurrentButton`。**不要**把空 key 当 `@main`。
5. **`@main` 特殊**：`[@Main]` 总是允许（不受按钮白名单限制）；其它页必须是当前页展示过的
   按钮（`NPCScript.cs:831-857`）。
6. **GOTO 不经过客户端**：`#ACT GOTO @页面` 在服务端循环跟进，本作上限
   `MAX_NPC_HOPS = 8` 跳（Crystal 无上限）。

## 3. 商店（买 / 卖）

### 3.1 页面与包

| 页面键 | 打开时发的包 | 说明 |
|---|---|---|
| `[@BUY]` | `S.NPCGoods`（102，**gzip**） | 只买 |
| `[@BUYSELL]` | `S.NPCGoods` + `S.NPCSell`（103） | 买卖同页；先商品表再卖面板 |
| `[@SELL]` | `S.NPCSell`（103） | 只卖 |

- `S.NPCGoods{List, Rate, Type=Buy, HideAddedStats=true}`：Crystal `ProcessSpecial`
  （`NPCScript.cs:927-953`）、`SendNPCGoods`（`PlayerObject.cs:14677-14680`）；
- `Rate` 来自 `NPCInfo.Rate`（百分比），本作 `World::npc_goods` 使用
  `NpcInfo.rate / 100`（`npc.rs` 的 `apply_price_rate`，对应 `NPCScript.cs:1188`）；
- 商品表来自脚本 `[Trade]` 段（每行 `物品名 [数量]`）；**货号 = 该行在 `[Trade]` 里的下标**
  （`Envir.CreateShopItem`，`NPCScript.cs:667`）。

### 3.2 买 `C.BuyItem{ItemIndex, Count, Type}`

`World::buy_item`（`npc.rs:2280`）按 Crystal `PlayerObject.BuyItem`（`:7946-8010`）：
死亡/数量/面板类型 → 页面必须是买类页 → NPC 16 格内 → 货号与堆叠上限 →
`cost = Price() * PriceRate`、金币不足 → `CanGainItem` → 扣钱 `S.LoseGold` →
`S.GainedItem`。

- 货币是**角色金币**（`Account.Gold`），不是元宝；
- 价格公式 `ItemData.Price()`（`Shared/Data/ItemData.cs:516-542`），本作 `npc::item_price`；
- 买失败分支大多**静默**（Crystal 只 `return`）；本作在部分错误上补了提示/错误码
  （见 `docs/world/npc.md` §5.3）。

### 3.3 卖 `C.SellItem{UniqueID, Count}`

`World::sell_item`（`npc.rs:2412`）按 Crystal `PlayerObject.SellItem`（`:7990-8090`）：
先回 `S.SellItem{UniqueID, Count, Success}`（成功/失败都发）；页面必须是 `[@SELL]`/
`[@BUYSELL]`；离开 16 格**连失败包都不发**；`DontSell`/类型不允许 → 失败；成功进账
**`Price() / 2`**（`S.GainedGold`，`:8084`）。

### 3.4 金币 vs 元宝

- **NPC 商店/修理**：金币（`Gold`），扣款包 `S.LoseGold(68)`；
- **商城 GameShop**：元宝（`Credit`），走 `C.GameshopBuy` / `S.LoseCredit`，**不是 NPC 页面**
  （见 `docs/world/gameshop.md`）；
- 不要把 `S.NPCGoods` 的价格当元宝价；商城页签的 `ClientAuction` 价格才是元宝。

## 4. 修理

### 4.1 页面与包

| 页面键 | 打开时发的包 | 说明 |
|---|---|---|
| `[@REPAIR]` | `S.NPCRepair{rate}`（104） | 普通修理 |
| `[@SREPAIR]` | `S.NPCSRepair{rate}`（105） | 特修；本作**页面可打开但没有 `C.SRepairItem` 入口**（未接入） |

`REPAIR_KEY = "[@REPAIR]"` / `SPECIAL_REPAIR_KEY = "[@SREPAIR]"`（`npc.rs:135/137`；
页面常量数组 `:183/186`）；Crystal `NPCScript.cs:971-976`。

### 4.2 `C.RepairItem{UniqueID}` 与三条前置

`World::repair_item`（`npc.rs:2530`；Crystal `PlayerObject.cs:8094-8157`）：

1. **先**发 `S.RepairItem{UniqueID}`（`:8094`，客户端清「修理中」标记）；
2. **在修理页**：当前页必须是 `[@REPAIR]`（`REPAIR_PAGES`），否则返回；
3. **物品在背包**：按 `UniqueID` 找背包格；找不到返回；
4. **NPC 在范围内**：16 格（`Globals.DataRange`，`:8100` 循环里的 `Functions.InRange`）；
5. `BindMode.DontRepair` / `[Types]` 不允许 → 系统聊天（`无法修理`/`这里不能修`）；
6. `cost = RepairPrice() * PriceRate`；满耐久 → `DurabilityFull` + `npc_repair_full`；
   金币不足 → `npc_repair_no_gold`（含 cost/gold）；扣款 `S.LoseGold`；
7. **上限衰减**：`MaxDura = max(0, MaxDura - (MaxDura - CurrentDura) / 30)`
   （Crystal `PlayerObject.cs:8152`；任务书写的 `:8154` 是下一行 `CurrentDura = MaxDura`）；
8. `CurrentDura = MaxDura`（`:8154`）→ `S.ItemRepaired{UniqueID, MaxDura, CurrentDura}`
   （`:8157`）。

`RepairPrice()` 公式见 `docs/world/npc.md` §5.2（`ItemData.cs:543-563`）；
本作 `npc::repair_price` + `apply_price_rate`。

### 4.3 实测验证（EQ-1，真实 TCP）

- `@main` → `@REPAIR` 页面能打开，页面文本含 `<Repair/@Repair> Weapon.`；
- `C.RepairItem` 被正确校验：返回「需要 2 金币，你只有 0」（`npc_repair_no_gold` 的文案）；
- 结论：**页面流程与前置校验通**；完整扣款/耐久变化/`ItemRepaired` 的端到端仍待验收。

## 5. 已知缺口 / 待办

| 项 | 状态 | 说明 / 归属 |
|---|---|---|
| 金币掉落经济链路审计 | **待审计** | 掉落→拾取→金币/修理消费的闭环未审计；数值与「卖店启动金币」证据见 [`economy.md`](economy.md)，assign 给**兵部二号** |
| `Merchant_John` 4 格内 BFS 无路 | **待走位专项** | 脚本坐标 `(292,604)` 与服务器刷出位置/占位可能不一致；4 格内 BFS 无路，真人可行走但脚本走不到，需走位专项 |
| 特修 `[@SREPAIR]` | **未接入客户端** | 页面/`S.NPCSRepair` 在，`C.SRepairItem` 无入口；Crystal 费用 ×3 |
| 商店买/卖端到端 | **待验收** | 页面、`S.NPCGoods`、`C.BuyItem`/`C.SellItem`、扣款/进包均已实现，真实 TCP 全链路待验 |
| 修理端到端 | **待验收** | EQ-1 只验证页面+前置；扣款、上限衰减、`S.ItemRepaired` 待验 |

## 6. 验收清单（建议）

1. **页面**：点 NPC → `S.NPCResponse` 含 `[@Main]` 文本；点 `<View/@BuySell>` → 收到
   `S.NPCGoods` + `S.NPCSell`；点 `<Repair/@Repair>` → 收到 `S.NPCRepair`；
2. **买**：`C.BuyItem` 成功 → `S.LoseGold` 金额 = `Price()*Rate`、`S.GainedItem` 模板/数量正确；
   金币不足/背包满/超重 → 不开包、不扣钱；
3. **卖**：`C.SellItem` → 必回 `S.SellItem`；成功时 `S.GainedGold = Price()/2`；
4. **修**：满耐久/金币不足 → TextKey 提示；成功 → `S.LoseGold` + `S.ItemRepaired`，
   且 `MaxDura` 按 `/30` 衰减、`CurrentDura = MaxDura`；
5. **退出**：点 `<Close/@exit>` 客户端关窗；服务端 `[@exit]` 清状态；
6. **空 key**：`C.CallNPC{Key=""}` → `NoDialog`（归一为 `[@]`），不崩溃。

## 7. 参考

- 服务端实现：`crates/mir-server/src/npc.rs`（`call_npc:952`、`buy_item:2280`、
  `sell_item:2412`、`repair_item:2530`）；
- 脚本解析：`crates/mir-db/src/npc_script.rs`（`normalize_key`/`strip_arguments`）；
- 现有总文档：`docs/world/npc.md`（脚本格式、条件/动作矩阵、完整包表）；
- Crystal：`Server/MirObjects/NPC/NPCScript.cs`、`PlayerObject.cs`、
  `Client/MirScenes/Dialogs/NPCDialogs.cs`、`Client/MirScenes/GameScene.cs`、
  `Shared/ServerPackets.cs`/`ClientPackets.cs`、`Shared/Data/ItemData.cs`。
