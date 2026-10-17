//! 增益（buff）容器：登记 / 属性叠加 / 到期清理 / 包广播。
//!
//! Crystal 把 buff 放在 `MapObject.Buffs`（`Server/MirObjects/MapObject.cs:49`），
//! 每个 `Buff` 有「剩余时长」`ExpireTime`、属性表 `Stats`、施法者 `Caster` 与
//! 每秒一次的 `ProcessBuffs` 到期扫描（`Server/MirObjects/HumanObject.cs:397-494`）。
//! 本模块把它移植成一个可枚举的 [`ActiveBuff`] 列表，挂在
//! [`Player::buffs`](crate::world::Player::buffs) 上。
//!
//! | 本模块 | Crystal |
//! |---|---|
//! | [`BuffKind`] | `BuffType`（`Shared/Enums.cs:1334`）里本作用到的子集 |
//! | [`ActiveBuff`] | `Buff`（`Server/MirDatabase/BuffInfo.cs:90-272`） |
//! | [`World::add_buff`] | `MapObject.AddBuff`（`MapObject.cs:570-670`）+ `HumanObject.AddBuff`（`HumanObject.cs:7460-7489`） |
//! | [`World::remove_buff`] | `MapObject.RemoveBuff`（`MapObject.cs:672-694`）+ `HumanObject.ProcessBuffs` 的移除分支（`HumanObject.cs:452-491`） |
//! | `World::tick_buffs` | `HumanObject.ProcessBuffs`（`HumanObject.cs:397-494`） |
//! | `World::shave_buff` | 受击扣时长（`HumanObject.cs:7163-7167`、`:7273-7277`） |
//! | 属性叠加 | `HumanObject.RefreshBuffs`（`HumanObject.cs:2320-2350`） |
//!
//! # 到期语义
//!
//! Crystal 的 `Buff.ExpireTime` 是**剩余时长**，`ProcessBuffs` 每秒按真实流逝时间扣减
//! （`HumanObject.cs:439-448`），所以最短可观测的到期粒度是 1 秒（`NextTime`）。
//! mir-next 直接存**绝对到期时刻** `expire_at`（服务器单调时钟），每 tick 精确判定，
//! 因此到期比 Crystal 早最多 1 秒——这是有意的简化，好处是测试可精确断言（见 `docs/world/buffs.md`）。
//!
//! # 属性叠加
//!
//! `HumanObject.RefreshStats` 的调用顺序是
//! `RefreshLevelStats` → `RefreshEquipmentStats` → `RefreshItemSetStats` → … →
//! `RefreshBuffs` → `RefreshGuildBuffs` → `RefreshStatCaps`（`HumanObject.cs:1730-1769`）。
//! 本作在 `World::refresh_stats`（`crate::items`）里按同样的位置
//! 把 buff 属性叠在装备之后、`apply_caps` 之前。
//!
//! # 叠加规则（`BuffStackType`）
//!
//! 所用到的 9 个技能增益全部是 `ResetDuration`（`Server/MirDatabase/BuffInfo.cs:19-43`），
//! 药水增益（`Impact`/`Magic`/…）是 `StackDuration`（`BuffInfo.cs:74-83`）。
//! Crystal 的 `ResetDuration` 只重置时长、**不刷新属性**（`MapObject.cs:591-595`），
//! 所以重复施放同一个护盾时沿用的是第一次的加成值；本作照抄该行为。

use mir_core::Point;
use mir_core::enums::{BuffType, SpellEffect, Stat};
use mir_protocol::ServerPacket;
use mir_protocol::data::{ClientBuff, Stats};
use mir_protocol::packets::server::{AddBuff, ObjectEffect, RemoveBuff};

use crate::world::{MINUTE_MS, OBJECT_HEALTH_EXPIRE_SECONDS, World};

/// 一秒的毫秒数：Crystal `Settings.Second = 1000`（`Server/Settings.cs:16`）。
pub const SECOND_MS: i64 = 1_000;

