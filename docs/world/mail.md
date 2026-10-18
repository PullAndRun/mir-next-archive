# 邮件（Mail）

> 参考实现根目录：`github\Crystal`（下文 `路径:行号` 均相对该目录）。
> 核心文件：
> * `Server/MirObjects/PlayerObject.cs`（`#region Mail` `:11672-12001`：`SendMail` `:11674-11845`、
>   `ReadMail` `:11847-11856`、`CollectMail` `:11858-11902`、`DeleteMail` `:11904-11913`、
>   `LockMail` `:11915-11924`、`GetMailCost` `:11926-11957`、`GetMail` `:11959-11988`）
> * `Server/MirDatabase/MailInfo.cs`（邮件实体 + `Send` + `CreateClientMail`）
> * `Server/MirDatabase/CharacterInfo.cs`（`List<MailInfo> Mail` `:85`、存档 `:500-503`）
> * `Server/MirObjects/NPC/NPCScript.cs`（`[@SENDPARCEL]` `:1078-1080`、`[@COLLECTPARCEL]` `:1081-1098`）
> * `Server/MirNetwork/MirConnection.cs`（包分发 `:652-672`、`SendMail`/`ReadMail`/`CollectParcel`/
>   `DeleteMail`/`LockMail`/`MailCost` `:2003-2052`）
> * `Server/Settings.cs`（邮件常量 `:241-247`、`MailSystem.ini` 读取 `:1550-1575`）
> * `Shared/ClientPackets.cs:2122-2276`、`Shared/ServerPackets.cs:5886-6004`、
>   `Shared/Data/ClientData.cs:154-205`（`ClientMail` 线格式）
>
> 本作实现：
> * `crates/mir-server/src/mail.rs`（常量、`Mail`/`MailState`、全部流程、`MailWrite` 落盘队列）
> * `crates/mir-server/src/{world.rs,session.rs,npc.rs}`（`Player::mail`、包分发、两个 NPC 页面、
>   `World::process_new_mail`）
> * `crates/mir-server/src/{store.rs,pg_store.rs}`（`MailRecord` + `AccountStore` 邮件函数）
> * `crates/mir-store/migrations/0007_mail.sql`、`crates/mir-store/src/{models.rs,repo.rs}`
> * 测试：`crates/mir-server/tests/storage_mail.rs`（26 个用例，其中邮件 17 个）
>
> `mir-protocol` **没有改动**：`C.SendMail`/`C.ReadMail`/`C.CollectParcel`/`C.DeleteMail`/
> `C.LockMail`/`C.MailLockedItem`/`C.MailCost`、`S.ReceiveMail`/`S.MailSent`/`S.MailSendRequest`/
> `S.ParcelCollected`/`S.MailCost`/`S.MailLockedItem` 与 `ClientMail` 数据类型都已存在。

---

## 1. 总览

| 主题 | Crystal | 本作 |
|---|---|---|
| 邮件实体 | `MailInfo`（`MailInfo.cs:5-167`），收件人在 `CharacterInfo.Mail`（`:85`） | `mail::Mail` + `mail` 表（`migrations/0007_mail.sql`） |
| 编号 | `++Envir.NextMailID`（`ulong`，`PlayerObject.cs:11835`） | `World::allocate_mail_id`（「Unix 毫秒 << 20」起跳，显式写号 + 推进序列） |
| 状态 | `Info.Mail`（内存）+ `Server.MirADB` 整块存档 | 运行期 `Player.mail.mails`；每次变更排队 `MailWrite`，会话异步写库 |
| 发送 | `SendMail(name, message, gold, items, stamped)`（`:11727-11845`） | `World::mail_send` |
| 阅读 | `ReadMail` 标记 `DateOpened` 后 `GetMail()`（`:11847-11856`） | `World::mail_read` |
| 领取 | `CollectMail`（`:11858-11902`） | `World::mail_collect` |
| 锁定/删除 | `LockMail` / `DeleteMail`（`:11904-11924`） | `World::mail_lock` / `mail_delete` |
| 报价 | `GetMailCost`（`:11926-11957`） | `World::mail_cost` + `mail_cost_request` |
| 列表 | `GetMail`（`:11959-11988`，最后 100 封） | `World::mail_list_packet` / `send_mail_list` |
| 登录下发 | `GetMail()` 紧跟 `GetCompletedQuests()`（`:1190`） | `session::on_start_game`（任务包之后、行会状态之前） |
| 过期 | **没有** | **本作扩展 30 天**（`MAIL_EXPIRE_DAYS`），登录查询时过滤并删除 |

