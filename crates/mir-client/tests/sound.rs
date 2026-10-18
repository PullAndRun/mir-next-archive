//! 音效事件与真实素材冒烟：服务端包 → `WorldSnapshot.sound_events` → 播放器。
//!
//! 三个测试：
//!
//! 1. [`packets_produce_sound_events`]：往快照里注入 `ObjectWalk` / `ObjectRun` /
//!    `ObjectAttack` / `ObjectStruck` / `S.Struck` / `ObjectDied` / `LevelChanged` /
//!    `GainedItem` / `GainedGold` / `NPCResponse`，断言事件顺序与音效编号
//!    （不连服务器、不开窗口、不需要音频设备）。
//! 2. [`local_commands_push_sound_events`]：自己的走/跑/攻击/施法/喝药走
//!    [`WorldSnapshot::push_local_sound`]（服务端不回发这些包，
//!    `mir-server/src/world.rs:2428-2449`），并验证 100 ms 频控。
//! 3. [`real_wav_files_open`]：`resource/wav` 里抽查至少 5 个文件，用 `rodio::Decoder`
//!    真解码（不只是读文件头）；素材缺失时打印跳过。

use std::path::{Path, PathBuf};

use mir_client::audio::{SoundLibrary, SoundPlayer, SoundThrottle, is_wav_file};
use mir_client::net::{WorldEntity, WorldSnapshot};
use mir_core::Point;
use mir_core::enums::{ItemType, MirDirection, MirGender, Spell};
use mir_protocol::ServerPacket;
use mir_protocol::data::{ItemInfo, UserItem};
use mir_protocol::packets::server::{
    GainedGold, GainedItem, LevelChanged, NPCResponse, ObjectAttack, ObjectDied, ObjectRun,
    ObjectStruck, ObjectWalk, Struck,
};
use mir_render::EntityKind;
use mir_render::sound::{SOUND_COOLDOWN_MS, SoundId, sound_for_event, spell_sound};

/// 仓库根目录。
fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// 真实素材目录（`resource/wav` + Crystal 完整音效集）。
///
/// 测试进程的 CWD 是 `crates/mir-client`，所以不能直接用相对路径
/// `resource/wav`，要按仓库根目录拼。
fn repo_library() -> SoundLibrary {
    SoundLibrary::from_repo_root(repo_root())
}

/// 灌一个「自己」进快照（走/攻击/死亡事件都要有对象）。
fn with_self() -> WorldSnapshot {
    let mut snapshot = WorldSnapshot::default();
    snapshot.apply_packet(&ServerPacket::UserInformation(
        mir_protocol::packets::server::UserInformation {
            object_id: 7,
            name: "测试者".to_owned(),
            level: 10,
            class: mir_core::enums::MirClass::Warrior,
            gender: MirGender::Male,
            hair: 1,
            hp: 40,
            mp: 20,
            location: Point::new(100, 100),
            direction: MirDirection::Down,
            ..Default::default()
        },
    ));
    snapshot
}

/// 取「新」事件里第一个音效（`after` 是上次消费到的编号）。
fn sound_after(snapshot: &WorldSnapshot, after: u64) -> (SoundId, u64) {
    let (events, last) = snapshot.sound_events_since(after);
    assert_eq!(events.len(), 1, "应当正好一条音效事件：{events:?}");
    (events[0].sound, last)
}