/// `SoulShield` / `BlessedArmour` 的作用半径（含中心共 7×7）：
/// `Map.cs:1052-1057` 的 `location.X-3 .. location.X+3`。
pub const SHIELD_AREA_RADIUS: i32 = 3;

/// `EnergyShield` 的触发概率下限：`if (chance < 2) chance = 2;`（`HumanObject.cs:4762`）。
pub const ENERGY_SHIELD_MIN_CHANCE: i32 = 2;

/// 受击时每点伤害扣掉的魔法盾时长（毫秒）：`(damage - armour) * 60`
/// （`HumanObject.cs:7165`、`:7275`）。
pub const MAGIC_SHIELD_MS_PER_DAMAGE: i64 = 60;

/// 本作用到的增益种类。
///
/// 前 9 个是技能增益（Crystal `AddBuff` 的调用点已列在各自注释里），
/// 后面的是药水增益（`PlayerObject.cs:5849-5892`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BuffKind {
    /// 战士护体（`Spell.ProtectionField`）：`HumanObject.cs:4955-4963`。
    ProtectionField,
    /// 魔法盾（`Spell.MagicShield`）：`HumanObject.cs:6196-6207`。
    MagicShield,
    /// 道士幽灵盾（`Spell.SoulShield`）：`Map.cs:1046-1097`。
    SoulShield,
    /// 道士神圣战甲术（`Spell.BlessedArmour`）：`Map.cs:1046-1097`。
    BlessedArmour,
    /// 道士气功波（`Spell.EnergyShield`）：`HumanObject.cs:4751-4783`。
    EnergyShield,
    /// 刺客疾风步（`Spell.Haste`）：`HumanObject.cs:6149-6153`。
    Haste,
    /// 刺客体轻如燕（`Spell.LightBody`）：`HumanObject.cs:6187-6191`。
    LightBody,
    /// 战士狂暴（`Spell.Fury`）：`HumanObject.cs:6160-6164`。
    Fury,
    /// 法师魔力增幅（`Spell.MagicBooster`）：`HumanObject.cs:6228-6240`。
    MagicBooster,
    /// 药水：物理攻击（`BuffType.Impact`，`PlayerObject.cs:5853-5854`）。
    Impact,
    /// 药水：魔法攻击（`BuffType.Magic`，`:5856-5857`）。
    Magic,
    /// 药水：道术攻击（`BuffType.Taoist`，`:5859-5860`）。
    Taoist,
    /// 药水：攻速（`BuffType.Storm`，`:5862-5863`）。
    Storm,
    /// 药水：生命上限（`BuffType.HealthAid`，`:5865-5866`）。
    HealthAid,
    /// 药水：魔法上限（`BuffType.ManaAid`，`:5868-5869`）。
    ManaAid,
    /// 药水：物理防御（`BuffType.Defence`，`:5871-5872`）。
    Defence,
    /// 药水：魔法防御（`BuffType.MagicDefence`，`:5874-5875`）。
    MagicDefence,
    /// 药水：负重（`BuffType.BagWeight`，`:5877-5878`）。
    BagWeight,
    /// 经验加成药水（`BuffType.Exp`，`:5881-5885`）。
    Exp,
    /// 爆率加成药水（`BuffType.Drop`，`:5887-5891`）。
    Drop,
}

