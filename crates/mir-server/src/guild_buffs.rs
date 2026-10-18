//! 行会增益（guild buff）：购买 / 激活 / 到期 / 属性加成。
//!
//! Crystal 的模型分三层：
//!
//! * **定义**：`GuildBuffInfo`（`Shared/Data/GuildData.cs:92-215`），从
//!   `Configs/GuildSettings.ini` 的 `[Guilds] TotalBuffs` + `[Buff-<i>]` 段读入
//!   （`Server/Settings.cs:1371-1375`），见 [`crate::guild::GuildSettings`]。
//! * **行会持有**：`GuildInfo.BuffList`（`List<GuildBuff>`，`Server/MirDatabase/GuildInfo.cs:23`），
//!   每条只有 `Id` / `Active` / `ActiveTimeRemaining`（单位**分钟**）；`Info` 是运行时按
//!   `Id` 查出来的（`GuildObject.cs:150`、`PlayerObject.cs:10367`）。
//! * **玩家属性**：`PlayerObject.RefreshGuildBuffs` 把**所有 `Active` 增益的 `Info.Stats`**
//!   无脑累加进 `Stats`（`PlayerObject.cs:1824-1836`），随后由 `RefreshStats` 统一生效
//!   （`HumanObject.cs:1730-1769`）。它**不**创建 `MapObject.Buffs` 里的 `Buff`，
//!   所以客户端看不到 `S.AddBuff`。
//!
//! # 包序
//!
//! | 语义 | 客户端包 | 服务端包 | Crystal |
//! |---|---|---|---|
//! | 请求可选列表 | `C.GuildBuffUpdate{Action=0, Id=0}`（131） | `S.GuildBuffList{GuildBuffs=全部}`（248） | `PlayerObject.cs:10357-10360` |
//! | 登录时下发可选列表 | — | 同上一行（无条件，所有玩家） | `PlayerObject.cs:1223-1224` |
//! | 购买 | `{Action=1, Id}` | `S.GuildStorageGoldChange{Type=2, Name="", Amount=金币}`（有金币消耗时）→ `S.GuildBuffList{ActiveBuffs=[新增益]}` → `S.GuildStatus` ×在线成员 → 行会频道成功消息 | `GuildObject.cs:912-983`、`PlayerObject.cs:10361-10419` |
//! | 激活（到期后重开） | `{Action=2, Id}` | `S.GuildBuffList{ActiveBuffs=[增益]}` → `S.GuildStorageGoldChange{Type=2, Name="", Amount=金币}` | `GuildObject.cs:992-1017`、`PlayerObject.cs:10420-10434` |
//! | 到期 | — | `S.GuildBuffList{ActiveBuffs=[Active=false 的增益]}` | `GuildObject.cs:796-842` |
//! | 加入行会 | — | `S.GuildBuffList{ActiveBuffs=行会已有增益}` | `PlayerObject.cs:10009-10010` |
//!
//! # 时间单位（容易看错）
//!
//! `GuildBuffInfo.TimeLimit` 与 `GuildBuff.ActiveTimeRemaining` 的单位都是**分钟**：
//! `GuildObject.Process` 每分钟被 `Envir.Process` 调一次（`Envir.cs:2330-2337`），
//! 每次把 `ActiveTimeRemaining` 减 1，`< 0` 才置 `Active = false`（`GuildObject.cs:806-814`）。
//! 客户端文案也叫 `BuffLengthMinutes` / `TimeRemainingMinutes`
//! （`Client/MirScenes/Dialogs/GuildDialog.cs:1237`、`:1241`）。
//! `TimeLimit == 0` 表示**永久**增益，`Process` 完全不碰它（`:803`）。
//!
//! # 本作与 Crystal 的差异
//!
//! * 到期会排一条落盘写入（Crystal 的 `Process` 不置 `NeedSave`，只有关服强制保存才写入，
//!   崩溃时已到期的增益会在重启后复活；本作修正这一点）。
//! * 登录时补发 `ActiveBuffs`（Crystal 只在加入行会时发，重登后客户端不知道哪些增益还开着）。
//! * `RequestedGuildBuffInfo` 与 `GuildCanRequestItems` 一样放在世界上（按对象 ID），
//!   而不是 `Player` 字段，见 `crate::world::World`。

