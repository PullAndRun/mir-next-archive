//! `[Flags]` 枚举的位掩码保真测试。
//!
//! Crystal 里 `RequiredClass`、`GuildRankOptions`、`BindMode`、`SpecialItemMode`、
//! `PoisonType`、`LevelEffects` 都标了 `[Flags]`，线上值是**位并集**：
//! 公会会长是 `(GuildRankOptions)255`，任务可以要求 `Warrior|Wizard|Taoist = 7`。
//! 如果把它们当成普通 Rust 枚举（`from_value(255)` → `None` → 默认值），
//! 解码后再编码就会把 255 写成 1，客户端表现就是「权限全丢」。这个文件专门守住这条。

use mir_core::enums::{
    BindMode, GuildRankOptions, LevelEffects, PoisonType, RequiredClass, RequiredGender,
    SpecialItemMode,
};
use mir_protocol::ServerPacket;
use mir_protocol::packets::server::GuildStatus;

#[test]
fn required_class_keeps_bit_combinations() {
    // Warrior = 1, Wizard = 2, Taoist = 4 → 三者可接 = 7。
    let combo = RequiredClass::Warrior | RequiredClass::Wizard | RequiredClass::Taoist;
    assert_eq!(combo.value(), 7);
    assert!(combo.contains(RequiredClass::Wizard));
    assert!(!combo.contains(RequiredClass::Assassin));

    // 任何位组合都能原样往返（C# 侧就是这么用的）。
    let decoded = RequiredClass::from_value(7).expect("位掩码任何取值都合法");
    assert_eq!(decoded, combo);
    assert_eq!(
        RequiredClass::from_value(255)
            .expect("未知位也不丢")
            .value(),
        255
    );
}

#[test]
fn guild_rank_options_leader_is_255() {
    // Crystal `Server/MirDatabase/GuildInfo.cs:54`：会长是 `(GuildRankOptions)255`。
    let leader = GuildRankOptions::from_value(255).expect("位掩码");
    assert_eq!(leader.value(), 255);
    for option in GuildRankOptions::ALL {
        assert!(leader.contains(*option), "会长应拥有全部权限：{option:?}");
    }
    // 普通职位按位或出来（`GuildObject.cs:370` 的 `Options |= 1 << Option`）。
    let officer = GuildRankOptions::CanRecruit | GuildRankOptions::CanKick;
    assert_eq!(
        officer.value(),
        GuildRankOptions::CanRecruit.value() | GuildRankOptions::CanKick.value()
    );
    assert!(officer.contains(GuildRankOptions::CanRecruit));
    assert!(!officer.contains(GuildRankOptions::CanChangeRank));
}

#[test]
fn other_flag_enums_keep_unknown_bits() {
    assert_eq!(BindMode::from_value(-1).expect("位掩码").value(), -1);
    assert_eq!(
        (SpecialItemMode::Paralize | SpecialItemMode::Teleport).value(),
        SpecialItemMode::Paralize.value() | SpecialItemMode::Teleport.value()
    );
    assert_eq!(
        PoisonType::from_value(0xFFFF).expect("位掩码").value(),
        0xFFFF
    );
    assert_eq!(
        (LevelEffects::Mist | LevelEffects::RedDragon).value(),
        LevelEffects::Mist.value() | LevelEffects::RedDragon.value()
    );
    assert_eq!(RequiredGender::from_value(3).expect("位掩码").value(), 3);
}

#[test]
fn guild_status_packet_preserves_leader_permissions() {
    let packet = GuildStatus {
        guild_name: "测试行会".to_owned(),
        guild_rank_name: "会长".to_owned(),
        level: 5,
        member_count: 1,
        max_members: 50,
        my_options: GuildRankOptions::from_value(255).expect("位掩码"),
        my_rank_id: 0,
        ..GuildStatus::default()
    };

    let frame = ServerPacket::GuildStatus(packet.clone()).encode_frame();
    let decoded = ServerPacket::decode(GuildStatus::INDEX, &frame[4..]).expect("解码 GuildStatus");
    let ServerPacket::GuildStatus(decoded) = decoded else {
        panic!("包编号不对");
    };
    assert_eq!(
        decoded.my_options.value(),
        255,
        "会长权限 255 必须原样往返，不能被折成默认值"
    );
    assert_eq!(decoded, packet);
}
