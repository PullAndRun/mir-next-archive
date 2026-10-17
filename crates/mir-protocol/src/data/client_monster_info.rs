//! 客户端怪物资料 `ClientMonsterInfo` 的线格式。
//!
//! 对照 Crystal `Shared/Data/MonsterData.cs:3`（读入 ctor `:23`，`Save` `:47`）手写。
//! 该结构由 `ServerPackets.NewMonsterInfo`（`Shared/ServerPackets.cs:6789-6809`）
//! 整块推送，没有版本参数。
//!
//! 线格式（一行一字段，括号内是宽度）：
//!
//! ```text
//! i32    Index
//! string Name                 <- 7 位长度前缀 + UTF-8
//! string GameName             <- 7 位长度前缀 + UTF-8
//! u16    Image                Monster 枚举（ushort）
//! u8     AI
//! u8     Effect
//! u16    Level
//! u8     ViewRange
//! u8     CoolEye
//! u8     Light
//! u16    AttackSpeed
//! u16    MoveSpeed
//! u32    Experience
//! bool   CanPush
//! bool   CanTame
//! bool   AutoRev
//! bool   Undead
//! bool   CanRecall
//! Stats  Stats                <- i32 个数 + (u8 stat, i32 value)*
//! ```
//!
//! **注意布尔顺序**：C# 的字段声明是 `bool CanTame, CanPush, AutoRev, Undead, CanRecall;`
//!（`:14`），但 ctor 与 `Save` 都是先 `CanPush` 再 `CanTame`（`:38-39`、`:62-63`）。
//! 线格式以 `Save` 为准，因此本结构体的字段顺序也是 `can_push` 在前。

use mir_core::enums::Monster;

use crate::data::Stats;
use crate::{PacketReader, PacketWriter, ProtocolError};

/// Crystal `ClientMonsterInfo`（`Shared/Data/MonsterData.cs:3`；ctor `:23`，`Save` `:47`）。
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ClientMonsterInfo {
    /// 怪物模板编号。
    pub index: i32,
    /// 显示名。
    pub name: String,
    /// 游戏内名字（带称号等）。
    pub game_name: String,
    /// 外观编号（`ushort` 的 `Monster` 枚举）。
    pub image: Monster,
    /// AI 编号。
    pub ai: u8,
    /// 特效编号。
    pub effect: u8,
    /// 等级。
    pub level: u16,
    /// 视野范围。
    pub view_range: u8,
    /// 反隐等级。
    pub cool_eye: u8,
    /// 光照范围。
    pub light: u8,
    /// 攻击速度。
    pub attack_speed: u16,
    /// 移动速度。
    pub move_speed: u16,
    /// 击杀经验。
    pub experience: u32,
    /// 是否可被推动。
    pub can_push: bool,
    /// 是否可被驯服。
    pub can_tame: bool,
    /// 是否自动复活。
    pub auto_rev: bool,
    /// 是否为亡灵。
    pub undead: bool,
    /// 是否可被召唤。
    pub can_recall: bool,
    /// 用于详细提示框的属性表。
    pub stats: Stats,
}

impl ClientMonsterInfo {
    /// 解码，见 `Shared/Data/MonsterData.cs:23`。
    pub fn decode(r: &mut PacketReader<'_>) -> Result<Self, ProtocolError> {
        let index = r.read_i32()?;
        let name = r.read_string()?;
        let game_name = r.read_string()?;
        let image = Monster::from_value(r.read_u16()?).unwrap_or_default();
        let ai = r.read_u8()?;
        let effect = r.read_u8()?;
        let level = r.read_u16()?;
        let view_range = r.read_u8()?;
        let cool_eye = r.read_u8()?;
        let light = r.read_u8()?;
        let attack_speed = r.read_u16()?;
        let move_speed = r.read_u16()?;
        let experience = r.read_u32()?;
        // ctor 顺序：CanPush -> CanTame -> AutoRev -> Undead -> CanRecall（`:38-42`）。
        let can_push = r.read_bool()?;
        let can_tame = r.read_bool()?;
        let auto_rev = r.read_bool()?;
        let undead = r.read_bool()?;
        let can_recall = r.read_bool()?;
        let stats = Stats::decode(r)?;
        Ok(Self {
            index,
            name,
            game_name,
            image,
            ai,
            effect,
            level,
            view_range,
            cool_eye,
            light,
            attack_speed,
            move_speed,
            experience,
            can_push,
            can_tame,
            auto_rev,
            undead,
            can_recall,
            stats,
        })
    }

