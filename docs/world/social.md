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

## 6. 测试

`crates/mir-server/tests/social.rs` **10 条**：好友编解码往返、加/删/拉黑、上限、
联盟双向与记录往返、联盟上限、盟友判定、`S.FriendUpdate` 包内容、好友/联盟上限
可配置（`0` = 不限）、`/ally` 回执本地化（`ally_replies_are_localized`）。
客户端 `crates/mir-client/tests/friends_panel.rs` 另 **6 条**：320×240 布局与控件不重叠、
`row_at(scroll)` 绝对下标、滚动后选中第 7 条并删除、127–130 映射与字节编码、
245 整表替换 + 中英切换、渲染截图（`client-friends.png`/`client-ally.png`）。