**收件箱属于角色**：Crystal 的 `Mail` 是 `CharacterInfo` 的字段（`CharacterInfo.cs:85`），
本作沿用这一点（`mail.character_id`）。

---

## 2. 协议与包序

| 语义 | 客户端包 | 服务端包 | Crystal |
|---|---|---|---|
| 打开写信界面 | `C.CallNPC{Key="[@SENDPARCEL]"}`（50） | `S.MailSendRequest`（233，空包） | `NPCScript.cs:1078-1080` |
| 询价 | `C.MailCost{Gold, ItemsIdx[5], Stamped}`（123） | `S.MailCost{Cost}`（236） | `MirConnection.cs:2045-2052` |
| 发送 | `C.SendMail{Name, Message, Gold, ItemsIdx[5], Stamped}`（117） | 逐件 `S.DeleteItem`（79）→ `S.LoseGold`（68）→ `S.MailSent{Result}`（234） | `PlayerObject.cs:11727-11845` |
| 阅读 | `C.ReadMail{MailID}`（118） | `S.ReceiveMail`（231，整个列表） | `:11847-11856` |
| 邮局受理包裹 | `C.CallNPC{Key="[@COLLECTPARCEL]"}` | `S.ParcelCollected{Result}`（235）→ `S.ReceiveMail` | `NPCScript.cs:1081-1098` |
| 领取附件 | `C.CollectParcel{MailID}`（119） | 逐件 `S.GainedItem`（66）→ `S.GainedGold`（67）→ `S.ParcelCollected{Result=1}` → `S.ReceiveMail` | `PlayerObject.cs:11858-11902` |
| 锁定/解锁 | `C.LockMail{MailID, Lock}`（121） | `S.ReceiveMail` | `:11915-11924` |
| 删除 | `C.DeleteMail{MailID}`（120） | `S.ReceiveMail` | `:11904-11913` |
| 附件格子锁 | `C.MailLockedItem{UniqueID, Locked}`（122） | `S.MailLockedItem`（232，原样回显） | `MirConnection.cs:667-669` |

### 发送的精确包序

`PlayerObject.SendMail(name, message, gold, items, stamped)`（`:11727-11845`）：

1. 限速检查（静默）→ 2. 正文长度 → 3. 收件人 → 4. 收件人邮箱上限 → 5. 邮费与总价
→ 6. 邮票 `S.DeleteItem{UniqueID, Count=1}` → 7. 每个附件 `S.DeleteItem{UniqueID, Count=物品数}`
→ 8. 扣钱 `S.LoseGold{Gold = 附件金币 + 邮费}` → 9. `MailInfo.Send()`（进收件箱）
→ 10. `S.MailSent{Result=1}`。

失败分支的包不完全一致（见 §11 第 3 条）：

| 失败点 | 服务端包 |
|---|---|
| 金币不够 | `S.MailSent{-1}` |
| `BindMode.NoMail` | 系统聊天 + `S.MailSent{-1}` |
| `BindMode.DontTrade` / 租借禁交易 | 只有系统聊天（**没有** `S.MailSent`） |
| 正文太长 / 收件人不存在 / 邮箱满 | 只有系统聊天 |
| 限速 | **什么也不发** |

### 登录包序

Crystal 的 `GetMail()` 在 `GetCompletedQuests()`（`PlayerObject.cs:1188`）之后、
`GetFriends()`（`:1191`）之前调用（`:1190`）。本作登录序列：