impl BuffKind {
    /// 对应的 Crystal `BuffType`（`Shared/Enums.cs:1334-1399`）。
    pub const fn buff_type(self) -> BuffType {
        match self {
            Self::ProtectionField => BuffType::ProtectionField,
            Self::MagicShield => BuffType::MagicShield,
            Self::SoulShield => BuffType::SoulShield,
            Self::BlessedArmour => BuffType::BlessedArmour,
            Self::EnergyShield => BuffType::EnergyShield,
            Self::Haste => BuffType::Haste,
            Self::LightBody => BuffType::LightBody,
            Self::Fury => BuffType::Fury,
            Self::MagicBooster => BuffType::MagicBooster,
            Self::Impact => BuffType::Impact,
            Self::Magic => BuffType::Magic,
            Self::Taoist => BuffType::Taoist,
            Self::Storm => BuffType::Storm,
            Self::HealthAid => BuffType::HealthAid,
            Self::ManaAid => BuffType::ManaAid,
            Self::Defence => BuffType::Defence,
            Self::MagicDefence => BuffType::MagicDefence,
            Self::BagWeight => BuffType::BagWeight,
            Self::Exp => BuffType::Exp,
            Self::Drop => BuffType::Drop,
        }
    }

    /// 是否在客户端图标栏显示，并因此**广播**给周围玩家（`BuffInfo.Visible`，
    /// `Server/MirDatabase/BuffInfo.cs:12`；广播点在 `HumanObject.cs:7478-7481`）。
    ///
    /// 取值逐条对照 `BuffInfo.Load()`（`BuffInfo.cs:19-83`）：只有
    /// `Fury` / `EnergyShield` / `MagicBooster` 在这批里是 `Visible = true`。
    pub const fn visible(self) -> bool {
        matches!(self, Self::Fury | Self::EnergyShield | Self::MagicBooster)
    }

    /// Crystal `BuffStackType`（`Shared/Enums.cs:1411-1421`）在本作里的两种行为。
    ///
    /// * `ResetDuration`：重复施放只把时长重置（`MapObject.cs:591-595`）；
    /// * `StackDuration`：时长**累加**（`MapObject.cs:596-600`）。
    pub const fn stacks_duration(self) -> bool {
        matches!(
            self,
            Self::Exp
                | Self::Drop
                | Self::Impact
                | Self::Magic
                | Self::Taoist
                | Self::Storm
                | Self::HealthAid
                | Self::ManaAid
                | Self::Defence
                | Self::MagicDefence
                | Self::BagWeight
        )
    }
}

/// 一个生效中的增益（Crystal `Buff`，`Server/MirDatabase/BuffInfo.cs:90-133`）。
#[derive(Debug, Clone, PartialEq)]
pub struct ActiveBuff {
    /// 增益种类。
    pub kind: BuffKind,
    /// 到期时刻（毫秒，服务器单调时钟，绝对时刻）。
    ///
    /// Crystal 存的是**剩余时长**（`Buff.ExpireTime`，每 tick 递减一次），
    /// 本作存绝对时刻，见模块文档「到期语义」。
    pub expire_at: i64,
    /// 技能的附加参数（Crystal `Buff.Values`，`BuffInfo.cs:108`）。
    ///
    /// 本作的 9 个技能增益与药水增益都不需要参数，保留字段是为了与 Crystal 的
    /// `ClientBuff.Values` 对齐（`BuffInfo.cs:268`）。
    pub values: Vec<i32>,
    /// 属性加成（Crystal `Buff.Stats`；叠加方式见 `RefreshBuffs`，`HumanObject.cs:2332`）。
    pub stats: Stats,
    /// 施法者对象 ID（Crystal `Buff.Caster`；**不**上行，`BuffInfo.cs:255`）。
    pub caster_id: u32,
}

impl ActiveBuff {
    /// 新建一个增益。
    pub fn new(
        kind: BuffKind,
        expire_at: i64,
        stats: Stats,
        caster_id: u32,
        values: Vec<i32>,
    ) -> Self {
        Self {
            kind,
            expire_at,
            values,
            stats,
            caster_id,
        }
    }