    /// 编码，见 `Shared/Data/MonsterData.cs:47`。
    pub fn encode(&self, w: &mut PacketWriter) {
        w.write_i32(self.index);
        w.write_string(&self.name);
        w.write_string(&self.game_name);
        w.write_u16(self.image.value());
        w.write_u8(self.ai);
        w.write_u8(self.effect);
        w.write_u16(self.level);
        w.write_u8(self.view_range);
        w.write_u8(self.cool_eye);
        w.write_u8(self.light);
        w.write_u16(self.attack_speed);
        w.write_u16(self.move_speed);
        w.write_u32(self.experience);
        // `Save` 顺序同样是 CanPush -> CanTame（`:62-63`），与字段声明顺序相反。
        w.write_bool(self.can_push);
        w.write_bool(self.can_tame);
        w.write_bool(self.auto_rev);
        w.write_bool(self.undead);
        w.write_bool(self.can_recall);
        self.stats.encode(w);
    }
}

#[cfg(test)]
mod tests {
    use mir_core::enums::Stat;

    use super::*;

    fn sample() -> ClientMonsterInfo {
        ClientMonsterInfo {
            index: 51,
            name: "半兽人".to_owned(),
            game_name: "半兽人战士".to_owned(),
            image: Monster::Oma,
            ai: 3,
            effect: 2,
            level: 21,
            view_range: 7,
            cool_eye: 3,
            light: 4,
            attack_speed: 1500,
            move_speed: 800,
            experience: 350,
            can_push: true,
            can_tame: false,
            auto_rev: true,
            undead: false,
            can_recall: true,
            stats: [(Stat::MaxDC, 12), (Stat::HP, 180), (Stat::Agility, -1)]
                .into_iter()
                .collect(),
        }
    }

    #[test]
    fn roundtrip() {
        let value = sample();
        let mut writer = PacketWriter::new();
        value.encode(&mut writer);
        let bytes = writer.into_inner();

        let mut reader = PacketReader::new(&bytes);
        let decoded = ClientMonsterInfo::decode(&mut reader).expect("解码应当成功");
        assert_eq!(decoded, value);
        assert!(
            reader.is_empty(),
            "应正好读完负载，剩余 {} 字节",
            reader.remaining()
        );
    }

    #[test]
    fn bool_order_is_push_before_tame() {
        // `MonsterData.cs:62-63` 先写 CanPush 再写 CanTame（字段声明顺序正好相反）。
        // 前 4+1+1（Index + 两个空串长度前缀）= 6，随后：
        //   u16 Image(2) + u8 AI/Effect(2) + u16 Level(2) + u8 ViewRange/CoolEye/Light(3)
        //   + u16 AttackSpeed/MoveSpeed(4) + u32 Experience(4) = 17
        // 因此布尔区从 6 + 17 = 23 开始。
        let value = ClientMonsterInfo {
            can_push: true,
            can_tame: false,
            auto_rev: false,
            undead: true,
            can_recall: false,
            ..ClientMonsterInfo::default()
        };

        let mut writer = PacketWriter::new();
        value.encode(&mut writer);
        let bytes = writer.into_inner();

        assert_eq!(&bytes[23..28], &[0x01, 0x00, 0x00, 0x01, 0x00]);
        // 空属性表：i32 个数 0。
        assert_eq!(&bytes[28..32], &[0x00, 0x00, 0x00, 0x00]);
        assert_eq!(bytes.len(), 32);

        let mut reader = PacketReader::new(&bytes);
        assert_eq!(
            ClientMonsterInfo::decode(&mut reader).expect("解码应当成功"),
            value
        );
    }

    #[test]
    fn truncated_payload_errors() {
        let mut writer = PacketWriter::new();
        sample().encode(&mut writer);
        let bytes = writer.into_inner();
        let mut reader = PacketReader::new(&bytes[..bytes.len() - 1]);
        assert!(ClientMonsterInfo::decode(&mut reader).is_err());
    }
}