```text
S.StartGame{Result=4} → MapInformation → NewMapInfo → UserInformation → SwitchGroup
→ GuildBuffList{GuildBuffs} → 任务包 → S.ReceiveMail → 行会状态包 → 欢迎聊天
```

`session_login_sends_mail_list_and_filters_expired_mail` 断言了
`UserInformation` 在 `ReceiveMail` 之前。

---

## 3. 发送流程（逐条对照）

`World::mail_send(sender_id, recipient, message, gold, items, stamped)`：

| # | 规则 | Crystal |
|---|---|---|
| 1 | 限速：`now < NextMailTime` → 静默丢弃；否则 `NextMailTime = now + 10000`（**先置位再校验**，失败的尝试也会进入 10 秒冷却） | `:11676-11682`、`:11729-11735` |
| 2 | 正文长度 `message.Length > 500` → 系统聊天「邮件正文不能超过 500 字」 | `:11737-11741` |
| 3 | 收件人按**角色名**查找（`OrdinalIgnoreCase`，允许离线） → 找不到就聊天「找不到该玩家」 | `:11743-11749`、`Envir.cs:4712-4719` |
| 4 | 收件人 `Mail.Count > 50` → 聊天「收件人的邮箱已满」（判定是 **> 50**，不是 ≥） | `:11751-11755` |
| 5 | 邮费 = `GetMailCost(items, gold, stamped)`；`total = gold + 邮费`；`Account.Gold < total` → `S.MailSent{-1}` | `:11757-11767` |
| 6 | 邮票：`ItemType.Nothing` + `Shape == 1` + `Count ≥ 1`，消耗 1 个并发 `S.DeleteItem{Count=1}`；**没找到邮票时静默按「未贴票」继续** | `:11769-11786` |
| 7 | 附件：前 `stamped ? 5 : 1` 格；`ItemsIdx[j] == 0` 跳过；按 UUID 查背包；`DontTrade`/`NoMail`/租借 `DontTrade` 拒绝；**查不到的 UUID 静默跳过** | `:11788-11824` |
| 8 | 扣 `total`（附件金币 + 邮费）并发 `S.LoseGold` | `:11826-11830` |
| 9 | 建邮件：`MailID = ++NextMailID`、`Sender = 自己名字`、`Message`、`Gold = 附件金币`（**不含邮费**）、`Items = 附件` | `:11832-11842` |
| 10 | `S.MailSent{1}` | `:11844` |

**邮票对附件上限的影响**：没贴邮票时只取第 1 个附件（`for j < 1`），但 `GetMailCost` 同样只
估算第 1 个；第 2~5 格里的 UUID 会被**静默忽略**，物品留在背包。测试
`send_mail_unstamped_takes_only_the_first_attachment` 覆盖这一行为。

**好友黑名单跳过了**：Crystal 在收件人查找之后检查双方的 `Friends.Blocked`
（`:11704-11714`）；本作没有好友系统，见 §10。

---

## 4. 邮费公式

`GetMailCost`（`:11926-11957`）逐位移植（`World::mail_cost`）：

```text
cost = 0
if (!MailFreeWithStamp || !stamped)
    if (gold > 0 && MailCostPer1KGold > 0)
        cost += floor(gold / 1000) * MailCostPer1KGold
    if (items.Length > 0 && MailItemInsurancePercentage > 0)
        for j in 0..(stamped ? 5 : 1):
            cost += floor(item.Price() / 100 * MailItemInsurancePercentage)
```

| 常量 | 值 | Crystal |
|---|---|---|
| `MAIL_FREE_WITH_STAMP` | `true` | `Settings.cs:244` |
| `MAIL_COST_PER_1K_GOLD` | `100` | `Settings.cs:245` |
| `MAIL_ITEM_INSURANCE_PERCENT` | `5` | `Settings.cs:246` |
| `MAIL_CAPACITY` | `100` | `Settings.cs:247` |
| `MAIL_AUTO_SEND_GOLD` | `false` | `Settings.cs:242` |
| `MAIL_AUTO_SEND_ITEMS` | `false` | `Settings.cs:243` |