    /// 组装上行的 `ClientBuff`（Crystal `Buff.ToClientBuff`，`BuffInfo.cs:256-270`）。
    ///
    /// 三个容易看错的点：
    /// * `ObjectID` 是**被增益的对象**（`Buff.ObjectID = ObjectID`，`MapObject.cs:577`），
    ///   不是施法者；
    /// * `ExpireTime` 上行的是**剩余毫秒数**（客户端收到后自己 `+= CMain.Time`，
    ///   `Client/MirScenes/GameScene.cs:5181`），不是 Unix 时间戳；
    /// * `Values` 为空时下发空数组（Crystal 里 `Values` 可能为 `null`，`Save` 会抛异常，
    ///   这里统一写 0 项）。
    fn to_client_buff(&self, object_id: u32, now: i64) -> ClientBuff {
        ClientBuff {
            buff_type: self.kind.buff_type(),
            visible: self.kind.visible(),
            object_id,
            expire_time: (self.expire_at - now).max(0),
            infinite: false,
            paused: false,
            stats: self.stats.clone(),
            values: self.values.clone(),
        }
    }
}

/// 一条「等待生效」的增益（Crystal `DelayedAction(DelayedType.Magic, …, stats)` 的数据部分）。
///
/// 增益的数值在**施法瞬间**就抽好（`HumanObject.cs:3591` 的 `magic.GetPower(...)`），
/// 只是写入 buff 的时刻被推迟了 500ms 或「距离 × 50 + 500」ms，所以这里带上已算好的属性表。
#[derive(Debug, Clone, PartialEq)]
pub struct PendingBuff {
    /// 增益种类。
    pub kind: BuffKind,
    /// 持续时长（毫秒）。
    pub duration_ms: i64,
    /// 已算好的属性加成。
    pub stats: Stats,
}

impl PendingBuff {
    /// 构造。
    pub fn new(kind: BuffKind, duration_ms: i64, stats: Stats) -> Self {
        Self {
            kind,
            duration_ms,
            stats,
        }
    }
}

impl World {
    /// 某个玩家当前的增益列表（没有该玩家时返回空切片）。
    #[must_use]
    pub fn buffs(&self, object_id: u32) -> &[ActiveBuff] {
        self.players
            .get(&object_id)
            .map_or(&[], |player| player.buffs.as_slice())
    }

    /// 玩家身上是否有这种增益（Crystal `MapObject.HasBuff(BuffType)`，`MapObject.cs:695-716`）。
    #[must_use]
    pub fn has_buff(&self, object_id: u32, kind: BuffKind) -> bool {
        self.buffs(object_id).iter().any(|buff| buff.kind == kind)
    }

    /// 某个玩家的增益里是否带这个属性（只读，给伤害结算用）。
    #[must_use]
    pub fn buff_stat(&self, object_id: u32, stat: Stat) -> i32 {
        self.buffs(object_id)
            .iter()
            .map(|buff| buff.stats.get(stat))
            .sum()
    }

