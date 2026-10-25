//! R52-B4：R51 终审 v4 的定向补证（默认资料 / 真实冻结 / `Freezing` 全来源 / `StormEscape`）。
//!
//! 本文件只补证据，不改生产语义；断言全部对照 Crystal（参考根目录 `github\Crystal`）：
//!
//! | 证据项 | Crystal |
//! |---|---|
//! | 默认七技能记录来自实际默认 `GameData`（`game-assets/data/game.mnd`，非手造） | `Server/MirDatabase/MagicInfo.cs:22-65` |
//! | 耗蓝 `BaseCost + LevelCost * Level` | `HumanObject.cs:3375-3398` |
//! | 冷却 `DelayBase - Level * DelayReduction`，`now < CastTime + Delay` 拒绝 | `MagicInfo.cs:219-222`、`HumanObject.cs:3436-3440` |
//! | 冰咆哮对怪：Slow/Frozen 两条真实毒 + 真实伤害 | `HumanObject.cs:5951-5985`、`SpellObject.cs` |
//! | 玩家 Frozen 后 `Attack` 直接返回（不消耗动作） | `HumanObject.cs:1680-1690`、`:1730-1745` |
//! | Freezing 聚合：模板 `Stats`(:1858) + `AddedStats`(:1859) + 生效增益(:2332)，坏装备跳过(:1836) | `HumanObject.cs:1830-1860`、`:2332` |
//! | 冻结跳数 `5 + Random.Next(Freezing)`；Freezing = 0 不抽 RNG | `HumanObject.cs:5974` |
//! | 狂风：`Random.Next(4) > Level` 失败，成功才传送 | `HumanObject.cs:3470-3500` |
//! | 狂风五位置：本人 `ObjectEffect` → `AddBuff(TemporalFlux)`；观察者 `Out` → `In` → `ObjectEffect` | `HumanObject.cs:6080-6092` |
//!
//! 所有用例确定性：固定 RNG 种子、`with_data` 里先 `advance_clock(0)` 冻结时钟。

mod common;

use std::collections::{BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use mir_core::Point;
use mir_core::enums::{
    AttackMode, BuffType, MirClass, MirDirection, MirGender, PoisonType, Spell, SpellEffect, Stat,
};
use mir_protocol::ServerPacket;
use mir_protocol::data::{Stats, UserItem};
use mir_server::MonsterKind;
use mir_server::combat::{self, CombatStats};
use mir_server::gamedata::GameData;
use mir_server::magic::{CastReport, MagicError, PoisonState};
use mir_server::world::{CombatError, Player, World};
use mir_server::{BuffKind, UserMagic};
use tokio::sync::mpsc;

const MAP: &str = "0";

/// 默认七技能（R51 范围；`docs/process/round51-libu-docs.md:32` 起）。
const SEVEN_SPELLS: [Spell; 7] = [
    Spell::Blink,
    Spell::ShoulderDash,
    Spell::StormEscape,
    Spell::FrostCrunch,
    Spell::ThunderStorm,
    Spell::PoisonCloud,
    Spell::TrapHexagon,
];

/// 默认资料里七技能的关键字段（独立硬编码期望，不读 `MagicInfo` 的字段来自证）。
struct ExpectedMagic {
    spell: Spell,
    name: &'static str,
    spell_value: u8,
    base_cost: u8,
    level_cost: u8,
    icon: u8,
    levels: [u8; 3],
    needs: [u16; 3],
    delay_base: u32,
    delay_reduction: u32,
    range: u8,
    power_base: u16,
    power_bonus: u16,
    m_power_base: u16,
    m_power_bonus: u16,
}

const EXPECTED_MAGICS: [ExpectedMagic; 7] = [
    ExpectedMagic {
        spell: Spell::Blink,
        name: "Blink",
        spell_value: 151,
        base_cost: 10,
        level_cost: 3,
        icon: 20,
        levels: [19, 22, 25],
        needs: [350, 1_000, 2_000],
        delay_base: 1_800,
        delay_reduction: 0,
        range: 9,
        power_base: 0,
        power_bonus: 0,
        m_power_base: 0,
        m_power_bonus: 0,
    },
    ExpectedMagic {
        spell: Spell::ShoulderDash,
        name: "野蛮冲撞",
        spell_value: 5,
        base_cost: 4,
        level_cost: 4,
        icon: 26,
        levels: [30, 32, 34],
        needs: [300, 400, 600],
        delay_base: 2_500,
        delay_reduction: 0,
        range: 9,
        power_base: 0,
        power_bonus: 0,
        m_power_base: 4,
        m_power_bonus: 0,
    },
    ExpectedMagic {
        spell: Spell::StormEscape,
        name: "StormEscape",
        spell_value: 55,
        base_cost: 65,
        level_cost: 8,
        icon: 23,
        levels: [60, 61, 62],
        needs: [2_200, 3_300, 4_400],
        delay_base: 1_800,
        delay_reduction: 0,
        range: 9,
        power_base: 4,
        power_bonus: 4,
        m_power_base: 12,
        m_power_bonus: 0,
    },
    ExpectedMagic {
        spell: Spell::FrostCrunch,
        name: "寒冰掌",
        spell_value: 41,
        base_cost: 15,
        level_cost: 3,
        icon: 38,
        levels: [28, 30, 33],
        needs: [300, 500, 800],
        delay_base: 1_500,
        delay_reduction: 0,
        range: 9,
        power_base: 12,
        power_bonus: 0,
        m_power_base: 12,
        m_power_bonus: 0,
    },
    ExpectedMagic {
        spell: Spell::ThunderStorm,
        name: "地狱雷光",
        spell_value: 42,
        base_cost: 29,
        level_cost: 9,
        icon: 23,
        levels: [30, 32, 34],
        needs: [400, 800, 1_200],
        delay_base: 1_500,
        delay_reduction: 0,
        range: 9,
        power_base: 10,
        power_bonus: 20,
        m_power_base: 10,
        m_power_bonus: 20,
    },
    ExpectedMagic {
        spell: Spell::PoisonCloud,
        name: "毒云",
        spell_value: 83,
        base_cost: 30,
        level_cost: 5,
        icon: 54,
        levels: [43, 45, 48],
        needs: [400, 800, 1_200],
        delay_base: 16_000,
        delay_reduction: 2_000,
        range: 9,
        power_base: 3,
        power_bonus: 0,
        m_power_base: 12,
        m_power_bonus: 0,
    },
    ExpectedMagic {
        spell: Spell::TrapHexagon,
        name: "困魔咒",
        spell_value: 73,
        base_cost: 7,
        level_cost: 3,
        icon: 15,
        levels: [28, 30, 32],
        needs: [250, 500, 1_000],
        delay_base: 1_500,
        delay_reduction: 0,
        range: 9,
        power_base: 0,
        power_bonus: 0,
        m_power_base: 0,
        m_power_bonus: 0,
    },
];

// ---------------------------------------------------------------------------
// 默认数据：实际默认加载链（`game-assets/data/game.mnd`）
// ---------------------------------------------------------------------------

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join("..")
}