* `item.Price()` 用 `crate::npc::item_price`（逐位对照 `Shared/Data/ItemData.cs:516-542`）。
* 整数化：`floor(item.Price() / 100 * pct)` = `price * pct / 100`（正数等价）。
* 例：附件金币 2500 → `2 * 100 = 200`；附件物品价格 1000 → `1000 * 5 / 100 = 50`；
  贴邮票且 `MailFreeWithStamp` → `0`（`GetMailCost` 第一行直接返回）。

测试 `mail_cost_request_matches_crystal_formula` 断言了 `250` 与贴票后的 `0`。

---

## 5. 包裹生命周期（`Collected` 的真实含义）

Crystal 的 `Collected` 不是「附件已进背包」，而是「包裹已被邮局受理」：

1. **发送时**（`MailInfo.Send`，`MailInfo.cs:99-143`）：先置 `Collected = true`；如果邮件带
   金币或物品（`Parcel`），再按 `MailAutoSendGold`/`MailAutoSendItems` 决定是否改回 `false`。
   两个开关默认都是 `false`，所以**任何包裹的初值都是 `false`**；纯信件是 `true`。
2. **邮局页面**（`[@COLLECTPARCEL]`，`NPCScript.cs:1081-1098`）：把所有 `Parcel` 邮件置
   `Collected = true`，回 `S.ParcelCollected{Result=0}`（没有待受理的包裹时 `Result=-1`），
   然后 `GetMail()` 重发邮箱。
3. **领取**（`C.CollectParcel`，`:11858-11902`）：要求 `Collected == true`，否则聊天
   「请先到邮局受理包裹」；背包放不下（`CanGainItems`）时聊天「背包已满」；之后逐件
   `S.GainedItem`、`GainGold`、清空 `Items`/`Gold`、`Collected = true`、`S.ParcelCollected{1}`、
   `GetMail()`。

> **反直觉**：`C.CollectParcel` 是**状态机**，不检查距离/NPC——只要 `Collected` 为真就能领。
> 玩家必须先点邮局 NPC 页面把包裹标记成已受理。

本作实现：`World::mail_collect_parcel_page` / `World::mail_collect`。
「能不能全放下」用 `World::can_gain_mail_items`，算法照抄 `HumanObject.CanGainItems`
（`HumanObject.cs:7629-7660`）：可堆叠物品若在背包里有同类堆且会溢出，就额外占一格，
再看 `FreeSpace >= 物品数 + 溢出数`。

---

## 6. 阅读 / 列表 / 锁定 / 删除

* **阅读**：`ReadMail` 只把 `DateOpened` 置为 `Envir.Now`，然后 `GetMail()`（`:11847-11856`）。
  **没有**专门的「读信回执」包，`S.ReceiveMail` 就是回执；本作 `mail_read` 只置
  `opened = true`、排队 `MailWrite::Save`、重发列表。
* **列表**：`GetMail` 取最后 `MailCapacity = 100` 封（`:11963`），逐条
  `MailInfo.CreateClientMail()`，发 `S.ReceiveMail`，并清 `NewMail`（`:11985`）。
* **新邮件提示**：收件人在线时 `MailInfo.Send` 置 `RecipientInfo.Player.NewMail = true`
  （`MailInfo.cs:137-140`）；`PlayerObject.Process` 看到后发一条系统聊天
  「你有新邮件」并 `GetMail()`（`:499-503`）。本作在 `World::tick` 里调用
  `World::process_new_mail`。
* **锁定**：`LockMail` 只改 `Locked` 再 `GetMail()`（`:11915-11924`）。
* **删除**：Crystal 的 `DeleteMail` **不检查** `Locked`、也不检查附件是否已领取
  （`:11904-11913`），客户端负责拦。本作在服务端补了两道保护（见 §8）。
* **附件格子锁**：`C.MailLockedItem` 服务端只原样回显（`MirConnection.cs:667-669`），
  没有任何状态。

---

## 7. 持久化

### 迁移

`migrations/0007_mail.sql`（字段对照见该文件头）：