    /// 登记一个增益（Crystal `HumanObject.AddBuff`，`HumanObject.cs:7460-7489`）。
    ///
    /// 流程与 Crystal 一一对应：
    ///
    /// ```csharp
    /// Buff b = base.AddBuff(type, owner, duration, stats, ...);      // 叠加规则 MapObject.cs:570-670
    /// switch (b.Type) { case MagicShield: CurrentMap.Broadcast(S.ObjectEffect{MagicShieldUp}); }
    /// var packet = new S.AddBuff { Buff = b.ToClientBuff() };
    /// Enqueue(packet);                                                // 本人一定收到
    /// if (b.Info.Visible) Broadcast(packet);                          // 可见的才广播
    /// if (refreshStats) RefreshStats();                               // 重新叠属性
    /// ```
    ///
    /// 返回 `false` 表示对象不在世界里（什么都没发生）。
    pub fn add_buff(
        &mut self,
        object_id: u32,
        kind: BuffKind,
        duration_ms: i64,
        stats: Stats,
        caster_id: u32,
    ) -> bool {
        let now = self.now();
        let Some((map, position)) = self
            .players
            .get(&object_id)
            .map(|player| (player.map.clone(), player.position))
        else {
            return false;
        };
        let expire_at = now.saturating_add(duration_ms.max(0));
        let existing = self
            .players
            .get_mut(&object_id)
            .and_then(|player| player.buffs.iter_mut().find(|buff| buff.kind == kind));
        if let Some(buff) = existing {
            if kind.stacks_duration() {
                // `BuffStackType.StackDuration`：`buff.ExpireTime += duration;`（`MapObject.cs:598`）。
                buff.expire_at = buff.expire_at.saturating_add(duration_ms.max(0));
            } else {
                // `BuffStackType.ResetDuration`：`buff.ExpireTime = duration;`（`MapObject.cs:593`）——
                // **不动属性**，所以重复施放沿用第一次算出来的加成值（Crystal 原样行为）。
                buff.expire_at = expire_at;
            }
            buff.caster_id = caster_id;
        } else {
            let buff = ActiveBuff::new(kind, expire_at, stats, caster_id, Vec::new());
            let Some(player) = self.players.get_mut(&object_id) else {
                return false;
            };
            player.buffs.push(buff);
        }

        // `case BuffType.MagicShield: CurrentMap.Broadcast(S.ObjectEffect{MagicShieldUp});`
        // （`HumanObject.cs:7466-7468`）——`Map.Broadcast` 连自己一起发（`Map.cs:2437-2448`）。
        if kind == BuffKind::MagicShield {
            self.broadcast_effect(&map, position, object_id, SpellEffect::MagicShieldUp);
        }

        // `Enqueue(S.AddBuff)` + 可见时 `Broadcast`（`HumanObject.cs:7474-7481`）。
        if let Some(buff) = self
            .players
            .get(&object_id)
            .and_then(|player| player.buffs.iter().find(|buff| buff.kind == kind))
        {
            let packet = ServerPacket::AddBuff(AddBuff {
                buff: buff.to_client_buff(object_id, now),
            })
            .encode_frame();
            self.send_to_player(object_id, &packet);
            if kind.visible() {
                self.broadcast(&map, position, Some(object_id), &packet, object_id);
            }
        }

        // `if (refreshStats) RefreshStats();`（`HumanObject.cs:7483-7486`）。
        self.refresh_stats(object_id);
        true
    }

    /// 移除一个增益（Crystal `MapObject.RemoveBuff` + `ProcessBuffs` 的移除分支）。
    ///
    /// Crystal 的 `RemoveBuff` 只打 `FlagForRemoval`，真正的移除与 `S.RemoveBuff` 发生在
    /// 下一次 `ProcessBuffs`（最多 1 秒后，`HumanObject.cs:402-453`）；本作立即移除并发包。
    ///
    /// 返回 `false` 表示身上没有这个增益。
    pub fn remove_buff(&mut self, object_id: u32, kind: BuffKind) -> bool {
        let Some(player) = self.players.get_mut(&object_id) else {
            return false;
        };
        let Some(index) = player.buffs.iter().position(|buff| buff.kind == kind) else {
            return false;
        };
        player.buffs.remove(index);
        let map = player.map.clone();
        let position = player.position;

        // `Enqueue(new S.RemoveBuff { Type, ObjectID })`；可见的再 `Broadcast`（`:453-458`）。
        let packet = ServerPacket::RemoveBuff(RemoveBuff {
            type_: kind.buff_type(),
            object_id,
        })
        .encode_frame();
        self.send_to_player(object_id, &packet);
        if kind.visible() {
            self.broadcast(&map, position, Some(object_id), &packet, object_id);
        }
        // `case BuffType.MagicShield: … MagicShieldDown`（`HumanObject.cs:485-487`）。
        if kind == BuffKind::MagicShield {
            self.broadcast_effect(&map, position, object_id, SpellEffect::MagicShieldDown);
        }
        self.refresh_stats(object_id);
        true
    }