fn default_mnd_path() -> PathBuf {
    repo_root().join("game-assets/data/game.mnd")
}

/// 实际默认 `GameData`：`server.rs:590-600` 的 `<asset_root>/data/game.mnd` 分支。
///
/// B4 契约：缺文件是 BLOCK，不允许 skip。
fn default_game_data() -> Arc<GameData> {
    let path = default_mnd_path();
    assert!(
        path.is_file(),
        "默认数据 game.mnd 缺失：{}（B4 不允许 skip）",
        path.display()
    );
    Arc::new(GameData::load_mnd(&path).expect("默认 game.mnd 应能加载"))
}

/// 手写 SHA-256（`mir-server` 没有 `sha2` 依赖，写范围也不允许改 Cargo.toml）。
///
/// 用来对 `game-assets/data/game.mnd` 的实际字节复算来源哈希，与
/// `game-assets/data/manifest.json` 的 `sha256` 对照。
#[allow(
    clippy::format_collect,
    clippy::many_single_char_names,
    clippy::unreadable_literal
)]
fn sha256_hex(data: &[u8]) -> String {
    const K: [u32; 64] = [
        0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4,
        0xab1c5ed5, 0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe,
        0x9bdc06a7, 0xc19bf174, 0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f,
        0x4a7484aa, 0x5cb0a9dc, 0x76f988da, 0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7,
        0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967, 0x27b70a85, 0x2e1b2138, 0x4d2c6dfc,
        0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85, 0xa2bfe8a1, 0xa81a664b,
        0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070, 0x19a4c116,
        0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
        0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7,
        0xc67178f2,
    ];
    let mut h: [u32; 8] = [
        0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab,
        0x5be0cd19,
    ];
    let bit_len = u64::try_from(data.len()).expect("文件长度应能放进 u64") * 8;
    let mut message = data.to_vec();
    message.push(0x80);
    while message.len() % 64 != 56 {
        message.push(0);
    }
    message.extend_from_slice(&bit_len.to_be_bytes());

    for chunk in message.chunks(64) {
        let mut w = [0_u32; 64];
        for (i, word) in w.iter_mut().take(16).enumerate() {
            let start = i * 4;
            *word = u32::from_be_bytes([
                chunk[start],
                chunk[start + 1],
                chunk[start + 2],
                chunk[start + 3],
            ]);
        }
        for i in 16..64 {
            let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
            let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
            w[i] = w[i - 16]
                .wrapping_add(s0)
                .wrapping_add(w[i - 7])
                .wrapping_add(s1);
        }
        let [mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut hh] = h;
        for i in 0..64 {
            let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let ch = (e & f) ^ ((!e) & g);
            let temp1 = hh
                .wrapping_add(s1)
                .wrapping_add(ch)
                .wrapping_add(K[i])
                .wrapping_add(w[i]);
            let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let maj = (a & b) ^ (a & c) ^ (b & c);
            let temp2 = s0.wrapping_add(maj);
            hh = g;
            g = f;
            f = e;
            e = d.wrapping_add(temp1);
            d = c;
            c = b;
            b = a;
            a = temp1.wrapping_add(temp2);
        }
        for (slot, value) in h.iter_mut().zip([a, b, c, d, e, f, g, hh]) {
            *slot = slot.wrapping_add(value);
        }
    }
    h.iter().map(|word| format!("{word:08x}")).collect()
}

// ---------------------------------------------------------------------------
// 测试脚手架（真实默认数据 + 真实 World 入口）
// ---------------------------------------------------------------------------

struct Harness {
    world: World,
    receivers: HashMap<u32, mpsc::UnboundedReceiver<Vec<u8>>>,
}

impl Harness {
    fn with_data(seed: u64, data: Arc<GameData>) -> Self {
        let mut world = World::new(999);
        // 先冻结时钟：`World::now` 在首次 `advance_clock` 前取真实墙钟。
        world.advance_clock(0);
        world.seed_rng(seed);
        world.set_game_data(data);
        world.insert_map(MAP, common::synthetic_map(MAP));
        Self {
            world,
            receivers: HashMap::new(),
        }
    }

    fn add_player(&mut self, object_id: u32, class: MirClass, level: u16, position: Point) -> u32 {
        let stats = combat::player_base_stats(level, class);
        let (sender, receiver) = mpsc::unbounded_channel();
        self.world.enter(Player {
            object_id,
            session_id: u64::from(object_id),
            character_id: i64::from(object_id),
            name: format!("玩家{object_id}"),
            map: MAP.to_owned(),
            position,
            direction: MirDirection::Down,
            level,
            experience: 0,
            max_experience: 0,
            class,
            gender: MirGender::Male,
            hair: 1,
            hp: stats.hp,
            mp: stats.mp,
            stats,
            action_time: 0,
            attack_time: 0,
            struck_time: 0,
            regen_time: 0,
            death_time: 0,
            revive_time: 0,
            cell_time: 0,
            step_counter: 0,
            run_counter: 0,
            run_time: 0,
            dead: false,
            attack_mode: AttackMode::Peace,
            pk_points: 0,
            brown_time: 0,
            pk_point_time: 0,
            last_hitter: None,
            last_hit_time: 0,
            name_colour: mir_core::Color(-1),
            visible_monsters: BTreeSet::new(),
            visible_npcs: BTreeSet::new(),
            movement_time: 0,
            inventory: mir_server::items::new_inventory(),
            equipment: mir_server::items::new_equipment(),
            storage: mir_server::storage::new_storage(),
            magics: Vec::new(),
            magic: mir_server::MagicState::default(),
            gold: 0,
            credit: 0,
            visible_items: BTreeSet::new(),
            quests: Vec::new(),
            completed_quests: BTreeSet::new(),
            mail: mir_server::MailState::default(),
            buffs: Vec::new(),
            pot_health_amount: 0,
            pot_mana_amount: 0,
            pot_time: 0,
            unlock_curse: false,
            npc: mir_server::NpcState::default(),
            group_id: None,
            group_invitation: None,
            group_invite_time: 0,
            allow_group: true,
            guild: None,
            trade: mir_server::TradeState::default(),
            trade_invitation: None,
            next_trade_time: 0,
            allow_trade: true,
            bind_map_index: 0,
            bind_point: Point::default(),
            sender,
        });
        if let Some(player) = self.world.player_mut(object_id) {
            player.regen_time = i64::MAX;
        }
        self.receivers.insert(object_id, receiver);
        object_id
    }