use mir_core::enums::GuildRankOptions;
use mir_protocol::ServerPacket;
use mir_protocol::data::{GuildBuff, GuildBuffInfo};
use mir_protocol::packets::server::GuildBuffList;

use crate::combat::CombatStats;
use crate::guild::{GuildError, GuildRef, has_option};
use crate::world::{MINUTE_MS, World};

/// 一条 `S.GuildBuffList`（包号 248）。
fn list_frame(
    remove: u8,
    active_buffs: Vec<GuildBuff>,
    guild_buffs: Vec<GuildBuffInfo>,
) -> Vec<u8> {
    ServerPacket::GuildBuffList(GuildBuffList {
        remove,
        active_buffs,
        guild_buffs,
    })
    .encode_frame()
}

/// 一条 `S.GuildStorageGoldChange{Type=2}`（包号 172）：只有金额，没有聊天文案
/// （客户端 `GameScene.cs:5923-5928` 直接从行会窗口扣除）。
fn gold_spent_frame(amount: u32) -> Vec<u8> {
    ServerPacket::GuildStorageGoldChange(mir_protocol::packets::server::GuildStorageGoldChange {
        amount,
        type_: 2,
        name: String::new(),
    })
    .encode_frame()
}

/// 把行会增益的属性叠进 `stats`（Crystal `PlayerObject.RefreshGuildBuffs`，
/// `PlayerObject.cs:1824-1836`）。
///
/// ```csharp
/// if (MyGuild == null) return;
/// if (MyGuild.BuffList.Count == 0) return;
/// foreach (var buff in MyGuild.BuffList)
/// {
///     if ((buff.Info == null) || (!buff.Active)) continue;
///     Stats.Add(buff.Info.Stats);
/// }
/// ```
///
/// 由 [`crate::items::World::refresh_stats`] 在装备之后、`apply_caps` 之前调用
/// （Crystal 的 `RefreshGuildBuffs` 排在 `RefreshBuffs` 之后、百分比加成之前，
/// `HumanObject.cs:1746-1751`）。
pub(crate) fn apply_guild_buffs(
    world: &World,
    guild_ref: Option<GuildRef>,
    stats: &mut CombatStats,
) {
    let Some(reference) = guild_ref else {
        return;
    };
    let Some(guild) = world.guilds.get(&reference.guild_id) else {
        return;
    };
    if guild.buffs.is_empty() {
        return;
    }
    let buff_stats = guild.active_buff_stats(&world.guild_settings);
    for (&stat, &value) in buff_stats.values() {
        crate::items::add_stat(stats, stat, value);
    }
}

impl World {
    /// 下发「全部可选增益」列表（Crystal `StartGame` 的
    /// `Enqueue(new S.GuildBuffList { GuildBuffs = Settings.Guild_BuffList })`，
    /// `PlayerObject.cs:1223-1224`），并记下「已经发过」
    /// （`RequestedGuildBuffInfo = true`，`:1224`）。
    ///
    /// 没有行会也发——Crystal 就是在 `StartGame` 里无条件发的。
    pub fn send_guild_buff_info(&mut self, player_id: u32) {
        let frame = list_frame(0, Vec::new(), self.guild_settings.buff_list.clone());
        self.send_to_player(player_id, &frame);
        self.guild_buff_info_sent.insert(player_id);
    }

    /// 把行会已有的增益发给某个玩家（Crystal 加入行会时的
    /// `Enqueue(new S.GuildBuffList { ActiveBuffs = MyGuild.BuffList })`，`PlayerObject.cs:10009-10010`）。
    ///
    /// 行会没有增益时**什么也不发**（Crystal 的 `if (MyGuild.BuffList.Count > 0)`）。
    pub fn send_active_guild_buffs(&mut self, player_id: u32) {
        let Some(reference) = self.players.get(&player_id).and_then(|player| player.guild) else {
            return;
        };
        let active_buffs = self
            .guilds
            .get(&reference.guild_id)
            .map(|guild| guild.buffs.clone())
            .unwrap_or_default();
        if active_buffs.is_empty() {
            return;
        }
        self.send_to_player(player_id, &list_frame(0, active_buffs, Vec::new()));
    }