    /// 受击扣减增益时长（Crystal `HumanObject.cs:7163-7167`、`:7273-7277`）。
    ///
    /// ```csharp
    /// if (HasBuff(BuffType.MagicShield, out Buff magicShield))
    /// {
    ///     var duration = (int)Math.Min(int.MaxValue, magicShield.ExpireTime - ((damage - armour) * 60));
    ///     AddBuff(BuffType.MagicShield, this, duration, null);      // ResetDuration → 换成新时长
    /// }
    /// ```
    ///
    /// 也就是**每点实际伤害扣 60ms**；`AddBuff` 还会再给本人补发一条 `S.AddBuff`
    /// （不可见 buff 不广播）。本作照抄这两点。
    pub fn shave_buff(&mut self, object_id: u32, kind: BuffKind, amount_ms: i64) -> bool {
        if amount_ms <= 0 {
            return false;
        }
        let now = self.now();
        let Some(buff) = self
            .players
            .get_mut(&object_id)
            .and_then(|player| player.buffs.iter_mut().find(|buff| buff.kind == kind))
        else {
            return false;
        };
        buff.expire_at = buff.expire_at.saturating_sub(amount_ms);
        let packet = ServerPacket::AddBuff(AddBuff {
            buff: buff.to_client_buff(object_id, now),
        })
        .encode_frame();
        // 不可见的 buff 只发给本人（`HumanObject.cs:7474-7481`）。
        self.send_to_player(object_id, &packet);
        true
    }

    /// 每秒一次的到期扫描（Crystal `HumanObject.ProcessBuffs`，`HumanObject.cs:397-494`）。
    ///
    /// Crystal 用 `Buff.NextTime = Envir.Time + 1000` 把扫描节流到 1 秒一次，
    /// 本作每 tick 都精确判定（见模块文档）。
    pub(crate) fn tick_buffs(&mut self, now: i64) {
        let mut ids: Vec<u32> = self.players.keys().copied().collect();
        ids.sort_unstable();
        for object_id in ids {
            let expired: Vec<BuffKind> = self
                .players
                .get(&object_id)
                .map(|player| {
                    player
                        .buffs
                        .iter()
                        .filter(|buff| now >= buff.expire_at)
                        .map(|buff| buff.kind)
                        .collect()
                })
                .unwrap_or_default();
            for kind in expired {
                self.remove_buff(object_id, kind);
            }
        }
    }

    /// 给玩家套上一组「待生效」的增益（延迟效果队列里的那一条）。
    pub(crate) fn apply_pending_buff(
        &mut self,
        target_id: u32,
        caster_id: u32,
        buff: &PendingBuff,
    ) -> bool {
        // `if (HasBuff(BuffType.MagicShield, out _)) return;`（`HumanObject.cs:6200`）：
        // 魔法盾还在时重复施放不会刷新。
        if buff.kind == BuffKind::MagicShield && self.has_buff(target_id, buff.kind) {
            return false;
        }
        self.add_buff(
            target_id,
            buff.kind,
            buff.duration_ms,
            buff.stats.clone(),
            caster_id,
        )
    }

    /// 广播一条 `S.ObjectEffect`（Crystal `Map.Broadcast(packet, location)`，`Map.cs:2437-2448`）。
    ///
    /// 与 [`World::broadcast`] 的区别：Crystal 这个重载**连自己一起发**（按坐标筛玩家，不排除本人），
    /// 所以这里先给自己发一份，再发给同图视野内的其它玩家。
    fn broadcast_effect(
        &mut self,
        map: &str,
        position: Point,
        object_id: u32,
        effect: SpellEffect,
    ) {
        let packet = ServerPacket::ObjectEffect(ObjectEffect {
            object_id,
            effect,
            ..ObjectEffect::default()
        })
        .encode_frame();
        self.send_to_player(object_id, &packet);
        self.broadcast(map, position, Some(object_id), &packet, 0);
    }