    fn learn(&mut self, object_id: u32, spell: Spell) {
        self.world
            .learn_magic(object_id, spell)
            .expect("学习应当成功");
    }

    fn spawn(&mut self, kind: MonsterKind, position: Point) -> u32 {
        let object_id = self.world.spawn_monster(MAP, kind, position);
        if let Some(monster) = self.world.monster_mut(object_id) {
            monster.operate_time = -1;
            monster.regen_time = i64::MAX;
        }
        object_id
    }

    fn set_mp(&mut self, object_id: u32, mp: i32) {
        let Some(player) = self.world.player_mut(object_id) else {
            return;
        };
        player.stats.mp = mp;
        player.mp = mp;
    }

    /// 清动作/技能硬直（保留 `magic.casts`，供冷却断言），回满魔法。
    fn clear_timers(&mut self, object_id: u32) {
        let Some(player) = self.world.player_mut(object_id) else {
            return;
        };
        player.action_time = 0;
        player.attack_time = 0;
        player.magic.spell_time = 0;
        player.mp = player.stats.mp;
    }

    /// 只清技能冷却（用于「固定种子多次真实施法」的采样循环）。
    fn clear_cooldowns(&mut self, object_id: u32) {
        if let Some(player) = self.world.player_mut(object_id) {
            player.magic.casts.clear();
        }
    }

    /// 在根装备槽放一件真实模板物品（`added_freezing` 写 `added_stats`）。
    fn equip(
        &mut self,
        slot: usize,
        item_index: i32,
        current_dura: u16,
        count: u16,
        added_freezing: i32,
    ) {
        let mut item = UserItem {
            item_index,
            current_dura,
            max_dura: current_dura,
            count,
            ..UserItem::default()
        };
        item.added_stats.set(Stat::Freezing, added_freezing);
        self.world.player_mut(1).expect("玩家").equipment[slot] = Some(item);
    }

    /// 给玩家加一条只提供 `Freezing` 的真实增益（`ActiveBuff.stats`，Crystal `:2332`）。
    fn add_freezing_buff(&mut self, object_id: u32, freezing: i32) {
        let mut stats = Stats::default();
        stats.set(Stat::Freezing, freezing);
        assert!(
            self.world
                .add_buff(object_id, BuffKind::Haste, 60_000, stats, object_id),
            "增益应能加上"
        );
    }

    fn elapse(&mut self, ms: i64) {
        self.world.advance_clock(ms);
        self.world.tick();
    }

    fn cast(
        &mut self,
        object_id: u32,
        spell: Spell,
        direction: MirDirection,
        target_id: u32,
        location: Point,
    ) -> Result<CastReport, MagicError> {
        self.world
            .cast_magic(object_id, spell, direction, target_id, location)
    }

    fn packets(&mut self, object_id: u32) -> Vec<ServerPacket> {
        let receiver = self.receivers.get_mut(&object_id).expect("玩家通道");
        let mut packets = Vec::new();
        while let Ok(frame) = receiver.try_recv() {
            let index = i16::from_le_bytes([frame[2], frame[3]]);
            packets.push(ServerPacket::decode(index, &frame[4..]).expect("解码服务端包"));
        }
        packets
    }

    fn drain(&mut self, object_id: u32) {
        self.packets(object_id);
    }
}

/// 训练木桩（AI 56）：不走动、不攻击，但毒 tick 照常跑。
fn test_monster(hp: i32, min_mac: i32) -> MonsterKind {
    MonsterKind {
        name: "B4 木桩".to_owned(),
        image: mir_core::enums::Monster::Oma,
        ai: 56,
        level: 1,
        view_range: 1,
        cool_eye: 0,
        attack_speed_ms: 2_500,
        move_speed_ms: 1_800,
        experience: 0,
        can_push: true,
        stats: CombatStats {
            hp,
            min_mac,
            max_mac: min_mac,
            ..CombatStats::default()
        },
    }
}

/// 按技能语义构造真实施法参数（目标/落点），全部走 `World::cast_magic` 公开入口。
fn cast_learned(
    h: &mut Harness,
    player: u32,
    spell: Spell,
    monster: u32,
) -> Result<CastReport, MagicError> {
    let position = h.world.player(player).expect("玩家").position;
    let (target_id, location) = match spell {
        Spell::FrostCrunch | Spell::TrapHexagon => {
            let monster = h.world.monster(monster).expect("怪物");
            (monster.object_id, monster.position)
        }
        Spell::ThunderStorm | Spell::ShoulderDash => (0, position),
        Spell::Blink | Spell::StormEscape | Spell::PoisonCloud => {
            (0, Point::new(position.x, position.y + 3))
        }
        other => panic!("未覆盖技能 {other:?}"),
    };
    let direction = position.direction_to(location);
    h.cast(player, spell, direction, target_id, location)
}

/// 反复真实施放冰咆哮（每次清冷却/硬直/目标毒与血量），直到目标吃到 Frozen。
fn cast_frost_until_frozen(h: &mut Harness, caster: u32, monster: u32) -> PoisonState {
    for _ in 0..400 {
        h.clear_timers(caster);
        h.clear_cooldowns(caster);
        {
            let target = h.world.monster_mut(monster).expect("怪物");
            target.poisons.clear();
            target.hp = 10_000;
            target.stats.hp = 10_000;
            target.dead = false;
        }
        let report = cast_learned(h, caster, Spell::FrostCrunch, monster).expect("冰咆哮施法成功");
        assert!(report.damage.is_some(), "真实冰咆哮应产生伤害");
        // 飞行延迟 = MaxDistance*50 + 500（Crystal `HumanObject.cs:3939-3952`）。
        h.elapse(700);
        if let Some(frozen) = h
            .world
            .monster(monster)
            .expect("怪物")
            .poisons
            .iter()
            .find(|poison| poison.kind == PoisonType::Frozen)
        {
            return *frozen;
        }
    }
    panic!("400 次真实冰咆哮内应至少冻结一次");
}

