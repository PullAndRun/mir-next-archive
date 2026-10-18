//! 好友 / 黑名单（Crystal `CharacterInfo.Friends`，`CharacterInfo.cs:86`；`FriendInfo` `:712`）。
//!
//! Crystal 的 `FriendInfo` 是 `{ int32 Index; bool Blocked; string Memo; }`，黑名单就是
//! `Blocked = true` 的同一条记录；登录/刷新时下发 `S.FriendUpdate`（`:245`，`ClientFriend` 列表）。
//!
//! 本作存储：`characters.friends` 字节块（`int32 count + 每条 { i64 character_id, string name,
//! bool blocked, string memo }`，比 Crystal 多存一个名字，便于离线好友显示）。
//! 运行时挂在 `World.friends` 侧表（按 object id），登录装载、下线写回（session.rs 归户部）。

use std::collections::HashMap;

use mir_protocol::ServerPacket;
use mir_protocol::data::ClientFriend;
use mir_protocol::packets::server::FriendUpdate;
use mir_protocol::{PacketReader, PacketWriter};

use crate::world::World;

/// 好友 + 黑名单总数上限（Crystal 没有显式上限；本作取 100，文档登记为扩展）。
pub const MAX_FRIENDS: usize = 100;

/// 一条好友/黑名单记录（Crystal `FriendInfo` + 角色名扩展）。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FriendEntry {
    /// 好友的角色主键（Crystal `FriendInfo.Index`）。
    pub character_id: i64,
    /// 角色名（Crystal 从 `CharacterInfo` 现查；本作存下来，离线也能显示）。
    pub name: String,
    /// 是否拉黑（Crystal `FriendInfo.Blocked`）。
    pub blocked: bool,
    /// 备注（Crystal `FriendInfo.Memo`，最多 200 字）。
    pub memo: String,
}

/// 好友操作失败原因。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FriendError {
    /// 玩家不在世界里。
    NotInWorld(u32),
    /// 不能加自己。
    CannotAddSelf,
    /// 已经在列表里。
    AlreadyAdded,
    /// 列表已满。
    ListFull,
    /// 不在列表里。
    NotFound,
}

impl std::fmt::Display for FriendError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotInWorld(id) => write!(f, "object {id} is not in the world"),
            Self::CannotAddSelf => f.write_str("you cannot add yourself"),
            Self::AlreadyAdded => f.write_str("the player is already in your friend list"),
            Self::ListFull => f.write_str("friend list limit reached"),
            Self::NotFound => f.write_str("the player is not in your friend list"),
        }
    }
}

/// 编码好友列表（`characters.friends`）。
#[must_use]
pub fn encode_friends(friends: &[FriendEntry]) -> Vec<u8> {
    let mut writer = PacketWriter::new();
    writer.write_count(friends.len());
    for friend in friends {
        writer.write_i64(friend.character_id);
        writer.write_string(&friend.name);
        writer.write_bool(friend.blocked);
        writer.write_string(&friend.memo);
    }
    writer.into_inner()
}

/// 解码好友列表；空块/损坏块按空列表处理（老存档兼容）。
#[must_use]
pub fn decode_friends(bytes: &[u8]) -> Vec<FriendEntry> {
    if bytes.is_empty() {
        return Vec::new();
    }
    let mut reader = PacketReader::new(bytes);
    let Ok(count) = reader.read_count() else {
        return Vec::new();
    };
    let mut friends = Vec::with_capacity(count.min(MAX_FRIENDS));
    for _ in 0..count {
        let Ok(character_id) = reader.read_i64() else {
            return friends;
        };
        let Ok(name) = reader.read_string() else {
            return friends;
        };
        let Ok(blocked) = reader.read_bool() else {
            return friends;
        };
        let Ok(memo) = reader.read_string() else {
            return friends;
        };
        friends.push(FriendEntry {
            character_id,
            name,
            blocked,
            memo,
        });
    }
    friends
}

impl World {
    /// 登录装载：把存档里的好友列表写进世界侧表。
    pub fn set_friends(&mut self, player_id: u32, friends: Vec<FriendEntry>) {
        self.friends.insert(player_id, friends);
    }

    /// 下线存盘：取走好友列表（取走后世界侧为空）。
    #[must_use]
    pub fn take_friends(&mut self, player_id: u32) -> Vec<FriendEntry> {
        self.friends.remove(&player_id).unwrap_or_default()
    }