    /// `C.GuildBuffUpdate`（Crystal `PlayerObject.GuildBuffUpdate`，
    /// `PlayerObject.cs:10350-10436`）。
    ///
    /// `action`：`0` 拉列表、`1` 购买、`2` 激活。`id < 0` 与不在行会都静默返回
    /// （`:10352-10354`）。
    pub fn guild_buff_update(
        &mut self,
        player_id: u32,
        action: u8,
        id: i32,
    ) -> Result<(), GuildError> {
        if !self.players.contains_key(&player_id) {
            return Err(GuildError::NotInWorld(player_id));
        }
        // `if (MyGuild == null) return; if (MyGuildRank == null) return; if (id < 0) return;`
        // （`PlayerObject.cs:10352-10354`）。
        let Some((guild_id, rank_index)) = self
            .players
            .get(&player_id)
            .and_then(|player| player.guild)
            .map(|reference| (reference.guild_id, reference.rank_index))
        else {
            return Ok(());
        };
        if id < 0 {
            return Ok(());
        }
        let options = self
            .guilds
            .get(&guild_id)
            .and_then(|guild| guild.rank(rank_index))
            .map_or(0, |rank| rank.options);

        match action {
            // `case 0: if (RequestedGuildBuffInfo) return; Enqueue(new S.GuildBuffList { GuildBuffs = ... });`
            // （`:10357-10360`）。登录时已经发过（`:1223-1224`），所以正常会话里永远走不到。
            0 => {
                if self.guild_buff_info_sent.contains(&player_id) {
                    return Ok(());
                }
                // Crystal 的这个分支**只发包、不置** `RequestedGuildBuffInfo`
                // （`:10357-10360` 只有 `Enqueue`；置位在登录处 `:1224`），
                // 所以这里不能复用会置位的 [`World::send_guild_buff_info`]。
                let frame = list_frame(0, Vec::new(), self.guild_settings.buff_list.clone());
                self.send_to_player(player_id, &frame);
                Ok(())
            }
            1 => self.guild_buff_buy(player_id, guild_id, options, id),
            2 => self.guild_buff_activate(player_id, guild_id, options, id),
            _ => Ok(()),
        }
    }