/// 同一套装备/增益下，每个固定种子采样一次 Frozen 时长（真实施法链）。
fn frozen_durations(
    data: &Arc<GameData>,
    seeds: impl IntoIterator<Item = u64>,
    mut setup: impl FnMut(&mut Harness),
) -> Vec<u32> {
    let mut durations = Vec::new();
    for seed in seeds {
        let mut h = Harness::with_data(seed, Arc::clone(data));
        h.add_player(1, MirClass::Taoist, 60, Point::new(10, 10));
        h.set_mp(1, 100_000);
        assert!(
            h.world.run_gm_command(1, "@learn FrostCrunch 3"),
            "GM @learn 应被识别"
        );
        setup(&mut h);
        let monster = h.spawn(test_monster(10_000, 0), Point::new(12, 10));
        durations.push(cast_frost_until_frozen(&mut h, 1, monster).duration);
    }
    durations
}

// ---------------------------------------------------------------------------
// G4：默认资料（实际默认 GameData/DB 加载链）
// ---------------------------------------------------------------------------

#[test]
fn round52_regression_default_magic_records_and_cost() {
    let data = default_game_data();

    // 来源：实际默认 game.mnd 的字节、SHA-256、manifest 记录（非手造 fixture）。
    let bytes = std::fs::read(default_mnd_path()).expect("读 game.mnd");
    assert_eq!(bytes.len(), 283_257, "默认 game.mnd 字节数");
    assert_eq!(
        sha256_hex(&bytes),
        "fb67712fe65099e3b0d484e9fe6132ff3dca71ade75c86322ad5e7a8eea0a460",
        "默认 game.mnd 的来源 SHA-256"
    );
    let manifest: serde_json::Value = serde_json::from_slice(
        &std::fs::read(repo_root().join("game-assets/data/manifest.json")).expect("manifest"),
    )
    .expect("manifest JSON");
    assert_eq!(
        manifest["sha256"].as_str(),
        Some("fb67712fe65099e3b0d484e9fe6132ff3dca71ade75c86322ad5e7a8eea0a460")
    );
    assert_eq!(manifest["size_bytes"].as_u64(), Some(283_257));
    assert_eq!(manifest["counts"]["magics"].as_u64(), Some(107));
    assert_eq!(
        manifest["source"]["db"].as_str(),
        Some("resource/community/suprcode-crystal-database/cjlaaa\\Server.MirDB"),
        "默认资料来自 cjlaaa/Server.MirDB"
    );
    assert_eq!(data.magic_count(), 107, "默认资料 magics 条数");

    // 七技能逐字段硬编码期望（独立于被测字段，防止自证）。
    for expected in EXPECTED_MAGICS {
        let info = data
            .magic(expected.spell)
            .unwrap_or_else(|| panic!("默认数据缺 {:?}", expected.spell));
        assert_eq!(info.name, expected.name, "{:?} 名称", expected.spell);
        assert_eq!(
            info.spell, expected.spell_value,
            "{:?} Spell 编号",
            expected.spell
        );
        assert_eq!(
            info.base_cost, expected.base_cost,
            "{:?} 基础耗蓝",
            expected.spell
        );
        assert_eq!(
            info.level_cost, expected.level_cost,
            "{:?} 每级耗蓝",
            expected.spell
        );
        assert_eq!(info.icon, expected.icon, "{:?} 图标", expected.spell);
        assert_eq!(
            [info.level1, info.level2, info.level3],
            expected.levels,
            "{:?} 等级门槛",
            expected.spell
        );
        assert_eq!(
            [info.need1, info.need2, info.need3],
            expected.needs,
            "{:?} 修炼值门槛",
            expected.spell
        );
        assert_eq!(
            info.delay_base, expected.delay_base,
            "{:?} 基础冷却",
            expected.spell
        );
        assert_eq!(
            info.delay_reduction, expected.delay_reduction,
            "{:?} 冷却缩减",
            expected.spell
        );
        assert_eq!(info.range, expected.range, "{:?} 施法距离", expected.spell);
        assert_eq!(
            (
                info.power_base,
                info.power_bonus,
                info.m_power_base,
                info.m_power_bonus
            ),
            (
                expected.power_base,
                expected.power_bonus,
                expected.m_power_base,
                expected.m_power_bonus
            ),
            "{:?} 威力字段",
            expected.spell
        );
        assert_eq!(
            (info.multiplier_base, info.multiplier_bonus),
            (1.0, 0.0),
            "{:?} 倍率字段",
            expected.spell
        );
    }

    // 耗蓝/冷却公式（`HumanObject.cs:3375-3398`、`MagicInfo.cs:219-222`）。
    for expected in EXPECTED_MAGICS {
        let info = data.magic(expected.spell).expect("七技能存在");
        let magic = UserMagic::new(expected.spell);
        assert_eq!(
            magic.cost(info),
            i32::from(expected.base_cost),
            "{:?} 0 级耗蓝 = BaseCost",
            expected.spell
        );
        assert_eq!(
            magic.delay_ms(info),
            i64::from(expected.delay_base),
            "{:?} 0 级冷却 = DelayBase",
            expected.spell
        );
    }
    let dash = data.magic(Spell::ShoulderDash).expect("野蛮冲撞");
    assert_eq!(
        UserMagic::new(Spell::ShoulderDash).delay_ms(dash),
        2_500,
        "野蛮冲撞基础冷却 2500ms"
    );

    // 毒云默认资料：DelayBase=16000、DelayReduction=2000。
    // 契约文字写「毒雾等级3=12000ms」，但实际默认资料 3 级是 16000-3*2000=10000ms，
    // 12000ms 是 **2 级**值（Crystal `MagicInfo.cs:219-222`：DelayBase - Level*Reduction）。
    let poison = data.magic(Spell::PoisonCloud).expect("毒云");
    let level2 = UserMagic {
        spell: Spell::PoisonCloud,
        level: 2,
        key: 0,
        experience: 0,
    };
    let level3 = UserMagic { level: 3, ..level2 };
    assert_eq!(level2.delay_ms(poison), 12_000, "毒云 2 级冷却 = 12000ms");
    assert_eq!(level3.delay_ms(poison), 10_000, "毒云 3 级冷却 = 10000ms");
}

