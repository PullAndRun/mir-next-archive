# 好友与黑名单（第 7 轮新增）

实现：`crates/mir-server/src/friends.rs`（编解码、增删改、下发），持久化列
`characters.friends`（迁移 `0015_guild_allies_and_friends.sql`）。

## 1. 数据模型

一条记录 = Crystal `FriendInfo` + 角色名扩展：

| 字段 | 类型 | 说明 |
|---|---|---|
| `character_id` | `i64` | 角色主键（Crystal `FriendInfo.Index`） |
| `name` | `String` | 角色名（Crystal 现查 `CharacterInfo`；本作存下来，离线也能显示） |
| `blocked` | `bool` | `true` = 黑名单（Crystal 也是同一个列表） |
| `memo` | `String` | 备注，超过 200 字会拒绝（`set_friend_memo` 返回 `false`） |

**上限**：好友 + 黑名单合计 **100** 条（`friends::MAX_FRIENDS`）。Crystal 的
`CharacterInfo.Friends` 是 `List<FriendInfo>`、没有显式上限——**本作扩展**。

存档编码（`characters.friends`，BYTEA，空块 = 空列表）：

```text
i32 count
每条：i64 character_id + string name + bool blocked + string memo
```

编解码：`friends::encode_friends` / `decode_friends`；空块/损坏块按空列表处理（老存档兼容）。

## 2. 协议

| 方向 | 包 | index | 语义 |
|---|---|---|---|
| C→S | `C.AddFriend{Name, Blocked}` | 127 | 按角色名加好友；`Blocked=true` 直接进黑名单 |
| C→S | `C.RemoveFriend{CharacterIndex}` | 128 | 按角色主键删除 |
| C→S | `C.RefreshFriends{}` | 129 | 请求整表刷新 |
| C→S | `C.AddMemo{CharacterIndex, Memo}` | 130 | 设置备注（≤200 字） |
| S→C | `S.FriendUpdate{Friends}` | 245 | 整表下发 `ClientFriend{index, name, memo, blocked, online}` |

`online` 由世界侧在线角色比对；登录、增删、改备注、刷新都会整表下发。**没有**独立的
`FriendList` / `BlockList` 包（Crystal 同样只有一个列表）。

## 3. 行为与差异

* 加好友只支持**在线**玩家（本作没有全局角色索引）；Crystal 可按角色名加离线角色——
  **有意差异**；
* 不能加自己；重复添加、列表满都会拒绝（`FriendError`）；
* 黑名单目前只做标记与展示，**聊天/交易拦截未做**（Crystal 的拉黑过滤未移植）；
* 好友上下线不主动推送，靠 `C.RefreshFriends` 重新拉整表（`online` 会更新）。

## 4. 持久化

`characters.friends`（迁移 0015）：登录解码进 `World.friends` 侧表；下线编码写回；
市场/商城落盘快照也带 `friends`（避免旧值覆盖）。与 `characters.pet_mode`、
`quest_inventory` 同一套「一列一块」约定。

## 5. 客户端（第 8 轮新增）

**好友面板（`F`）**：`ui::friends`，380×300、小屏自动压缩；列表行含名字/备注/在线
（绿）/黑名单标记，滚轮滚动、点行选中；底部可加好友/拉黑、改备注、删除、刷新，
分别对应 §2 的 127/130/128/129。Crystal 也有 `Client/MirScenes/Dialogs/FriendDialog.cs`
（标题 199、12 行、好友/黑名单两 tab、Add/Remove/Memo/Email/Whisper，
`KeybindOptions.Friends` 可配键位）；本作是独立轻量面板，固定 **F** 键，暂不做
Email/Whisper 快捷按钮（用聊天/邮件面板代替）。

**联盟页签**：行会面板（`G`）第 4 个页签 `GuildTab::Ally`（公告页顺延到 index 4），
输入行会名/ID + 申请/同意/解除三个按钮，发送 `/ally request|accept|remove` 聊天命令。
名单由 **`S.GuildAllies`(303)** 权威整表下发（登录/结盟/解除/对方解散时刷新，最多 5 条），
客户端整表替换、不做乐观更新；对方行会解散时服务端会清掉反向链接（第 8 轮修复）。

细节（布局、按键、包对照）见 [`docs/client.md`](../client.md) §5.20；协议见
[`docs/world/guild.md`](guild.md) §15.3。

## 6. 婚姻 / 师徒（第 14 轮）

**持久化字段来自迁移 0001，本作未新增迁移**：`characters` 表在
`0001_init.sql:102-108` 就有 `married` / `married_date` / `mentor` / `mentor_date` /
`is_mentor` / `mentor_exp`（对齐 Crystal `CharacterInfo` 的社交字段）；当前迁移最新为
`0016_conquest_objects.sql`，**没有 0017**。详见
[`docs/store/schema.md`](../store/schema.md) §11。

**配偶传送（第 14 轮，本作扩展；Crystal 对应 `@RECALLLOVER`，`PlayerObject.cs:2577-2700`）**：

* **核心**：`World::spouse_teleport(player)` —— 未结婚 / 配偶离线 / 配偶死亡 / 无空位 →
  `SocialError`（全部走 TextKey）；成功则 `free_cell_next_to()` 在配偶周围 **9 格**里取第一个
  「可走且无人占」的格子（配偶自身格作为最后兜底候选）；
* **两条入口**：
  1. **结婚戒指**：`C.UseItem` 的 `ItemType::Ring && item.wedding_ring >= 0` 分支
     （Crystal `UserItem.WeddingRing == Married`，`:2596-2604`）；成功回
     `S.UseItem{success}` 并把落点写进 `UseItemReport.teleported`；
  2. **`/spouse` 聊天命令**：`session.rs` 入口拦截，**本作扩展**、不进频道；
* **包**：只发 `S.MapChanged`（复用 `World::change_map`，**同图/跨图都发**）+ 戒指路径的
  `S.UseItem` + 系统 `S.Chat` 提示；**没有新增协议包**；
* **简化**：`spouse_recall_needs_ring` 目前只登记、未在流程里强制——本作把戒指当**触发器**
  而不是前置条件；未做强制「必须先戴戒指」的校验。

测试（`tests/social.rs` 24/24 中的 4 条）：`spouse_teleport_moves_next_to_the_spouse_same_map`、
`spouse_teleport_rejects_unmarried_and_offline_spouse`、
`spouse_teleport_crosses_maps_when_the_spouse_is_elsewhere`、
`wedding_ring_use_teleports_to_the_spouse`。

## 7. 测试

`crates/mir-server/tests/social.rs` **24 条**：好友编解码往返、加/删/拉黑、上限、
联盟双向与记录往返、联盟上限、盟友判定、`S.FriendUpdate` 包内容、好友/联盟上限
可配置（`0` = 不限）、`/ally` 回执本地化（`ally_replies_are_localized`）、
配偶传送 4 条（同图/跨图/拒绝/戒指）。
客户端 `crates/mir-client/tests/friends_panel.rs` 另 **6 条**：320×240 布局与控件不重叠、
`row_at(scroll)` 绝对下标、滚动后选中第 7 条并删除、127–130 映射与字节编码、
245 整表替换 + 中英切换、渲染截图（`client-friends.png`/`client-ally.png`）。