    /// 购买增益（Crystal `GuildObject.NewBuff`，`GuildObject.cs:912-983` +
    /// `PlayerObject.cs:10361-10419` 的校验与成功消息）。
    fn guild_buff_buy(
        &mut self,
        player_id: u32,
        guild_id: i32,
        options: u8,
        id: i32,
    ) -> Result<(), GuildError> {
        // `if (!MyGuildRank.Options.HasFlag(GuildRankOptions.CanActivateBuff))`（`:10362-10366`）。
        if !has_option(options, GuildRankOptions::CanActivateBuff) {
            let error = GuildError::NoBuffPermission;
            self.guild_error_to(player_id, &error);
            return Err(error);
        }
        // `GuildBuffInfo BuffInfo = Envir.FindGuildBuffInfo(id);`（`:10367-10372`）。
        let Some(info) = self.guild_settings.guild_buff_info(id).cloned() else {
            let error = GuildError::BuffNotExist(id);
            self.guild_error_to(player_id, &error);
            return Err(error);
        };
        // `if (MyGuild.GetBuff(id) != null)`（`:10373-10377`）。
        let already_obtained = self
            .guilds
            .get(&guild_id)
            .is_some_and(|guild| guild.buff(id).is_some());
        if already_obtained {
            let error = GuildError::BuffAlreadyObtained(id);
            self.guild_error_to(player_id, &error);
            return Err(error);
        }
        let (guild_level, spare_points, guild_gold) =
            self.guilds.get(&guild_id).map_or((0, 0, 0), |guild| {
                (guild.level, guild.spare_points, guild.gold)
            });
        // `if (MyGuild.Info.Level < BuffInfo.LevelRequirement)`（`:10378-10382`）。
        if guild_level < info.level_requirement {
            let error = GuildError::GuildLevelTooLow {
                required: info.level_requirement,
                level: guild_level,
            };
            self.guild_error_to(player_id, &error);
            return Err(error);
        }
        // `if (MyGuild.Info.SparePoints < BuffInfo.PointsRequirement)`（`:10383-10387`）。
        if spare_points < info.points_requirement {
            let error = GuildError::SparePointsInsufficient {
                required: info.points_requirement,
                remaining: spare_points,
            };
            self.guild_error_to(player_id, &error);
            return Err(error);
        }
        // `bool requiresGold = BuffInfo.TimeLimit > 0 && BuffInfo.ActivationCost > 0;`（`:10389`）。
        let requires_gold = info.time_limit > 0 && info.activation_cost > 0;
        let cost = u32::try_from(info.activation_cost).unwrap_or(0);
        if requires_gold && guild_gold < cost {
            let error = GuildError::GuildBankNotEnough {
                required: cost,
                gold: guild_gold,
            };
            self.guild_error_to(player_id, &error);
            return Err(error);
        }
        if !self.guild_new_buff(guild_id, &info) {
            // Crystal `NewBuff` 返回 false 时**没有**任何提示（`:10399`）。
            tracing::debug!(
                guild = guild_id,
                id,
                "购买行会增益失败（NewBuff 返回 false）"
            );
            return Ok(());
        }
        // 成功消息（`PlayerObject.cs:10400-10418`）：四种文案按「有没有金币/点数消耗」选。
        let player_name = self
            .players
            .get(&player_id)
            .map_or_else(String::new, |player| player.name.clone());
        let has_point_cost = info.points_requirement > 0;
        let message = if requires_gold && has_point_cost {
            format!(
                "「{player_name}」花费 {} 升级点与 {cost} 金币为行会购买了增益「{}」。",
                info.points_requirement, info.name
            )
        } else if requires_gold {
            format!(
                "「{player_name}」花费 {cost} 金币为行会购买了增益「{}」。",
                info.name
            )
        } else if has_point_cost {
            format!(
                "「{player_name}」花费 {} 升级点为行会购买了增益「{}」。",
                info.points_requirement, info.name
            )
        } else {
            format!("「{player_name}」为行会购买了增益「{}」。", info.name)
        };
        self.send_guild_message(guild_id, &message);
        Ok(())
    }

    /// 重新激活一个已获得、已过期（或 `TimeLimit == 0` 时首次）的增益
    /// （Crystal `GuildObject.ActivateBuff`，`GuildObject.cs:992-1017` +
    /// `PlayerObject.cs:10420-10434`）。
    fn guild_buff_activate(
        &mut self,
        player_id: u32,
        guild_id: i32,
        options: u8,
        id: i32,
    ) -> Result<(), GuildError> {
        if !has_option(options, GuildRankOptions::CanActivateBuff) {
            let error = GuildError::NoBuffPermission;
            self.guild_error_to(player_id, &error);
            return Err(error);
        }
        let Some(info) = self.guild_settings.guild_buff_info(id).cloned() else {
            // Crystal 在 `GetBuff` 之后直接读 `Buff.Info.ActivationCost`，`Info == null` 会空引用；
            // 我们把「配置里没这个增益」当成静默失败（`:10426-10432` 的防御版本）。
            return Ok(());
        };
        let Some(buff) = self
            .guilds
            .get(&guild_id)
            .and_then(|guild| guild.buff(id))
            .cloned()
        else {
            let error = GuildError::BuffNotObtained(id);
            self.guild_error_to(player_id, &error);
            return Err(error);
        };
        let guild_gold = self.guilds.get(&guild_id).map_or(0, |guild| guild.gold);
        let cost = u32::try_from(info.activation_cost).unwrap_or(0);
        // `if ((MyGuild.Gold < Buff.Info.ActivationCost) || (Buff.Active)) return;`（`:10432`）——
        // 静默返回，没有聊天也没有包。
        if buff.active || guild_gold < cost {
            return Ok(());
        }

        // `buff.Active = true; buff.ActiveTimeRemaining = buff.Info.TimeLimit;`
        // `Info.Gold -= (uint)buff.Info.ActivationCost;`（`GuildObject.cs:1001-1004`）。
        let activated = {
            let Some(guild) = self.guilds.get_mut(&guild_id) else {
                return Err(GuildError::NotInGuild(player_id));
            };
            let Some(buff) = guild.buffs.iter_mut().find(|buff| buff.id == id) else {
                return Err(GuildError::BuffNotObtained(id));
            };
            buff.active = true;
            buff.active_time_remaining = info.time_limit;
            guild.gold = guild.gold.saturating_sub(cost);
            buff.clone()
        };

        // `SendServerPacket(new S.GuildBuffList { ActiveBuffs = NewBuff });`
        // `SendServerPacket(new S.GuildStorageGoldChange { Type = 2, Name = "", Amount = ... });`
        // （`GuildObject.cs:1011-1012`）——顺序：先列表，后金币。
        let frame = list_frame(0, vec![activated], Vec::new());
        self.send_to_guild(guild_id, &frame);
        let frame = gold_spent_frame(cost);
        self.send_to_guild(guild_id, &frame);
        self.mark_guild_dirty(guild_id);
        self.refresh_guild_stats(guild_id);
        Ok(())
    }