#[test]
fn round52_regression_default_magic_info_cost_cooldown() {
    let data = default_game_data();
    for spell in SEVEN_SPELLS {
        let info = data.magic(spell).expect("默认七技能存在").clone();
        let base_cost = i32::from(info.base_cost);
        let delay = i64::from(info.delay_base); // 0 级：无冷却缩减
        let mut h = Harness::with_data(7, Arc::clone(&data));
        h.add_player(1, MirClass::Warrior, 60, Point::new(10, 10));
        h.set_mp(1, 100_000);
        h.learn(1, spell);
        let monster = h.spawn(test_monster(10_000, 0), Point::new(12, 10));
        // 真实耗材：毒云需 5 护身符 + 5 灰色药粉；困魔咒需 1 护身符。
        match spell {
            Spell::PoisonCloud => {
                h.equip(9, 712, 0, 10, 0);
                h.equip(10, 710, 0, 10, 0);
            }
            Spell::TrapHexagon => h.equip(9, 712, 0, 5, 0),
            _ => {}
        }

        let t0 = h.world.now();
        let report = cast_learned(&mut h, 1, spell, monster)
            .unwrap_or_else(|error| panic!("{spell:?} 首次施法应成功：{error:?}"));
        assert_eq!(report.cost, base_cost, "{spell:?} 0 级耗蓝 = BaseCost");
        assert_eq!(
            h.world.player(1).expect("玩家").mp,
            100_000 - base_cost,
            "{spell:?} 应真实扣蓝"
        );
        assert_eq!(
            h.world.player(1).expect("玩家").magic.casts.get(&spell),
            Some(&t0),
            "{spell:?} 成功施法应记录 CastTime"
        );

        // 冷却前一毫秒：拒绝且给出精确到期时刻。
        h.clear_timers(1);
        h.elapse(delay - 1);
        let error = cast_learned(&mut h, 1, spell, monster).expect_err("冷却未到必须拒绝");
        match error {
            MagicError::CoolingDown { until } => {
                assert_eq!(until, t0 + delay, "{spell:?} 冷却到期 = CastTime + Delay");
            }
            other => panic!("{spell:?} 应为 CoolingDown，实际 {other:?}"),
        }

        // 到期后：再次成功，CostTime 刷新。
        h.clear_timers(1);
        h.elapse(1);
        let now = h.world.now();
        let report = cast_learned(&mut h, 1, spell, monster)
            .unwrap_or_else(|error| panic!("{spell:?} 冷却到期后应能再次施法：{error:?}"));
        let teleport_penalty = h.world.buff_value(1, BuffKind::TemporalFlux).is_some();
        let expected = if teleport_penalty && matches!(spell, Spell::Blink | Spell::StormEscape) {
            base_cost + base_cost * 30 / 100
        } else {
            base_cost
        };
        assert_eq!(
            report.cost, expected,
            "{spell:?} 到期后耗蓝（TemporalFlux 传送惩罚 30%）"
        );
        assert_eq!(
            h.world.player(1).expect("玩家").magic.casts.get(&spell),
            Some(&now),
            "{spell:?} 到期后 CastTime 刷新"
        );
    }
}

// ---------------------------------------------------------------------------
// G5：真实 Slow/Frozen（概率两侧）、玩家/怪物边界、Frozen 后真实拒攻
// 说明：玩家侧目前没有真实施法 Frozen 来源（`magic_target_hittable` 只扫怪），
// 本测试只覆盖怪物；玩家侧用真实 `PoisonState` 注入 + 真实 `World::attack`
// 在 `round52_regression_player_poison_state_real_attack_blocks_frozen_target` 覆盖。
// ---------------------------------------------------------------------------

#[test]
fn round52_regression_real_frost_crunch_slow_frozen_monsters() {
    let data = default_game_data();
    let mut slow_hits = 0_usize;
    let mut slow_misses = 0_usize;
    let mut frozen_hits = 0_usize;
    let mut frozen_misses = 0_usize;
    let mut frozen_durations_seen = Vec::new();

    for seed in 0..=23 {
        let mut h = Harness::with_data(seed, Arc::clone(&data));
        h.add_player(1, MirClass::Taoist, 60, Point::new(10, 10));
        h.set_mp(1, 100_000);
        assert!(
            h.world.run_gm_command(1, "@learn FrostCrunch 3"),
            "GM @learn 应被识别"
        );
        assert_eq!(
            h.world
                .player(1)
                .expect("玩家")
                .magics
                .iter()
                .find(|magic| magic.spell == Spell::FrostCrunch)
                .expect("已学")
                .level,
            3,
            "GM @learn FrostCrunch 3 应把技能等级设到 3"
        );
        let monster = h.spawn(test_monster(10_000, 0), Point::new(12, 10));

        for _ in 0..48 {
            h.clear_timers(1);
            h.clear_cooldowns(1);
            {
                let target = h.world.monster_mut(monster).expect("怪物");
                target.poisons.clear();
                target.hp = 10_000;
                target.stats.hp = 10_000;
                target.dead = false;
            }
            let report =
                cast_learned(&mut h, 1, Spell::FrostCrunch, monster).expect("冰咆哮施法成功");
            assert!(report.damage.is_some(), "真实冰咆哮应产生伤害");
            h.elapse(700);

            let now = h.world.now();
            let target = h.world.monster(monster).expect("怪物");
            match target.poisons.iter().find(|p| p.kind == PoisonType::Slow) {
                Some(poison) => {
                    slow_hits += 1;
                    assert!(poison.duration > 0, "Slow 毒时长必须非空");
                }
                None => slow_misses += 1,
            }
            match target.poisons.iter().find(|p| p.kind == PoisonType::Frozen) {
                Some(poison) => {
                    frozen_hits += 1;
                    frozen_durations_seen.push(poison.duration);
                    assert!(poison.duration > 0, "Frozen 毒时长必须非空");
                    assert!(
                        !target.can_attack(now),
                        "Frozen 怪生产谓词 can_attack 必须为 false"
                    );
                    assert!(
                        !target.can_move(now),
                        "Frozen 怪生产谓词 can_move 必须为 false"
                    );
                }
                None => frozen_misses += 1,
            }
        }
    }

    assert!(slow_hits > 0, "固定种子下 Slow 命中数必须非零（实际 0）");
    assert!(
        slow_misses > 0,
        "固定种子下 Slow 未命中数必须非零（实际 0）"
    );
    assert!(
        frozen_hits > 0,
        "固定种子下 Frozen 命中数必须非零（实际 0）"
    );
    assert!(
        frozen_misses > 0,
        "固定种子下 Frozen 未命中数必须非零（实际 0）"
    );
    assert!(
        frozen_durations_seen.iter().all(|duration| *duration == 5),
        "无 Freezing 来源时 Frozen 恒 5 跳（5 + Random.Next(0)），实际 {frozen_durations_seen:?}"
    );
}