| 列 | Crystal |
|---|---|
| `id BIGSERIAL` | `MailInfo.MailID`（显式写号后推进序列） |
| `character_id BIGINT`（外键，级联删除） | `CharacterInfo.Mail` 的所属角色 |
| `sender_name TEXT` | `MailInfo.Sender` |
| `subject TEXT` | **本作扩展**（Crystal 没有主题，恒空串） |
| `message TEXT` | `MailInfo.Message` |
| `items BYTEA` | `MailInfo.Items`（`encode_slots` 字节块） |
| `gold BIGINT` | `MailInfo.Gold`（`CHECK 0..4294967295`） |
| `date_sent TIMESTAMPTZ` | `MailInfo.DateSent` |
| `opened` / `locked` / `collected` | `MailInfo.Opened` / `Locked` / `Collected` |

`mir-store` 新增函数：`create_mail` / `list_mail` / `load_mail` / `save_mail` / `delete_mail`，
以及邮件收件人查找用的 `find_character_by_name`（大小写不敏感、允许离线，对应
`Envir.GetCharacterInfo(string)`，`Envir.cs:4712-4719`）。

### 运行期

世界层是同步的、存储层是异步的（与行会同一套做法）：

* 所有变更只把 `MailWrite::Create/Save/Delete` 排进 `World::mail_writes`；
* 会话在每个邮件包处理完之后调用 `flush_mail_writes`（`mail::persist_mail_writes` 逐条写库）；
* 下线时再补一次；邮件本体不写回 `characters` 行。

### 登录 / 离线收件人

* 登录：`session::load_mailbox` 读 `list_mail(character_id)`，解码附件、过滤过期邮件
  （顺手 `delete_mail`），然后放进 `Player.mail.mails`；`ReceiveMail` 在任务包之后下发。
* 离线收件人：会话用 `find_character_by_name` 查角色（`World` 不持有离线角色），
  带着 `MailRecipient{character_id, name, mail_count}` 调 `World::mail_send`；
  邮件只落库，下次收件人登录时出现在收件箱。

---

## 8. 与 Crystal 的**有意**差异

1. **过期**：Crystal 没有邮件过期（`Settings.cs:241-247` 无相关常量、`MailInfo` 无字段）。
   本作扩展 `MAIL_EXPIRE_DAYS = 30`（`// 扩展：` 注释在 `mail.rs`），登录时过滤
   `date_sent + 30 天 < now` 的邮件并删除。
2. **删除保护**：Crystal 交给客户端；本作服务端拦两类删除——已锁定（`MailError::Locked`）、
   附件仍在（`MailError::UncollectedAttachments`，包括「已到邮局受理但还没领取」）。
   领取完附件后（`items`/`gold` 都清空）才能删。
3. **金币挂在角色上**：Crystal 的 `Account.Gold` 是账号钱包；本作用 `characters.gold`。
4. **主题列**：Crystal 无此字段；本作保留 `subject` 列恒空串。
5. **好友黑名单**：Crystal 检查双方 `Friends.Blocked`（`:11704-11714`）；本作无好友系统，跳过。
6. **`can_reply` 恒真**：玩家邮件的 `CanReply = true`（`MailInfo.cs:44`、`PlayerObject.cs:11833`）；
   系统邮件（`Envir.MailCharacter`）本作不做，所以恒真。
7. **`MailSendRequest` 只由 NPC 页面触发**：与 Crystal 一致（`NPCScript.cs:1078-1080`），
   没有额外的「未读邮件自动弹写信窗」。

---

## 9. 常量与配置