    /// 把 `S.ObjectHealth` 同步给本人与同图视野内的玩家。
    ///
    /// Crystal 的 `BroadcastHealthChange` 对**玩家**只发给队友（`MapObject.cs:856-871`），
    /// 本作没有队伍系统，于是改为「本人 + 视野内所有人」：客户端用它更新对象血条，
    /// 这样增益到期导致上限变化时血条不会停在旧值（**本作扩展**，见 `docs/world/buffs.md`）。
    pub(crate) fn broadcast_health_change(&mut self, object_id: u32) {
        let Some(player) = self.players.get(&object_id) else {
            return;
        };
        let percent = percent_health(player.hp, player.stats.hp);
        let map = player.map.clone();
        let position = player.position;
        let packet = ServerPacket::ObjectHealth(mir_protocol::packets::server::ObjectHealth {
            object_id,
            percent,
            // Crystal `Math.Max(5, (RevTime - Envir.Time) / 1000)`（`MapObject.cs:845`）。
            expire: OBJECT_HEALTH_EXPIRE_SECONDS,
        })
        .encode_frame();
        self.send_to_player(object_id, &packet);
        self.broadcast(&map, position, Some(object_id), &packet, 0);
    }
}

/// 血量百分比：Crystal `MapObject.PercentHealth`（`Server/MirObjects/MapObject.cs:812-816`）。
fn percent_health(hp: i32, max_hp: i32) -> u8 {
    if max_hp <= 0 {
        return 0;
    }
    let percent = i64::from(hp) * 100 / i64::from(max_hp);
    u8::try_from(percent.clamp(0, 100)).unwrap_or(0)
}

/// 药水增益的「每分钟」换算：`time * Settings.Minute`（`PlayerObject.cs:5854` 等）。
///
/// `time` 是 `item.Info.Durability`（分钟）。
pub fn minutes_to_ms(minutes: u16) -> i64 {
    i64::from(minutes) * MINUTE_MS
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn buff_kinds_map_to_crystal_buff_types() {
        assert_eq!(BuffKind::MagicShield.buff_type(), BuffType::MagicShield);
        assert_eq!(BuffKind::Haste.buff_type(), BuffType::Haste);
        assert_eq!(BuffKind::Exp.buff_type(), BuffType::Exp);
        assert_eq!(BuffKind::Drop.buff_type(), BuffType::Drop);
        // 可见性逐条对照 `BuffInfo.Load()`：这批里只有三个是 Visible = true。
        assert!(BuffKind::Fury.visible());
        assert!(BuffKind::EnergyShield.visible());
        assert!(BuffKind::MagicBooster.visible());
        assert!(!BuffKind::MagicShield.visible());
        assert!(!BuffKind::ProtectionField.visible());
        assert!(!BuffKind::SoulShield.visible());
    }

    #[test]
    fn stack_rules_split_skills_from_potions() {
        // 技能增益是 `ResetDuration`，药水增益是 `StackDuration`（`BuffInfo.cs:19-83`）。
        assert!(!BuffKind::ProtectionField.stacks_duration());
        assert!(!BuffKind::MagicShield.stacks_duration());
        assert!(BuffKind::Exp.stacks_duration());
        assert!(BuffKind::HealthAid.stacks_duration());
    }

    #[test]
    fn percent_health_clamps_to_byte_range() {
        assert_eq!(percent_health(50, 100), 50);
        assert_eq!(percent_health(0, 100), 0);
        assert_eq!(percent_health(200, 100), 100);
        assert_eq!(percent_health(10, 0), 0);
    }

    #[test]
    fn minutes_to_ms_matches_setting() {
        // `Settings.Minute = 60 * Second = 60000`（`Server/Settings.cs:16`）。
        assert_eq!(minutes_to_ms(1), 60_000);
        assert_eq!(minutes_to_ms(3), 180_000);
    }
}