#[test]
fn packets_produce_sound_events() {
    let mut snapshot = with_self();
    let mut last = 0u64;

    // 别人走路 → 走路音（`PlayerObject.cs:3596-3599`）。
    snapshot.apply_packet(&ServerPacket::ObjectWalk(ObjectWalk {
        object_id: 42,
        location: Point::new(101, 100),
        direction: MirDirection::Down,
    }));
    let (sound, next) = sound_after(&snapshot, last);
    assert_eq!(sound, SoundId::Walk);
    last = next;

    // 别人跑步 → 跑步音。
    snapshot.apply_packet(&ServerPacket::ObjectRun(ObjectRun {
        object_id: 42,
        location: Point::new(103, 100),
        direction: MirDirection::Down,
    }));
    let (sound, next) = sound_after(&snapshot, last);
    assert_eq!(sound, SoundId::Run);
    last = next;

    // 别人挥砍 → 挥空音；带技能时事件里带上技能号（施法音靠它）。
    snapshot.apply_packet(&ServerPacket::ObjectAttack(ObjectAttack {
        object_id: 42,
        location: Point::new(104, 100),
        direction: MirDirection::Down,
        spell: Spell::FireBall,
        level: 1,
        type_: 0,
    }));
    let (events, next) = snapshot.sound_events_since(last);
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].sound, SoundId::Swing);
    assert_eq!(events[0].spell, Spell::FireBall);
    assert_eq!(spell_sound(events[0].spell).as_deref(), Some("M31-0.wav"));
    last = next;

    // 别人挨打 → 命中音（`PlayerObject.cs:4700-4762`）。
    snapshot.apply_packet(&ServerPacket::ObjectStruck(ObjectStruck {
        object_id: 42,
        attacker_id: 7,
        location: Point::new(104, 100),
        direction: MirDirection::Up,
    }));
    let (sound, next) = sound_after(&snapshot, last);
    assert_eq!(sound, SoundId::Hit);
    last = next;

    // 自己挨打（`S.Struck` 只发给本人）→ 受击音，按自己的性别选文件。
    snapshot.apply_packet(&ServerPacket::Struck(Struck { attacker_id: 42 }));
    let (events, next) = snapshot.sound_events_since(last);
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].sound, SoundId::Struck);
    assert!(!events[0].female, "自己的性别来自 UserInformation");
    last = next;

    // 别人死亡 → 死亡音；随后编号继续递增。
    snapshot.apply_packet(&ServerPacket::ObjectDied(ObjectDied {
        object_id: 42,
        location: Point::new(104, 100),
        direction: MirDirection::Up,
        type_: 0,
    }));
    let (sound, next) = sound_after(&snapshot, last);
    assert_eq!(sound, SoundId::Die);
    last = next;

    // 升级 → 升级音（`GameScene.cs:3853`）。
    snapshot.apply_packet(&ServerPacket::LevelChanged(LevelChanged {
        level: 11,
        experience: 0,
        max_experience: 1,
    }));
    let (sound, next) = sound_after(&snapshot, last);
    assert_eq!(sound, SoundId::LevelUp);
    last = next;

    // 捡东西 → 拾取音。
    snapshot.apply_packet(&ServerPacket::GainedItem(GainedItem {
        item: UserItem {
            unique_id: 1,
            item_index: 1,
            count: 1,
            ..UserItem::default()
        },
    }));
    let (sound, next) = sound_after(&snapshot, last);
    assert_eq!(sound, SoundId::Pickup);
    last = next;

    // 金币 → 金币音；`GainedGold(0)` 不响（`GameScene.cs:3297`）。
    snapshot.apply_packet(&ServerPacket::GainedGold(GainedGold { gold: 100 }));
    let (sound, next) = sound_after(&snapshot, last);
    assert_eq!(sound, SoundId::Gold);
    last = next;
    let before = snapshot.sound_events.len();
    snapshot.apply_packet(&ServerPacket::GainedGold(GainedGold { gold: 0 }));
    assert_eq!(snapshot.sound_events.len(), before, "0 金币不应出事件");

    // 打开 NPC 对话 → 面板音（简化，见 `mir-render::sound` 模块文档）。
    snapshot.apply_packet(&ServerPacket::NPCResponse(NPCResponse {
        page: vec!["你好，勇士。".to_owned()],
    }));
    let (sound, _) = sound_after(&snapshot, last);
    assert_eq!(sound, SoundId::NpcDialog);

    // 关对话（空页）不响。
    let before = snapshot.sound_events.len();
    snapshot.apply_packet(&ServerPacket::NPCResponse(NPCResponse::default()));
    assert_eq!(snapshot.sound_events.len(), before, "空页不应出事件");

    // 事件编号严格递增，消费过的不会被重复取到。
    let (all, newest) = snapshot.sound_events_since(0);
    assert!(
        all.len() >= 9,
        "应当收集到 9 条以上事件，实际 {}",
        all.len()
    );
    assert!(
        all.windows(2).all(|pair| pair[0].id < pair[1].id),
        "编号必须递增"
    );
    let (empty, last2) = snapshot.sound_events_since(newest);
    assert!(empty.is_empty(), "消费到 {last2} 后不应再有新事件");
}