| 常量 | 值 | Crystal |
|---|---|---|
| `MAIL_TEXT_MAX` | 500（UTF-16 码元） | `PlayerObject.cs:11684`、`:11737` |
| `MAIL_RATE_MS` | 10_000 | `:11682`、`:11735` |
| `MAIL_RECIPIENT_LIMIT` | 50 | `:11698`、`:11751` |
| `MAIL_CAPACITY` | 100 | `Settings.cs:247` |
| `MAIL_MAX_ATTACHMENTS` | 5 | `ClientPackets.cs:2129` |
| `MAIL_ATTACHMENTS_WITHOUT_STAMP` | 1 | `PlayerObject.cs:11790` |
| `MAIL_COST_PER_1K_GOLD` | 100 | `Settings.cs:245` |
| `MAIL_ITEM_INSURANCE_PERCENT` | 5 | `Settings.cs:246` |
| `MAIL_FREE_WITH_STAMP` | `true` | `Settings.cs:244` |
| `MAIL_AUTO_SEND_GOLD` / `MAIL_AUTO_SEND_ITEMS` | `false` | `Settings.cs:242-243` |
| `MAIL_EXPIRE_DAYS` | 30 | **本作扩展** |

本作**不读** `Configs/MailSystem.ini`（Crystal `Settings.LoadMail`，`Settings.cs:1550-1575`）；
常量硬编码在 `mail.rs`，与默认值一致。要改配置应改常量（或后续接入 `ServerConfig`）。

---

## 10. 测试清单（邮件部分）

| # | 用例 | 覆盖点 |
|---|---|---|
| 1 | `send_mail_with_attachment_and_gold_uses_stamp_and_acks` | 邮票消耗、附件扣除、`S.LoseGold` 总价、`S.MailSent{1}`、落盘记录、附件字节块 |
| 2 | `send_mail_unstamped_takes_only_the_first_attachment` | 未贴票只取第 1 件、保险费公式、`> 5` 附件由 API 拒绝 |
| 3 | `send_mail_rejects_missing_recipient_and_insufficient_gold` | 收件人不存在（聊天、无 `MailSent`）、金币不足（`MailSent{-1}`）、邮箱满（`>50`） |
| 4 | `send_mail_rejects_unmailable_items_and_rate_limits` | `NoMail`/`DontTrade` 分支差异、10 秒限速的静默与恢复 |
| 5 | `letter_without_attachments_is_collected_and_notifies_online_recipient` | 纯信件 `Collected=true`、在线收件人的新邮件聊天 + `S.ReceiveMail` |
| 6 | `read_mail_marks_opened_and_resends_the_list` | `ReadMail` 无专用回执、`opened`、落盘 Save |
| 7 | `mailbox_list_keeps_only_the_last_hundred_mails` | `MailCapacity=100` 与按 ID 升序 |
| 8 | `collect_requires_post_office_acceptance` | 未受理包裹不能领、聊天 |
| 9 | `post_office_page_accepts_parcels_then_collect_moves_them_to_bag` | 邮局页 `Result=0/-1`、`S.GainedItem`/`S.GainedGold`/`ParcelCollected{1}`、附件清空后可删 |
| 10 | `collect_rejects_when_bag_is_full` | `CanGainItems` 失败、附件不清空 |
| 11 | `locked_mail_cannot_be_deleted_until_unlocked` | 锁定保护、解锁后可删、`MailWrite::Delete` |
| 12 | `mail_with_uncollected_attachments_cannot_be_deleted` | 附件未领（含已受理未领取）不能删 |
| 13 | `mail_cost_request_matches_crystal_formula` | `S.MailCost`：250 / 贴票 0 / 999 金 0 |
| 14 | `mail_locked_item_is_echoed` | `S.MailLockedItem` 回显 |
| 15 | `mail_writes_persist_and_reload_through_memory_store` | `Create/Save/Delete` 全链路 |
| 16 | `session_login_sends_mail_list_and_filters_expired_mail` | 登录包序、30 天过期过滤 + 删除 |
| 17 | `session_send_mail_to_offline_character_by_name` | 按名（大小写不敏感）发给离线角色、落库、附件金币+邮费 |
| 18 | `mail::tests::*`（4 个单元测试） | 邮费、`Collected` 初值、过期判定、字节块往返 |

---

## 11. Crystal 异常行为（照抄或登记）

