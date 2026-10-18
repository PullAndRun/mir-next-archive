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

## 5. 测试

`crates/mir-server/tests/social.rs` 7 条：好友编解码往返、加/删/拉黑、上限、
联盟双向与记录往返、联盟上限、盟友判定、`S.FriendUpdate` 包内容。