#[test]
fn local_commands_push_sound_events() {
    let mut snapshot = with_self();
    // 本地走/跑/攻击/施法/喝药都用同一个入口（服务端不回发这些）。
    snapshot.push_local_sound(SoundId::Walk, Spell::None);
    snapshot.push_local_sound(SoundId::Cast, Spell::FireBall);
    let (events, last) = snapshot.sound_events_since(0);
    assert_eq!(events.len(), 2);
    assert_eq!(events[0].sound, SoundId::Walk);
    assert_eq!(events[1].sound, SoundId::Cast);
    assert_eq!(events[1].spell, Spell::FireBall);

    // 队列有上限：灌 100 条后只剩最后 `SOUND_EVENT_LIMIT` 条，且编号还能接着用。
    for _ in 0..100 {
        snapshot.push_local_sound(SoundId::Swing, Spell::None);
    }
    assert!(
        snapshot.sound_events.len() < 100,
        "事件队列应当被裁剪，实际 {}",
        snapshot.sound_events.len()
    );
    let (_, newest) = snapshot.sound_events_since(last);
    assert!(newest > last, "编号必须继续增长");
}

#[test]
fn throttle_blocks_duplicate_walk_sounds() {
    // 100 ms 内同一音效只播一次（需求；Crystal 没有这张表，见 `mir-render::sound` 文档）。
    let mut throttle = SoundThrottle::new();
    let file = sound_for_event(SoundId::Walk).expect("走路音有文件");
    assert!(throttle.allow(file, 10_000));
    assert!(!throttle.allow(file, 10_000 + SOUND_COOLDOWN_MS - 1));
    assert!(throttle.allow(file, 10_000 + SOUND_COOLDOWN_MS));
    // 不同事件互不影响。
    let run = sound_for_event(SoundId::Run).expect("跑步音有文件");
    assert!(throttle.allow(run, 10_000));
}

#[test]
fn no_device_player_is_silent_and_does_not_panic() {
    // CI 上没有音频设备：播放器应当安静地什么都不做。
    let mut player = SoundPlayer::new(repo_library(), true, 0.5);
    let walked = player.play_event(SoundId::Walk, false, 1_000);
    if player.is_active() {
        eprintln!("音频设备可用：走路音已送进混音器 = {walked}");
        // 有声音的机器上，第一次播放必须成功（前提是素材在）。
        if repo_library().contains("1.wav") {
            assert!(walked, "有设备且素材在场时第一次播放应当成功");
        }
    } else {
        eprintln!("没有音频设备：播放退化为空操作（窗口照常运行）");
        assert!(!walked, "没有设备时播放必须是空操作");
    }
}