    /// 当前好友列表。
    #[must_use]
    pub fn friends_of(&self, player_id: u32) -> &[FriendEntry] {
        self.friends
            .get(&player_id)
            .map_or(&[][..], std::vec::Vec::as_slice)
    }

    /// 加好友（`blocked = true` 即加入黑名单；Crystal `PlayerObject.AddFriend`，`:12426-12453`）。
    pub fn add_friend(
        &mut self,
        player_id: u32,
        character_id: i64,
        name: &str,
        blocked: bool,
    ) -> Result<(), FriendError> {
        let Some(player) = self.players.get(&player_id) else {
            return Err(FriendError::NotInWorld(player_id));
        };
        if player.character_id == character_id {
            return Err(FriendError::CannotAddSelf);
        }
        let friends = self.friends.entry(player_id).or_default();
        if friends
            .iter()
            .any(|friend| friend.character_id == character_id)
        {
            return Err(FriendError::AlreadyAdded);
        }
        if self.max_friends != 0 && friends.len() >= self.max_friends {
            return Err(FriendError::ListFull);
        }
        friends.push(FriendEntry {
            character_id,
            name: name.to_owned(),
            blocked,
            memo: String::new(),
        });
        Ok(())
    }

    /// 按角色名加好友（在线玩家；Crystal `Envir.GetCharacterInfo` 支持离线角色，
    /// 本作没有全局角色名索引，因此只支持在线玩家，差异见文档）。
    pub fn add_friend_by_name(
        &mut self,
        player_id: u32,
        name: &str,
        blocked: bool,
    ) -> Result<(), FriendError> {
        let target = self
            .players
            .values()
            .find(|player| player.name.eq_ignore_ascii_case(name))
            .map(|player| (player.character_id, player.name.clone()));
        let Some((character_id, target_name)) = target else {
            return Err(FriendError::NotFound);
        };
        self.add_friend(player_id, character_id, &target_name, blocked)?;
        self.send_friends(player_id);
        Ok(())
    }

    /// 删除好友/黑名单（Crystal `RemoveFriend`，`:12455-12467`）。
    pub fn remove_friend(&mut self, player_id: u32, character_id: i64) -> bool {
        let Some(friends) = self.friends.get_mut(&player_id) else {
            return false;
        };
        let before = friends.len();
        friends.retain(|friend| friend.character_id != character_id);
        friends.len() != before
    }

    /// 设置备注（Crystal `AddMemo`，`:12469-12483`；最多 200 字）。
    pub fn set_friend_memo(&mut self, player_id: u32, character_id: i64, memo: &str) -> bool {
        if memo.is_empty() || memo.chars().count() > 200 {
            return false;
        }
        let Some(friends) = self.friends.get_mut(&player_id) else {
            return false;
        };
        let Some(friend) = friends
            .iter_mut()
            .find(|friend| friend.character_id == character_id)
        else {
            return false;
        };
        memo.clone_into(&mut friend.memo);
        true
    }

    /// 给玩家下发整张好友表（Crystal `GetFriends` → `S.FriendUpdate`，`:12485-12498`）。
    pub fn send_friends(&mut self, player_id: u32) {
        let online_ids: Vec<i64> = self
            .players
            .values()
            .map(|player| player.character_id)
            .collect();
        let friends: Vec<ClientFriend> = self
            .friends
            .get(&player_id)
            .map(|list| {
                list.iter()
                    .map(|friend| ClientFriend {
                        index: i32::try_from(friend.character_id).unwrap_or(0),
                        name: friend.name.clone(),
                        memo: friend.memo.clone(),
                        blocked: friend.blocked,
                        online: online_ids.contains(&friend.character_id),
                    })
                    .collect()
            })
            .unwrap_or_default();
        let frame = ServerPacket::FriendUpdate(FriendUpdate { friends }).encode_frame();
        self.send_to_player(player_id, &frame);
    }
}

/// 以 `HashMap` 承载的便捷构造（测试/序列化用）。
#[must_use]
pub fn friends_map(entries: Vec<(u32, Vec<FriendEntry>)>) -> HashMap<u32, Vec<FriendEntry>> {
    entries.into_iter().collect()
}