1. **「简单发信」重载是死代码**：`MirConnection.SendMail` 判断
   `if (p.Gold > 0 || p.ItemsIdx.Length > 0)`（`MirConnection.cs:2007`），而 `ItemsIdx` 是
   **定长 5 的数组**，`Length` 恒为 5 → 网络路径永远走
   `SendMail(name, message, gold, items, stamped)`（`:11727`）；`SendMail(name, message)`
   （`:11674`）只有服务端内部调用才会用到。本作只有一个入口。
2. **限速在校验之前**：`NextMailTime` 在正文/收件人检查之前就置位（`:11735`），
   所以一次失败（例如收件人不存在）也会让接下来 10 秒内的发送全部静默失败。
3. **失败回执不对称**：`DontTrade` 分支只聊天、**不发** `S.MailSent`（`:11800-11804`），
   `NoMail` 分支聊天 + `S.MailSent{-1}`（`:11806-11811`）；客户端如果只刷新金币/背包，
   在 `DontTrade` 下会永远等不到回执。本作照抄。
4. **中途失败会吞物品**：附件循环是「边检查边移除」，若第 1 件成功、第 2 件 `DontTrade`，
   Crystal 直接 `return`（`:11800-11804`），第 1 件已经从背包删掉、又没有进邮件——
   **物品丢失**。本作照抄这个顺序（测试未鼓励该用法）。
5. **`stamped = true` 但玩家没有邮票**：`GetMailCost` 仍按「贴票」免费（`:11930`）；
   后面找不到邮票只是 `hasStamp = false`，附件上限降回 1 件，没有任何提示
   （`:11769-11790`）。本作照抄。
6. **邮箱上限 vs 邮箱容量**：发送前检查 `Mail.Count > 50`（`:11751`），但列表下发取最后
   100 封（`Settings.MailCapacity`，`:11963`）——50~100 封的收件箱能收信但只有部分可见。
7. **`DeleteMail` 没有任何保护**：不检查 `Locked`，也不检查包裹是否已领取
   （`:11904-11913`）；误删带附件的邮件会直接丢东西。本作补了服务端保护（§8）。
8. **`MailLockedItem` 是纯回显**：`MirConnection.cs:667-669` 收到什么就发回什么，
   不落库、不影响任何服务器状态。
9. **`ReadMail` 没有「读信」回执**：只能靠 `S.ReceiveMail` 里 `Opened` 字段判断；
   本作照抄。
10. **`MailInfo.Send` 的 `Collected` 初值**：`MailAutoSendGold/Items` 默认 `false`
    （`Settings.cs:242-243`），所以带附件的邮件一发出就是 `Collected=false`；
    玩家必须先去邮局页面（§5）。
11. **个人仓库没有金币**：`C.DepositGold` / `C.TakeBackGold` / `S.StorageGold` 在
    当前 Crystal 分支里不存在（见 `docs/world/storage.md` §6）；行会金币仓库是另一套包。
12. **Crystal 没有邮件过期**：任务书提到的 `Settings.MailExpireTime` / `S.ParcleExpired`
    在当前分支里都不存在；本作的 30 天过期是**扩展**。
13. **邮件按角色存**：`CharacterInfo.Mail`（`CharacterInfo.cs:85`、`:500-503`），
    不是账号级；同账号的另一个角色看不到。

---

## 12. 未做 / 明确不做

* **行会邮件**（群发全体成员）——不做。
* **商城邮件**（`Envir.MailCharacter` 的购买/拍卖结果，`PlayerObject.cs:13900-13910`、
  `Envir.cs:5069`）——不做。
* **系统公告邮件 / GM 补发**——不做。
* **邮件物品找回**（误删/过期邮件的恢复）——不做。
* **好友黑名单**（`Friends.Blocked`，`:11704-11714`）——没有好友系统。
* **拍卖/租借/交易失败邮件**（`GainItemMail`，`:10912`、`:14251-14272` 等）——对应系统都不存在。
* **`MailAutoSendGold/Items` 的配置化**（`Configs/MailSystem.ini`）——常量固定为默认值。
* **`S.MailSendRequest` 的「未读邮件自动打开」**——Crystal 只有 NPC 页面触发，本作一致。