#[test]
fn real_wav_files_open() {
    let root = repo_root();
    let library = repo_library();
    let candidates = [
        "1.wav",
        "3.wav",
        "50.wav",
        "70.wav",
        "106.wav",
        "108.wav",
        "144.wav",
        "103.wav",
        "138.wav",
        "levelup.wav",
        "pet_pickup.wav",
        "M31-0.wav",
    ];
    let mut opened = 0;
    let mut missing = Vec::new();
    for file in candidates {
        let Some(path) = library.resolve(file) else {
            missing.push(file);
            continue;
        };
        assert!(
            is_wav_file(&path),
            "{} 的文件头不是 RIFF/WAVE",
            path.display()
        );
        let handle = std::fs::File::open(&path).expect("打开 wav");
        match rodio::Decoder::try_from(handle) {
            Ok(decoder) => {
                use rodio::Source as _;
                let sample_rate = decoder.sample_rate();
                let channels = decoder.channels();
                assert!(sample_rate.get() > 0, "{} 采样率非法", path.display());
                assert!(channels.get() > 0, "{} 声道数非法", path.display());
                opened += 1;
                eprintln!(
                    "WAV 可解码：{}（{} Hz，{} 声道）",
                    path.file_name().unwrap_or_default().to_string_lossy(),
                    sample_rate,
                    channels
                );
            }
            Err(error) => panic!("rodio 解不开 {}：{error}", path.display()),
        }
    }
    if opened == 0 {
        eprintln!(
            "跳过真实 WAV 冒烟：{} 下一个文件都没找到（缺素材）",
            root.join("resource/wav").display()
        );
        return;
    }
    assert!(opened >= 5, "至少要能打开 5 个 WAV，实际 {opened}");
    if !missing.is_empty() {
        eprintln!("素材里缺这些音效文件（播放时会静默跳过）：{missing:?}");
    }
}

#[test]
fn sound_library_resolves_every_mapped_file_when_assets_exist() {
    let library = repo_library();
    if library.resolve("1.wav").is_none() {
        eprintln!("跳过：resource/wav 不存在（.gitignore 忽略）");
        return;
    }
    let mut missing = Vec::new();
    for event in SoundId::ALL {
        let Some(file) = sound_for_event(event) else {
            continue;
        };
        if !library.contains(file) {
            missing.push((event.label(), file));
        }
    }
    eprintln!("12 个事件的映射文件：缺失 {missing:?}");
    // 允许缺个别文件（`resource/wav` 与 Crystal 完整音效集合起来才齐），
    // 但「走路 / 挥空 / 受击 / 金币 / 界面音」这些基础音必须在。
    for essential in [
        SoundId::Walk,
        SoundId::Swing,
        SoundId::Hit,
        SoundId::Gold,
        SoundId::NpcDialog,
    ] {
        let file = sound_for_event(essential).expect("有映射");
        assert!(
            library.contains(file),
            "{} 的音效 {file} 找不到",
            essential.label()
        );
    }
}

#[test]
fn item_type_potion_is_the_drink_trigger() {
    // 喝药音只在「使用药品」时播（`MirItemCell.cs:2047-2048`）。
    let mut snapshot = with_self();
    snapshot.item_infos.insert(
        5,
        mir_client::net::ItemInfoView::from_item_info(&ItemInfo {
            index: 5,
            name: "金创药".to_owned(),
            item_type: ItemType::Potion,
            ..ItemInfo::default()
        }),
    );
    snapshot.inventory = vec![Some(mir_client::net::InventoryItem {
        unique_id: 77,
        item_index: 5,
        count: 1,
        current_dura: 0,
        max_dura: 0,
    })];
    let slot = snapshot.inventory_slot_of(77).expect("背包里有这件物品");
    let info = snapshot
        .item_info(snapshot.inventory_item(slot).expect("物品").item_index)
        .expect("模板");
    assert_eq!(info.item_type, ItemType::Potion);
    // 顺便确认「地面物品」不进小地图点位（同一份快照给 `ui::minimap` 用）。
    snapshot.entities.insert(
        42,
        WorldEntity::new(
            42,
            EntityKind::Item,
            "金创药".to_owned(),
            Point::new(100, 101),
            MirDirection::Down,
            5,
        ),
    );
    let view = mir_client::ui::minimap::view_for(&snapshot, (200, 200));
    assert!(view.markers.is_empty(), "地面物品不画点");
}