#[test]
fn round52_regression_player_poison_state_real_attack_blocks_frozen_target() {
    let data = default_game_data();
    let mut h = Harness::with_data(11, data);
    h.add_player(1, MirClass::Warrior, 30, Point::new(10, 10));
    let monster = h.spawn(test_monster(10_000, 0), Point::new(11, 10));

    // 先证明正常攻击链路可用（否则后面的 Frozen 拒攻没有对照）。
    h.clear_timers(1);
    let report = h
        .world
        .attack(1, MirDirection::Right)
        .expect("正常攻击应成功")
        .expect("前方有怪应命中");
    assert_eq!(report.target, monster, "攻击目标");

    // 注入真实 `PoisonState`（结构由生产 `tick_player_poison` 结算；集成层拿不到
    // `pub(crate) apply_poison_to_player`，真实生产入口在 magic.rs 单测里覆盖）。
    let now = h.world.now();
    h.world
        .player_mut(1)
        .expect("玩家")
        .magic
        .poisons
        .push(PoisonState::new(PoisonType::Frozen, 0, 2, now + 1_000, 0).with_tick_speed(1_000));
    assert!(h.world.player(1).expect("玩家").is_frozen());

    h.clear_timers(1);
    let error = h
        .world
        .attack(1, MirDirection::Right)
        .expect_err("Frozen 必须拒绝真实攻击");
    assert!(
        matches!(error, CombatError::Frozen { object_id: 1 }),
        "应为 CombatError::Frozen，实际 {error:?}"
    );
    // 拒攻不消耗动作（Crystal `HumanObject.cs:142` 的 `CanAttack` 含 Frozen 判定，
    // `HumanObject.cs:2856` 的 Attack 入口 `if (!CanAttack)` 直接 return）。
    assert_eq!(h.world.player(1).expect("玩家").action_time, 0);
    assert_eq!(h.world.player(1).expect("玩家").attack_time, 0);

    // 两跳后毒到期，攻击恢复。
    h.elapse(1_100);
    h.elapse(1_100);
    assert!(
        !h.world.player(1).expect("玩家").is_frozen(),
        "两跳后 Frozen 应移除"
    );
    h.clear_timers(1);
    assert!(
        h.world
            .attack(1, MirDirection::Right)
            .expect("解冻后应能攻击")
            .is_some(),
        "解冻后应能真实命中"
    );
}

#[test]
fn round52_regression_frozen_blocks_real_attack_and_freezing_sources() {
    let data = default_game_data();

    // ① 玩家 Frozen 由真实 `PoisonState` 注入后跑真实 `World::attack` 拒攻。
    //    集成层无法调 `pub(crate) apply_poison_to_player`，且玩家侧目前没有
    //    真实施法 Frozen 来源（`magic_target_hittable` 只扫怪）→ 这是已知缺口，
    //    真实生产注入入口在 `magic.rs` 单测 `player_frozen_blocks_real_attack` 覆盖。
    let mut h = Harness::with_data(21, Arc::clone(&data));
    h.add_player(1, MirClass::Warrior, 30, Point::new(10, 10));
    h.spawn(test_monster(10_000, 0), Point::new(11, 10));
    let now = h.world.now();
    h.world
        .player_mut(1)
        .expect("玩家")
        .magic
        .poisons
        .push(PoisonState::new(PoisonType::Frozen, 0, 2, now + 1_000, 0).with_tick_speed(1_000));
    h.clear_timers(1);
    assert!(
        matches!(
            h.world.attack(1, MirDirection::Right),
            Err(CombatError::Frozen { object_id: 1 })
        ),
        "Frozen 玩家真实攻击必须返回 CombatError::Frozen"
    );

    // ② 真实施法冻结怪物 → 生产谓词 `can_attack/can_move` 必须否决。
    let mut h = Harness::with_data(22, Arc::clone(&data));
    h.add_player(1, MirClass::Taoist, 60, Point::new(10, 10));
    h.set_mp(1, 100_000);
    assert!(h.world.run_gm_command(1, "@learn FrostCrunch 3"));
    let monster = h.spawn(test_monster(10_000, 0), Point::new(12, 10));
    let frozen = cast_frost_until_frozen(&mut h, 1, monster);
    let now = h.world.now();
    assert!(!h.world.monster(monster).expect("怪物").can_attack(now));
    assert!(!h.world.monster(monster).expect("怪物").can_move(now));
    assert!(frozen.duration >= 5, "真实 Frozen 时长 = 5 + roll");

    // ③ Freezing=7 上界：真实默认物品（index 1 祈祷之剑，durability=20000）+ added 7。
    //    每条 Frozen = 5 + Random.Next(7) ∈ 5..=11（R51 终审 v4 的 12 上界应收紧）。
    let mut over_five = 0_usize;
    for seed in 0..=23 {
        let mut h = Harness::with_data(seed, Arc::clone(&data));
        h.add_player(1, MirClass::Taoist, 60, Point::new(10, 10));
        h.set_mp(1, 100_000);
        assert!(h.world.run_gm_command(1, "@learn FrostCrunch 3"));
        h.equip(0, 1, 20_000, 1, 7);
        let monster = h.spawn(test_monster(10_000, 0), Point::new(12, 10));
        let frozen = cast_frost_until_frozen(&mut h, 1, monster);
        assert!(
            (5..=11).contains(&frozen.duration),
            "Freezing=7 → 时长 5..=11（Crystal HumanObject.cs:5987），实际 {}",
            frozen.duration
        );
        over_five += usize::from(frozen.duration > 5);
    }
    assert!(
        over_five > 0,
        "Freezing=7 的固定种子样本中应至少有一次 roll>0（时长>5），证明 roll 真实生效"
    );
}

#[test]
fn round52_regression_freezing_sources_and_rng_state() {
    let data = default_game_data();

    // A. 坏装备（模板 2、耐久>0、current_dura=0 → 整件跳过，Crystal `:1836`）+ 有效 buff 3。
    let damaged = frozen_durations(&data, 0..=11, |h| {
        h.equip(0, 251, 0, 1, 5); // 月轮剑模板 Freezing=2，当前耐久 0 → 模板+附加都跳过
        h.add_freezing_buff(1, 3);
    });
    assert!(
        damaged.iter().all(|duration| (5..=7).contains(duration)),
        "坏装备+buff3 → 5..=7（buff 仍生效），实际 {damaged:?}"
    );
    assert!(
        damaged.iter().any(|duration| *duration > 5),
        "坏装备+buff3 样本应至少一次 roll>0，实际 {damaged:?}"
    );

    // B. 0 耐久模板（护身符 712，durability=0 → 不跳过）+ added 4 → 5..=8。
    let zero_durability = frozen_durations(&data, 0..=11, |h| {
        h.equip(0, 712, 0, 1, 4);
    });
    assert!(
        zero_durability
            .iter()
            .all(|duration| (5..=8).contains(duration)),
        "0 耐久+added4 → 5..=8，实际 {zero_durability:?}"
    );
    assert!(
        zero_durability.iter().any(|duration| *duration > 5),
        "0 耐久+added4 样本应至少一次 roll>0，实际 {zero_durability:?}"
    );

    // C. 多有效根槽叠加：槽0 月轮剑（模板 2 + 附加 5）+ 槽1 酷寒宝玉（模板 1 + 附加 4）= 12。
    let multi_slot = frozen_durations(&data, 0..=23, |h| {
        h.equip(0, 251, 20_000, 1, 5);
        h.equip(1, 744, 100, 1, 4);
    });
    assert!(
        multi_slot
            .iter()
            .all(|duration| (5..=16).contains(duration)),
        "12 点 Freezing → 5..=16，实际 {multi_slot:?}"
    );
    assert!(
        multi_slot.iter().any(|duration| *duration >= 14),
        "多槽叠加总和 12 时样本应出现 ≥14（roll≥9），实际 {multi_slot:?}"
    );

    // D. 无来源：Freezing=0 不抽 RNG，Frozen 恒 5 跳。
    let none = frozen_durations(&data, 0..=11, |_| {});
    assert!(
        none.iter().all(|duration| *duration == 5),
        "无 Freezing 来源 → 恒 5 跳，实际 {none:?}"
    );
}