    /// Crystal `GuildObject.NewBuff`（`GuildObject.cs:912-983`）：扣点、扣钱、登记、发包、刷新。
    ///
    /// 返回 `false` 表示点数/金币不够或编号无效（与 Crystal 的返回值语义一致）。
    fn guild_new_buff(&mut self, guild_id: i32, info: &GuildBuffInfo) -> bool {
        let Some(guild) = self.guilds.get(&guild_id) else {
            return false;
        };
        if guild.buff(info.id).is_some() {
            return false;
        }
        // `if (Info.SparePoints < info.PointsRequirement) return false;`（`:928-929`）。
        if guild.spare_points < info.points_requirement {
            return false;
        }
        let requires_gold = info.time_limit > 0 && info.activation_cost > 0;
        let cost = u32::try_from(info.activation_cost).unwrap_or(0);
        // `if (info.TimeLimit > 0 && info.ActivationCost > 0) { if (Info.Gold < activationCost) return false; }`
        // （`:931-937`）。
        if requires_gold && guild.gold < cost {
            return false;
        }

        // `buff.ActiveTimeRemaining = buff.Info.TimeLimit;`（`:946`）。
        let buff = GuildBuff {
            id: info.id,
            active: true,
            active_time_remaining: info.time_limit,
        };
        let Some(guild) = self.guilds.get_mut(&guild_id) else {
            return false;
        };
        // `ChargeForBuff(buff)`：`Info.SparePoints -= buff.Info.PointsRequirement;`（`:985-990`）。
        guild.spare_points = guild.spare_points.saturating_sub(info.points_requirement);
        // 金币消耗（`:952-957`）。
        if requires_gold {
            guild.gold = guild.gold.saturating_sub(cost);
        }
        guild.buffs.push(buff.clone());

        // 金币包在增益列表**之前**：Crystal `NewBuff` 在 `:952-957` 结算金币并发包，
        // 到 `:961-966` 才发 `S.GuildBuffList`。
        if requires_gold {
            // `SendServerPacket(new S.GuildStorageGoldChange { Type = 2, Name = "", Amount = activationCost });`
            // （`:956`）。
            let frame = gold_spent_frame(cost);
            self.send_to_guild(guild_id, &frame);
        }
        // `SendServerPacket(new S.GuildBuffList { ActiveBuffs = NewBuff });`（`:961-966`）。
        let frame = list_frame(0, vec![buff], Vec::new());
        self.send_to_guild(guild_id, &frame);
        // 「now tell everyone our new sparepoints」→ 逐人 `SendGuildStatus`（`:969-978`）。
        self.send_guild_status_to_all(guild_id);
        self.mark_guild_dirty(guild_id);
        // `RefreshAllStats();`（`:981`）。
        self.refresh_guild_stats(guild_id);
        true
    }

    /// 给行会全体**在线**成员刷新一次属性（Crystal `GuildObject.RefreshAllStats`，`:780-793`）。
    pub fn refresh_guild_stats(&mut self, guild_id: i32) {
        let online = self
            .guilds
            .get(&guild_id)
            .map(crate::guild::Guild::online_members)
            .unwrap_or_default();
        for object_id in online {
            self.refresh_stats(object_id);
        }
    }

    /// 给行会全体在线成员发一条**行会频道**消息
    /// （Crystal `GuildObject.SendMessage` 默认 `ChatType.Guild`，`GuildObject.cs:113-126`）。
    fn send_guild_message(&mut self, guild_id: i32, message: &str) {
        let online = self
            .guilds
            .get(&guild_id)
            .map(crate::guild::Guild::online_members)
            .unwrap_or_default();
        for object_id in online {
            self.send_guild_chat_to(object_id, message);
        }
    }

    /// 行会增益每分钟结算一次（Crystal `GuildObject.Process`，`GuildObject.cs:796-842`）。
    ///
    /// 只做「有到期增益的」处理：`TimeLimit == 0`（永久）、配置里已删除（`Info == null`）、
    /// 未激活的都跳过；其余 `ActiveTimeRemaining -= 1`，`< 0` 时置 `Active = false`
    /// 并广播 `S.GuildBuffList{ActiveBuffs=[…]}`，随后给在线成员重算属性。
    ///
    /// 返回本次到期的增益条数（测试用；Crystal 没有返回值）。
    pub fn process_guild_buffs(&mut self) -> usize {
        // 先把「有时间限制的增益编号 → 分钟数」取出来，避免同时借用 `guilds` 与 `guild_settings`。
        // 重复编号取**第一次**出现的定义（Crystal `FindGuildBuffInfo` 也是这样扫的，
        // `Envir.cs:5026-5037`）。
        let mut limits: std::collections::HashMap<i32, i32> = std::collections::HashMap::new();
        for info in &self.guild_settings.buff_list {
            limits.entry(info.id).or_insert(info.time_limit);
        }
        let mut guild_ids: Vec<i32> = self.guilds.keys().copied().collect();
        guild_ids.sort_unstable();
        let mut expired = 0;
        for guild_id in guild_ids {
            let Some(guild) = self.guilds.get_mut(&guild_id) else {
                continue;
            };
            let mut updated: Vec<GuildBuff> = Vec::new();
            for buff in &mut guild.buffs {
                // `if ((Info == null) || (Info.TimeLimit == 0)) continue;`（`:803`）。
                let Some(limit) = limits.get(&buff.id).copied() else {
                    continue;
                };
                if limit == 0 {
                    continue;
                }
                // `if (BuffList[k].Active == false) continue;`（`:804`）。
                if !buff.active {
                    continue;
                }
                // `BuffList[k].ActiveTimeRemaining -= 1;`（`:806`）——配置里若写了负数，
                // `saturating_sub` 与 C# 的 int 下溢在「< 0 就失效」的判定上等价。
                buff.active_time_remaining = buff.active_time_remaining.saturating_sub(1);
                // `if (BuffList[k].ActiveTimeRemaining < 0)`（`:808`）。
                if buff.active_time_remaining < 0 {
                    buff.active = false;
                    updated.push(buff.clone());
                }
            }
            if updated.is_empty() {
                continue;
            }
            expired += updated.len();
            // `SendServerPacket(new S.GuildBuffList { ActiveBuffs = UpdatedBuffs });`（`:821`）。
            let frame = list_frame(0, updated, Vec::new());
            self.send_to_guild(guild_id, &frame);
            // Crystal 的 `Process` **没有**置 `NeedSave`；本作修正为落盘（见模块文档）。
            self.mark_guild_dirty(guild_id);
            // `RefreshAllStats();`（`:824`）。
            self.refresh_guild_stats(guild_id);
        }
        expired
    }

    /// 按服务器时钟节流的行会增益结算（Crystal `guildTime` 每分钟触发一次，
    /// `Envir.cs:2330-2337`）。
    pub(crate) fn tick_guild_buffs(&mut self, now: i64) {
        if now < self.next_guild_buff_tick {
            return;
        }
        self.next_guild_buff_tick = now.saturating_add(MINUTE_MS);
        self.process_guild_buffs();
    }
}