// ---------------------------------------------------------------------------
// G5：StormEscape 真实升级 / 五位置包序 / 跨图
// ---------------------------------------------------------------------------

#[test]
fn round52_regression_storm_escape_cross_map_and_rank() {
    let data = default_game_data();

    // ① 0 级：Random.Next(4) > Level 概率失败（3/4）→ 不传送、不加 TemporalFlux。
    let mut failing_seed = None;
    for seed in 0..=15 {
        let mut h = Harness::with_data(seed, Arc::clone(&data));
        h.add_player(1, MirClass::Taoist, 60, Point::new(10, 10));
        h.set_mp(1, 100_000);
        h.learn(1, Spell::StormEscape);
        let report = h
            .cast(
                1,
                Spell::StormEscape,
                MirDirection::Down,
                0,
                Point::new(10, 13),
            )
            .expect("0 级也应能施法");
        assert_eq!(report.cost, 65, "0 级耗蓝 = BaseCost");
        h.elapse(750);
        if h.world.player(1).expect("玩家").position == Point::new(10, 10) {
            assert!(
                h.world.buff_value(1, BuffKind::TemporalFlux).is_none(),
                "传送失败不应加 TemporalFlux"
            );
            failing_seed = Some(seed);
            break;
        }
    }
    let failing_seed = failing_seed.expect("0 级在 16 个固定种子里应至少失败一次（失败概率 3/4）");

    // ② 同 seed 升到 3 级：Random.Next(4) > 3 不可能 → 必成功。
    let mut h = Harness::with_data(failing_seed, Arc::clone(&data));
    h.add_player(1, MirClass::Taoist, 60, Point::new(10, 10));
    h.set_mp(1, 100_000);
    assert!(
        h.world.run_gm_command(1, "@learn StormEscape 3"),
        "GM @learn 应被识别"
    );
    let learned = *h
        .world
        .player(1)
        .expect("玩家")
        .magics
        .iter()
        .find(|magic| magic.spell == Spell::StormEscape)
        .expect("已学");
    assert_eq!(learned.level, 3, "GM @learn StormEscape 3 应设到 3 级");
    let report = h
        .cast(
            1,
            Spell::StormEscape,
            MirDirection::Down,
            0,
            Point::new(10, 13),
        )
        .expect("3 级施法成功");
    assert_eq!(report.cost, 89, "3 级耗蓝 = 65 + 8*3");
    h.elapse(750);
    assert_eq!(
        h.world.player(1).expect("玩家").position,
        Point::new(10, 13),
        "3 级 Random.Next(4) > 3 不可能，必传送"
    );
    assert!(
        h.world.buff_value(1, BuffKind::TemporalFlux).is_some(),
        "成功传送加 TemporalFlux"
    );
    let learned = *h
        .world
        .player(1)
        .expect("玩家")
        .magics
        .iter()
        .find(|magic| magic.spell == Spell::StormEscape)
        .expect("已学");
    assert_eq!(learned.level, 3);
    assert_eq!(
        learned.experience, 0,
        "满级 level_magic 早退，修炼值不变（Crystal case 2 升级清经验 `HumanObject.cs:6964`；满级 default 早退 `:6968-6969`）"
    );

    // ③ 真实训练路径：0 级成功传送 → 修炼值增加并回 MagicLeveled 包。
    let mut trained = false;
    for seed in 0..=63 {
        let mut h = Harness::with_data(seed, Arc::clone(&data));
        h.add_player(1, MirClass::Taoist, 60, Point::new(10, 10));
        h.set_mp(1, 100_000);
        h.learn(1, Spell::StormEscape);
        h.drain(1);
        h.cast(
            1,
            Spell::StormEscape,
            MirDirection::Down,
            0,
            Point::new(10, 13),
        )
        .expect("施法成功");
        h.elapse(750);
        if h.world.player(1).expect("玩家").position == Point::new(10, 10) {
            continue;
        }
        let learned = *h
            .world
            .player(1)
            .expect("玩家")
            .magics
            .iter()
            .find(|magic| magic.spell == Spell::StormEscape)
            .expect("已学");
        assert!(
            learned.experience > 0,
            "成功传送应累计修炼值（need1=2200 未满，实际 {}）",
            learned.experience
        );
        let packets = h.packets(1);
        assert!(
            packets.iter().any(
                |packet| matches!(packet, ServerPacket::MagicLeveled(m) if m.spell == Spell::StormEscape)
            ),
            "修炼应回 MagicLeveled 包"
        );
        trained = true;
        break;
    }
    assert!(
        trained,
        "64 个固定种子里应至少有一次 0 级传送成功（成功概率 1/4）"
    );

    // ④ 跨图：换图会丢弃该玩家的 pending_magic（`maps.rs:525-526` 有意偏差）。
    //    Crystal 的 `ActionList` 是 `MapObject.cs:182` 的字段，仅在 `Despawn`
    //    （`MapObject.cs:370`）清空；换图走 `PlayerObject.cs:4515-4618`
    //    `CompleteMapMovement`，不会丢。因此这里断言的是本作已记录的
    //    生命周期偏差（D3 PARTIAL），不是等价。
    let mut h = Harness::with_data(4, Arc::clone(&data));
    h.add_player(1, MirClass::Taoist, 60, Point::new(10, 10));
    h.add_player(2, MirClass::Taoist, 60, Point::new(12, 10));
    h.set_mp(1, 100_000);
    assert!(h.world.run_gm_command(1, "@learn StormEscape 3"));
    h.world.insert_map("1", common::synthetic_map("1"));
    h.drain(1);
    h.drain(2);
    h.cast(
        1,
        Spell::StormEscape,
        MirDirection::Down,
        0,
        Point::new(10, 13),
    )
    .expect("跨图前施法成功");
    h.world
        .change_map(1, "1", Point::new(20, 20), MirDirection::Down)
        .expect("结算前换图");
    h.drain(2);
    h.elapse(750);
    let caster = h.world.player(1).expect("玩家");
    assert_eq!(caster.map, "1", "换图后施法者在新图");
    assert_eq!(
        caster.position,
        Point::new(20, 20),
        "本作换图丢弃 pending_magic → 不会在旧图/新图传送（D3 偏差）"
    );
    assert!(
        h.world.buff_value(1, BuffKind::TemporalFlux).is_none(),
        "pending 被丢弃 → 无 TemporalFlux"
    );
    let self_packets = h.packets(1);
    assert!(
        !self_packets.iter().any(
            |packet| matches!(packet, ServerPacket::ObjectEffect(effect) if effect.effect == SpellEffect::StormEscape)
        ),
        "pending 被丢弃 → 本人不再收到狂风光效"
    );
    let observer_packets = h.packets(2);
    assert!(
        !observer_packets.iter().any(|packet| matches!(
            packet,
            ServerPacket::ObjectTeleportOut(_) | ServerPacket::ObjectTeleportIn(_)
        )),
        "原图观察者不应收到传送包"
    );
}

#[test]
fn round52_regression_stormescape_order_cross_map() {
    let data = default_game_data();

    // ① 五位置包序（先断言 is_some，再比较顺序；Crystal `HumanObject.cs:6080-6092`）。
    let mut h = Harness::with_data(3, Arc::clone(&data));
    h.add_player(1, MirClass::Taoist, 60, Point::new(10, 10));
    h.add_player(2, MirClass::Taoist, 60, Point::new(12, 10));
    h.set_mp(1, 100_000);
    assert!(h.world.run_gm_command(1, "@learn StormEscape 3"));
    h.drain(1);
    h.drain(2);
    h.cast(
        1,
        Spell::StormEscape,
        MirDirection::Down,
        0,
        Point::new(10, 13),
    )
    .expect("3 级狂风施法成功");
    h.elapse(750);
    assert_eq!(
        h.world.player(1).expect("玩家").position,
        Point::new(10, 13),
        "3 级必传送"
    );

    let caster_packets = h.packets(1);
    let caster_effect = caster_packets
        .iter()
        .position(|packet| {
            matches!(packet, ServerPacket::ObjectEffect(effect) if effect.effect == SpellEffect::StormEscape)
        })
        .expect("本人必须收到 ObjectEffect{StormEscape}");
    let caster_buff = caster_packets
        .iter()
        .position(|packet| {
            matches!(packet, ServerPacket::AddBuff(add) if add.buff.buff_type == BuffType::TemporalFlux)
        })
        .expect("本人必须收到 AddBuff(TemporalFlux)");
    assert!(
        caster_effect < caster_buff,
        "本人顺序：ObjectEffect(StormEscape) → AddBuff(TemporalFlux)（光效先于增益）"
    );

    let observer_packets = h.packets(2);
    let observer_out = observer_packets
        .iter()
        .position(|packet| matches!(packet, ServerPacket::ObjectTeleportOut(_)))
        .expect("观察者必须收到 ObjectTeleportOut");
    let observer_in = observer_packets
        .iter()
        .position(|packet| matches!(packet, ServerPacket::ObjectTeleportIn(_)))
        .expect("观察者必须收到 ObjectTeleportIn");
    let observer_effect = observer_packets
        .iter()
        .position(|packet| {
            matches!(packet, ServerPacket::ObjectEffect(effect) if effect.effect == SpellEffect::StormEscape)
        })
        .expect("观察者必须收到 ObjectEffect{StormEscape}");
    assert!(
        observer_out < observer_in && observer_in < observer_effect,
        "观察者顺序：ObjectTeleportOut → ObjectTeleportIn → ObjectEffect(StormEscape)"
    );

    // ② 跨图例：把施法者与观察者都换到 "1" 后再次真实施法，包序在新图同样成立。
    h.world.insert_map("1", common::synthetic_map("1"));
    h.world
        .change_map(1, "1", Point::new(20, 20), MirDirection::Down)
        .expect("施法者换图");
    h.world
        .change_map(2, "1", Point::new(22, 20), MirDirection::Down)
        .expect("观察者换图");
    h.clear_timers(1);
    h.clear_cooldowns(1);
    h.set_mp(1, 100_000);
    h.drain(1);
    h.drain(2);
    h.cast(
        1,
        Spell::StormEscape,
        MirDirection::Down,
        0,
        Point::new(20, 23),
    )
    .expect("跨图后施法成功");
    h.elapse(750);
    let caster = h.world.player(1).expect("玩家");
    assert_eq!(caster.map, "1", "落图结算使用施法者当前图");
    assert_eq!(caster.position, Point::new(20, 23), "落点在新图");
    assert!(h.world.buff_value(1, BuffKind::TemporalFlux).is_some());
    let caster_packets = h.packets(1);
    let caster_effect = caster_packets
        .iter()
        .position(|packet| {
            matches!(packet, ServerPacket::ObjectEffect(effect) if effect.effect == SpellEffect::StormEscape)
        })
        .expect("新图本人必须收到 ObjectEffect{StormEscape}");
    let caster_buff = caster_packets
        .iter()
        .position(|packet| {
            matches!(packet, ServerPacket::AddBuff(add) if add.buff.buff_type == BuffType::TemporalFlux)
        })
        .expect("新图本人必须收到 AddBuff(TemporalFlux)");
    assert!(caster_effect < caster_buff, "新图本人顺序仍为 光效 → 增益");
    let observer_packets = h.packets(2);
    let observer_out = observer_packets
        .iter()
        .position(|packet| matches!(packet, ServerPacket::ObjectTeleportOut(_)))
        .expect("新图观察者必须收到 ObjectTeleportOut");
    let observer_in = observer_packets
        .iter()
        .position(|packet| matches!(packet, ServerPacket::ObjectTeleportIn(_)))
        .expect("新图观察者必须收到 ObjectTeleportIn");
    let observer_effect = observer_packets
        .iter()
        .position(|packet| {
            matches!(packet, ServerPacket::ObjectEffect(effect) if effect.effect == SpellEffect::StormEscape)
        })
        .expect("新图观察者必须收到 ObjectEffect{StormEscape}");
    assert!(
        observer_out < observer_in && observer_in < observer_effect,
        "新图观察者顺序：Out → In → Effect"
    );
}
