//! 任务系统的端到端测试：脚本解析、接取前置、击杀/收集进度、交付领奖、超时、放弃、
//! NPC `CHECKQUEST` / `FINISHQUEST` 集成、存档往返与真实数据冒烟。
//!
//! 全部确定性：
//! * 世界随机数固定种子（[`World::seed_rng`]）；
//! * 时间用 [`World::advance_clock`] 快进（任务的时间戳走 `World::wall_now`，一起被驱动）；
//! * 脚本与数据库都在内存/临时目录里构造（每个用例一套独立目录），
//!   只有 `real_*` 开头的用例读 `resource/` 里的真实数据，缺失时跳过。
//!
//! 每个断言尽量对应 Crystal 的一处行为，注释里写了出处（`文件.cs:行号`）。

use std::collections::{BTreeSet, HashMap};
use std::path::PathBuf;
use std::sync::Arc;

use mir_core::Point;
use mir_core::enums::{ItemGrade, ItemType, MirClass, MirDirection, MirGender, QuestState, Stat};
use mir_core::i18n::{Language, TextKey, format_text};
use mir_db::quest_script::QuestScript;
use mir_db::{MapInfo, MirDb, MonsterInfo, NpcInfo, QuestInfo};
use mir_protocol::ServerPacket;
use mir_protocol::data::{ItemInfo, Stats, UserItem};
use mir_server::combat;
use mir_server::gamedata::GameData;
use mir_server::items;
use mir_server::npc::NpcState;
use mir_server::quests::{QuestError, QuestProgressState, QuestSave};
use mir_server::world::{Player, World};
use tokio::sync::mpsc;

const MAP: &str = "0";

// ---------------------------------------------------------------------------
// 测试脚手架
// ---------------------------------------------------------------------------

/// 每个用例一个独立的 `Envir/` 目录（`Quests/` + `NPCs/`），退出时删除。
struct Envir {
    root: PathBuf,
}

impl Envir {
    fn new(tag: &str) -> Self {
        let root = std::env::temp_dir().join(format!(
            "mir-next-quests-{tag}-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("NPCs/Test")).expect("建 NPC 目录");
        std::fs::create_dir_all(root.join("Quests/Test")).expect("建任务目录");
        Self { root }
    }

    fn script(&self, name: &str, text: &str) -> &Self {
        let path = self.root.join("Quests").join(format!("{name}.txt"));
        std::fs::write(&path, text).expect("写任务脚本");
        self
    }

    fn npc_script(&self, name: &str, text: &str) -> &Self {
        let path = self.root.join("NPCs").join(format!("{name}.txt"));
        std::fs::write(&path, text).expect("写 NPC 脚本");
        self
    }

    fn drop_dir(&self) -> PathBuf {
        self.root.join("Drops")
    }
}

impl Drop for Envir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

fn item(index: i32, name: &str, item_type: ItemType, stack_size: u16, price: u32) -> ItemInfo {
    ItemInfo {
        index,
        name: name.to_owned(),
        item_type,
        grade: ItemGrade::None,
        weight: 1,
        image: u16::try_from(index).unwrap_or(0),
        durability: 0,
        stack_size,
        price,
        ..ItemInfo::default()
    }
}

fn monster(index: i32, name: &str, level: u16, hp: i32, experience: u32) -> MonsterInfo {
    MonsterInfo {
        index,
        name: name.to_owned(),
        image: 1,
        level,
        experience,
        stats: Stats::from_iter([(Stat::HP, hp)]),
        ..MonsterInfo::default()
    }
}

/// 一个任务头部；等级区间之外的字段用默认值，需要时再改。
fn quest(
    index: i32,
    name: &str,
    file_name: &str,
    (min_level, max_level): (i32, i32),
    (required_quest, required_class, time_limit): (i32, u8, i32),
) -> QuestInfo {
    QuestInfo {
        index,
        name: name.to_owned(),
        group: "测试".to_owned(),
        file_name: file_name.to_owned(),
        required_min_level: min_level,
        required_max_level: max_level,
        required_quest,
        required_class,
        quest_type: 0,
        goto_message: "去交任务".to_owned(),
        kill_message: String::new(),
        item_message: String::new(),
        flag_message: String::new(),
        time_limit_in_seconds: time_limit,
    }
}

/// 测试库：5 件物品、2 只怪、1 张图、3 个 NPC、9 个任务。
///
/// | 任务 | 脚本 | 用途 |
/// |---|---|---|
/// | 1 | `Test/1` | 击杀 3 只 Hen + 固定奖励 Meat + 100 金币 / 200 经验 |
/// | 2 | `Test/2` | 收集 Egg ×2（由 NPC 3 的脚本 `[Quests]` 关联） |
/// | 3 | `Test/3` | 前置任务 1 |
/// | 4 | `Test/4` | 限时 10 秒 |
/// | 5 | `Test/5` | 任选奖励 Sword / Bow ×2 |
/// | 6 | `Test/6` | 等级上限 5（玩家 10 级接不了） |
/// | 7 | `Test/7` | 只允许法师 |
/// | 8 | `Test/8` | 真实战斗链路（打死一只 Hen） |
/// | 9 | `Test/9` | 携带物品 Oil ×2 + 标记位任务 |
fn database() -> MirDb {
    MirDb {
        items: vec![
            item(10, "Meat", ItemType::Meat, 1, 10),
            item(11, "Egg", ItemType::Meat, 20, 5),
            item(12, "Sword", ItemType::Weapon, 1, 100),
            item(13, "Bow", ItemType::Weapon, 20, 120),
            item(14, "Oil", ItemType::Meat, 5, 8),
        ],
        monsters: vec![monster(1, "Hen", 1, 10, 5), monster(2, "Deer", 12, 25, 40)],
        maps: vec![MapInfo {
            index: 1,
            file_name: MAP.to_owned(),
            title: "比奇省".to_owned(),
            ..MapInfo::default()
        }],
        npcs: vec![
            NpcInfo {
                index: 1,
                map_index: 1,
                file_name: "Test/Giver".to_owned(),
                name: "任务使者".to_owned(),
                location: Point::new(100, 100),
                image: 5,
                rate: 100,
                // 任务书要求的两条权威来源之一：数据库里的任务列表。
                collect_quest_indexes: vec![1, 3, 4, 5, 6, 7, 8, 9],
                finish_quest_indexes: vec![1, 3, 4, 5, 6, 7, 8, 9],
                ..NpcInfo::default()
            },
            NpcInfo {
                index: 2,
                map_index: 1,
                file_name: "Test/Bystander".to_owned(),
                name: "旁观者".to_owned(),
                location: Point::new(110, 100),
                image: 6,
                rate: 100,
                ..NpcInfo::default()
            },
            NpcInfo {
                index: 3,
                map_index: 1,
                file_name: "Test/CraftLady".to_owned(),
                name: "工匠".to_owned(),
                location: Point::new(105, 100),
                image: 7,
                rate: 100,
                // 数据库列表为空，只靠脚本 `[Quests]` 关联任务 2。
                ..NpcInfo::default()
            },
        ],
        quests: vec![
            quest(1, "杀鸡", "Test/1", (1, 0), (0, 31, 0)),
            quest(2, "交蛋", "Test/2", (1, 0), (0, 31, 0)),
            quest(3, "后续", "Test/3", (1, 0), (1, 31, 0)),
            quest(4, "限时", "Test/4", (1, 0), (0, 31, 10)),
            quest(5, "任选奖励", "Test/5", (1, 0), (0, 31, 0)),
            quest(6, "低级任务", "Test/6", (1, 5), (0, 31, 0)),
            quest(7, "法师任务", "Test/7", (1, 0), (0, 2, 0)),
            quest(8, "实战", "Test/8", (1, 0), (0, 31, 0)),
            quest(9, "携带物品", "Test/9", (1, 0), (0, 31, 0)),
        ],
        ..MirDb::default()
    }
}

/// 任务脚本内容（键是 `QuestInfo.file_name`）。
fn quest_scripts() -> Vec<(&'static str, &'static str)> {
    vec![
        (
            "Test/1",
            "[@Description]\n\
             Kill three hens.\n\
             [@TaskDescription]\n\
             Eliminate 3 hens.\n\
             [@Completion]\n\
             Well done.\n\
             [@KillTasks]\n\
             Hen 3\n\
             [@FixedRewards]\n\
             Meat\n\
             [@ExpReward]\n\
             200\n\
             [@GoldReward]\n\
             100\n",
        ),
        (
            "Test/2",
            "[@Description]\nBring two eggs.\n[@ItemTasks]\nEgg 2\n[@GoldReward]\n50\n",
        ),
        (
            "Test/3",
            "[@Description]\nOne more hen.\n[@KillTasks]\nHen 1\n",
        ),
        (
            "Test/4",
            "[@Description]\nTimed quest.\n[@KillTasks]\nHen 1\n[@GoldReward]\n20\n",
        ),
        (
            "Test/5",
            "[@Description]\nPick one.\n[@KillTasks]\nHen 1\n[@SelectRewards]\nSword\nBow 2\n",
        ),
        (
            "Test/6",
            "[@Description]\nToo high level.\n[@KillTasks]\nHen 1\n",
        ),
        (
            "Test/7",
            "[@Description]\nWizards only.\n[@KillTasks]\nHen 1\n",
        ),
        (
            "Test/8",
            "[@Description]\nFight.\n[@KillTasks]\nHen 1\n[@GoldReward]\n5\n",
        ),
        (
            "Test/9",
            "[@Description]\nCarry the oil and light the lamp.\n\
             [@CarryItems]\nOil 2\n\
             [@FlagTasks]\n7 \"Light the lamp\"\n",
        ),
    ]
}

/// 任务使者的脚本：数据库列表负责发任务，脚本再提供 `CHECKQUEST` 页与任务动作页。
const GIVER_SCRIPT: &str = "\
[@Main]\n\
#SAY\n\
任务使者在此。\n\
<状态/@Check> <数字/@CheckNumeric> <上交/@TurnIn> <接取/@Accept>\n\
<开灯/@Light> <放弃/@GiveUp>\n\
\n\
[@Check]\n\
#IF\n\
CHECKQUEST 1 ACTIVE\n\
#SAY\n\
任务进行中\n\
#ELSESAY\n\
没有接过任务\n\
\n\
[@CheckNumeric]\n\
#IF\n\
CHECKQUEST 1 1\n\
#SAY\n\
数字写法按 COMPLETE 判定\n\
#ELSESAY\n\
数字写法没通过\n\
\n\
[@TurnIn]\n\
#ACT\n\
FINISHQUEST 1\n\
\n\
[@Accept]\n\
#ACT\n\
ACCEPTQUEST 1\n\
\n\
[@Light]\n\
#ACT\n\
SET [7] 1\n\
\n\
[@GiveUp]\n\
#ACT\n\
ABANDONQUEST 4\n\
";

/// 旁观者：不关联任何任务。
const BYSTANDER_SCRIPT: &str = "[@Main]\n#SAY\n我什么也不知道。\n";

/// 工匠：只靠脚本 `[Quests]` 关联任务 2（正数=发布、负数=交付）。
const CRAFT_LADY_SCRIPT: &str = "\
[@Main]\n\
#SAY\n\
把蛋给我。\n\
\n\
[Quests]\n\
2\n\
-2\n\
";

struct Harness {
    world: World,
    receivers: HashMap<u32, mpsc::UnboundedReceiver<Vec<u8>>>,
}

impl Harness {
    fn new(tag: &str) -> (Self, Envir) {
        let envir = Envir::new(tag);
        for (name, text) in quest_scripts() {
            envir.script(name, text);
        }
        envir.npc_script("Test/Giver", GIVER_SCRIPT);
        envir.npc_script("Test/Bystander", BYSTANDER_SCRIPT);
        envir.npc_script("Test/CraftLady", CRAFT_LADY_SCRIPT);
        let data = GameData::from_db(database(), Some(envir.drop_dir()));
        let mut world = World::new(999);
        world.seed_rng(0x5EED);
        world.set_game_data(Arc::new(data));
        let spawned = world.spawn_npcs(MAP);
        assert_eq!(spawned.len(), 3, "三个 NPC 都应该刷出来");
        (
            Self {
                world,
                receivers: HashMap::new(),
            },
            envir,
        )
    }

    fn add_player(&mut self, object_id: u32, position: Point) -> u32 {
        self.add_player_with_level(object_id, position, 10)
    }

    fn add_player_with_level(&mut self, object_id: u32, position: Point, level: u16) -> u32 {
        let stats = combat::player_base_stats(level, MirClass::Warrior);
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
            class: MirClass::Warrior,
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
            attack_mode: mir_core::enums::AttackMode::Peace,
            pk_points: 0,
            brown_time: 0,
            pk_point_time: 0,
            last_hitter: None,
            last_hit_time: 0,
            name_colour: mir_core::Color(-1),
            visible_monsters: BTreeSet::new(),
            visible_npcs: BTreeSet::new(),
            movement_time: 0,
            inventory: items::new_inventory(),
            equipment: items::new_equipment(),
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
            npc: NpcState::default(),
            group_id: None,
            group_invitation: None,
            group_invite_time: 0,
            allow_group: true,
            guild: None,
            trade: mir_server::TradeState::default(),
            trade_invitation: None,
            next_trade_time: 0,
            allow_trade: true,
            sender,
        });
        self.receivers.insert(object_id, receiver);
        object_id
    }

    fn npc_id(&self, name: &str) -> u32 {
        self.world
            .npcs()
            .iter()
            .find(|npc| npc.name == name)
            .map_or_else(|| panic!("找不到 NPC {name}"), |npc| npc.object_id)
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

    fn give(&mut self, object_id: u32, info: &ItemInfo, count: u16) -> UserItem {
        let item = UserItem {
            count,
            ..self.world.create_fresh_item(info)
        };
        assert!(
            self.world.add_item_to_inventory(object_id, item.clone()),
            "背包应该有空位"
        );
        item
    }

    fn count_in_inventory(&self, object_id: u32, item_index: i32) -> u16 {
        self.world
            .player(object_id)
            .expect("玩家存在")
            .inventory
            .iter()
            .flatten()
            .filter(|item| item.item_index == item_index)
            .map(|item| item.count)
            .sum()
    }
}

/// 把一页对话的正文拼成一行，方便断言。
fn page_text(packets: &[ServerPacket]) -> Vec<String> {
    packets
        .iter()
        .filter_map(|packet| match packet {
            ServerPacket::NPCResponse(response) => Some(response.page.join("\n")),
            _ => None,
        })
        .collect()
}

// ---------------------------------------------------------------------------
// 1. 脚本解析
// ---------------------------------------------------------------------------

#[test]
fn real_quest_script_parses_into_tasks_and_rewards() {
    let Ok(data) = GameData::discover() else {
        eprintln!("跳过：resource/ 里没有 Server.MirDB 样本");
        return;
    };
    let Some(envir) = data.envir_dir() else {
        eprintln!("跳过：样本没有 Envir/ 目录");
        return;
    };
    // `BichonProvince/BichonWall/1.txt`：5 只 ForestYeti + PrecisionPendant + 200 经验 / 50 金币。
    let script = QuestScript::load("BichonProvince/BichonWall/1", envir).expect("真实脚本可加载");
    assert_eq!(script.file_name(), "BichonProvince/BichonWall/1");
    assert_eq!(script.kill_tasks.len(), 1);
    assert_eq!(script.kill_tasks[0].monster, "ForestYeti");
    assert_eq!(script.kill_tasks[0].count, 5);
    assert_eq!(script.fixed_rewards.len(), 1);
    assert_eq!(script.fixed_rewards[0].item, "PrecisionPendant");
    assert_eq!(script.exp_reward, 200);
    assert_eq!(script.gold_reward, 50);
    assert!(
        script
            .description
            .iter()
            .any(|line| line.contains("ForestYeti"))
    );
    assert!(script.warnings().is_empty(), "{:?}", script.warnings());
}

#[test]
fn synthetic_script_parses_every_supported_section() {
    let (harness, _envir) = Harness::new("script-sections");
    let script = harness
        .world
        .quest_definition(9)
        .expect("任务 9 存在")
        .script;
    assert_eq!(script.carry_items.len(), 1);
    assert_eq!(script.carry_items[0].item, "Oil");
    assert_eq!(script.carry_items[0].count, 2);
    assert_eq!(script.flag_tasks.len(), 1);
    assert_eq!(script.flag_tasks[0].number, 7);
    assert_eq!(script.flag_tasks[0].message, "Light the lamp");
    // 没有目标的脚本照样可用（Crystal `CheckCompleted` 对空目标直接通过）。
    let empty = QuestScript::parse(
        "Test/empty",
        &["[@Description]".to_owned(), "hi".to_owned()],
    );
    assert!(!empty.has_tasks());
}

// ---------------------------------------------------------------------------
// 2. 接取
// ---------------------------------------------------------------------------

#[test]
fn accept_quest_records_progress_and_sends_change_quest() {
    let (mut harness, _envir) = Harness::new("accept");
    let giver = harness.npc_id("任务使者");
    let player = harness.add_player(1, Point::new(100, 101));
    harness.packets(player);

    let report = harness
        .world
        .accept_quest(player, giver, 1)
        .expect("接取成功");
    assert_eq!(report.quest_index, 1);
    assert_eq!(report.name, "杀鸡");
    assert!(!report.completed);
    assert_eq!(
        report.task_list,
        vec![format_text(
            Language::ZhCn,
            TextKey::QuestTaskKill,
            &[("monster", "Hen"), ("count", "0"), ("total", "3")],
        )]
    );

    let progress = harness.world.quest_progress(player, 1).expect("进度存在");
    assert_eq!(progress.state, QuestProgressState::InProgress);
    assert_eq!(progress.kill_count.len(), 1);
    assert_eq!(progress.kill_count[0], ("hen".to_owned(), 0));
    assert!(progress.taken_at > 0);

    // `SendUpdateQuest(quest, QuestState.Add, true)`（`PlayerObject.cs:11345`）。
    let packets = harness.packets(player);
    let change = packets
        .iter()
        .find_map(|packet| match packet {
            ServerPacket::ChangeQuest(change) => Some(change),
            _ => None,
        })
        .expect("应有 S.ChangeQuest");
    assert_eq!(change.quest_state, QuestState::Add);
    assert!(change.track_quest, "接取时 trackQuest = true");
    assert_eq!(change.quest.id, 1);
    assert!(change.quest.taken);
    assert!(!change.quest.completed);
    assert!(change.quest.new, "刚接的任务 New = true");
}

#[test]
fn accept_rejects_level_class_and_prerequisite() {
    let (mut harness, _envir) = Harness::new("accept-gates");
    let giver = harness.npc_id("任务使者");
    let player = harness.add_player(1, Point::new(100, 101));

    // 等级上限 5，玩家 10 级（`QuestInfo.CanAccept`，`QuestInfo.cs:352-353`）。
    match harness.world.accept_quest(player, giver, 6) {
        Err(QuestError::LevelOutOfRange { level, min, max }) => {
            assert_eq!((level, min, max), (10, 1, 5));
        }
        other => panic!("期望 LevelOutOfRange，收到 {other:?}"),
    }
    // 只允许法师（`:358-380` 的职业位掩码）。
    match harness.world.accept_quest(player, giver, 7) {
        Err(QuestError::WrongClass { class }) => assert_eq!(class, "Warrior"),
        other => panic!("期望 WrongClass，收到 {other:?}"),
    }
    // 前置任务 1 没完成（`:11288-11305`）。
    assert_eq!(
        harness.world.accept_quest(player, giver, 3),
        Err(QuestError::PrerequisiteMissing(1))
    );

    // 完成前置之后再接就成功。
    harness.world.accept_quest(player, giver, 1).expect("接 1");
    for _ in 0..3 {
        harness.world.advance_quest_kills(player, "Hen");
    }
    harness.world.finish_quest(player, 1, -1).expect("交 1");
    harness
        .world
        .accept_quest(player, giver, 3)
        .expect("前置完成后可以接后续任务");
}

#[test]
fn accept_rejects_the_wrong_npc() {
    let (mut harness, _envir) = Harness::new("accept-wrong-npc");
    let bystander = harness.npc_id("旁观者");
    let craft_lady = harness.npc_id("工匠");
    let player = harness.add_player(1, Point::new(100, 101));

    // 旁观者不关联任何任务。
    assert_eq!(
        harness.world.accept_quest(player, bystander, 1),
        Err(QuestError::NotAQuestPublisher {
            npc: bystander,
            quest: 1
        })
    );
    // 工匠的 `[Quests]` 里只有任务 2，没有任务 1。
    assert_eq!(
        harness.world.accept_quest(player, craft_lady, 1),
        Err(QuestError::NotAQuestPublisher {
            npc: craft_lady,
            quest: 1
        })
    );
    // 离得太远也不行（`Functions.InRange(..., Globals.DataRange)`，`:11265`）。
    let far = harness.add_player(2, Point::new(500, 500));
    assert_eq!(
        harness
            .world
            .accept_quest(far, harness.npc_id("任务使者"), 1),
        Err(QuestError::OutOfRange {
            object_id: harness.npc_id("任务使者"),
            range: mir_server::npc::NPC_TALK_RANGE
        })
    );
}

#[test]
fn accept_rejects_duplicates_and_completed_quests() {
    let (mut harness, _envir) = Harness::new("accept-dup");
    let giver = harness.npc_id("任务使者");
    let player = harness.add_player(1, Point::new(100, 101));
    harness
        .world
        .accept_quest(player, giver, 1)
        .expect("第一次接");
    assert_eq!(
        harness.world.accept_quest(player, giver, 1),
        Err(QuestError::AlreadyTaken(1))
    );
    // 交付后再接同一个（非可重复）任务也不行（`:11282-11286`）。
    for _ in 0..3 {
        harness.world.advance_quest_kills(player, "Hen");
    }
    harness.world.finish_quest(player, 1, -1).expect("交付");
    assert_eq!(
        harness.world.accept_quest(player, giver, 1),
        Err(QuestError::AlreadyCompleted(1))
    );
}

#[test]
fn npc_script_quest_list_links_the_craft_lady() {
    let (mut harness, _envir) = Harness::new("script-quests");
    let craft_lady = harness.npc_id("工匠");
    let npc = harness.world.npc(craft_lady).expect("工匠存在").clone();
    // 数据库列表为空，靠 `NPCScript.ParseQuests`（`NPCScript.cs:685-721`）关联。
    assert!(npc.collect_quest_indexes.is_empty());
    assert!(npc.finish_quest_indexes.is_empty());
    assert!(harness.world.npc_gives_quest(&npc, 2));
    assert!(harness.world.npc_finishes_quest(&npc, 2));
    assert!(!harness.world.npc_gives_quest(&npc, 1));

    let player = harness.add_player(1, Point::new(100, 101));
    harness
        .world
        .accept_quest(player, craft_lady, 2)
        .expect("脚本关联的任务可以接");
}

// ---------------------------------------------------------------------------
// 3. 进度推进
// ---------------------------------------------------------------------------

#[test]
fn kill_progress_completes_the_quest() {
    let (mut harness, _envir) = Harness::new("kill");
    let giver = harness.npc_id("任务使者");
    let player = harness.add_player(1, Point::new(100, 101));
    harness.world.accept_quest(player, giver, 1).expect("接取");
    harness.packets(player);

    // 大小写与空格差异都要命中（Crystal `NeedKill` 用 `OrdinalIgnoreCase` + `StartsWith`）。
    assert_eq!(harness.world.advance_quest_kills(player, " hen "), 1);
    assert_eq!(harness.world.advance_quest_kills(player, "HEN"), 1);
    let progress = harness.world.quest_progress(player, 1).expect("进度");
    assert_eq!(progress.kill_count[0].1, 2);
    assert!(!progress.is_completed());

    assert_eq!(harness.world.advance_quest_kills(player, "Hen"), 1);
    let progress = harness.world.quest_progress(player, 1).expect("进度");
    assert_eq!(progress.kill_count[0].1, 3);
    assert!(
        !progress.is_completed(),
        "计数到点只表示「目标达成」，还要去交任务"
    );
    assert!(progress.objectives_met, "目标已达成（Crystal `Completed`）");

    // 达成后进度文本带「已完成」并追加「去交任务」（Crystal `UpdateKillTasks` +
    // `UpdateGotoTask`，`QuestProgressInfo.cs:438-466`、`:527-532`）。
    let definition = harness.world.quest_definition(1).expect("定义");
    let client = harness.world.client_progress(
        &definition,
        progress,
        harness.world.wall_now(),
        Language::ZhCn,
    );
    assert_eq!(
        client.task_list,
        vec![
            format_text(
                Language::ZhCn,
                TextKey::QuestTaskKill,
                &[("monster", "Hen"), ("count", "3"), ("total", "3")],
            ),
            // 脚本自带数据（`Server.MirDB` 的 `GotoMessage`），不是代码文案。
            definition.info.goto_message.clone(),
        ]
    );
    assert!(
        client.completed,
        "`ClientQuestProgress.Completed` 表示「目标达成」，不是「已交付」"
    );

    // 每次推进都发一条 `S.ChangeQuest{Update}` 与一条 `S.SendOutputMessage`。
    let packets = harness.packets(player);
    assert_eq!(
        packets
            .iter()
            .filter(|packet| matches!(packet, ServerPacket::ChangeQuest(_)))
            .count(),
        3
    );
    assert!(packets.iter().any(|packet| matches!(
        packet,
        ServerPacket::SendOutputMessage(message) if message.message.contains("Hen")
    )));
}

#[test]
fn monster_death_advances_the_killer_quest() {
    let (mut harness, _envir) = Harness::new("kill-real");
    let giver = harness.npc_id("任务使者");
    let player = harness.add_player(1, Point::new(100, 101));
    harness.world.accept_quest(player, giver, 8).expect("接取");
    // 接完任务再走到空地上（接取要求离 NPC 不超过 16 格）。
    assert!(harness.world.place_player(player, MAP, Point::new(20, 20)));
    harness.packets(player);

    // 一只 1 点血、名字叫 Hen 的怪，站在玩家正前方。
    let kind = mir_server::monster::MonsterKind {
        name: "Hen".to_owned(),
        stats: combat::CombatStats {
            hp: 1,
            accuracy: 100,
            ..combat::CombatStats::default()
        },
        ..mir_server::monster::MonsterKind::new("Hen", mir_core::enums::Monster::Hen)
    };
    let monster = harness.world.spawn_monster(MAP, kind, Point::new(21, 20));
    harness
        .world
        .attack(player, MirDirection::Right)
        .expect("发起攻击")
        .expect("前方有目标");
    // 伤害 300ms 后落地（`HumanObject.cs:3201`）。
    harness.world.advance_clock(400);
    harness.world.tick();

    assert!(harness.world.monster(monster).expect("怪还在").dead);
    let progress = harness.world.quest_progress(player, 8).expect("进度");
    assert_eq!(progress.kill_count_of("Hen"), 1, "怪物死亡推进击杀任务");
}

#[test]
fn item_progress_counts_the_inventory() {
    let (mut harness, _envir) = Harness::new("items");
    let craft_lady = harness.npc_id("工匠");
    let player = harness.add_player(1, Point::new(105, 101));
    harness
        .world
        .accept_quest(player, craft_lady, 2)
        .expect("接取");
    assert_eq!(
        harness
            .world
            .quest_progress(player, 2)
            .unwrap()
            .item_count_of("Egg"),
        0
    );

    let egg = harness.world.item_info_by_name("Egg").unwrap().clone();
    harness.give(player, &egg, 1);
    harness.world.refresh_quest_items(player);
    assert_eq!(
        harness
            .world
            .quest_progress(player, 2)
            .unwrap()
            .item_count_of("Egg"),
        1,
        "背包里 1 个蛋"
    );
    harness.give(player, &egg, 1);
    harness.world.refresh_quest_items(player);
    let progress = harness
        .world
        .quest_progress(player, 2)
        .expect("进度")
        .clone();
    assert_eq!(progress.item_count_of("Egg"), 2);
    let definition = harness.world.quest_definition(2).expect("定义");
    assert!(mir_server::quests::is_objectives_complete(
        &definition.script,
        &progress
    ));
}

// ---------------------------------------------------------------------------
// 4. 交付与奖励
// ---------------------------------------------------------------------------

#[test]
fn finish_quest_pays_gold_exp_and_fixed_items() {
    let (mut harness, _envir) = Harness::new("finish");
    let giver = harness.npc_id("任务使者");
    let player = harness.add_player(1, Point::new(100, 101));
    harness.world.accept_quest(player, giver, 1).expect("接取");
    for _ in 0..3 {
        harness.world.advance_quest_kills(player, "Hen");
    }
    harness.packets(player);

    let report = harness.world.finish_quest(player, 1, -1).expect("交付");
    assert!(report.completed);
    assert_eq!(report.gold, 100);
    assert_eq!(report.exp, 200);
    assert_eq!(report.credit, 0);
    assert_eq!(report.items, vec![("Meat".to_owned(), 1)]);

    let player_state = harness.world.player(player).expect("玩家");
    assert_eq!(player_state.gold, 100);
    assert_eq!(player_state.experience, 200);
    assert!(player_state.completed_quests.contains(&1));
    assert_eq!(
        harness.world.quest_progress(player, 1).unwrap().state,
        QuestProgressState::Completed
    );
    assert_eq!(
        harness.count_in_inventory(player, 10),
        1,
        "固定奖励 Meat ×1"
    );

    // 包顺序：`S.CompleteQuest` → `S.ChangeQuest{Remove}` → `S.GainedItem` → `S.GainedGold` → 经验。
    let packets = harness.packets(player);
    let order: Vec<&str> = packets
        .iter()
        .filter_map(|packet| match packet {
            ServerPacket::CompleteQuest(_) => Some("complete"),
            ServerPacket::ChangeQuest(change) if change.quest_state == QuestState::Remove => {
                Some("remove")
            }
            ServerPacket::GainedItem(_) => Some("item"),
            ServerPacket::GainedGold(_) => Some("gold"),
            ServerPacket::GainExperience(_) => Some("exp"),
            _ => None,
        })
        .collect();
    assert_eq!(
        order,
        vec!["complete", "remove", "item", "gold", "exp"],
        "实际包序列 {order:?}"
    );
}

#[test]
fn finish_uses_the_selected_optional_reward() {
    let (mut harness, _envir) = Harness::new("select");
    let giver = harness.npc_id("任务使者");
    let first = harness.add_player(1, Point::new(100, 101));
    let second = harness.add_player(2, Point::new(101, 101));

    for player in [first, second] {
        harness.world.accept_quest(player, giver, 5).expect("接取");
        harness.world.advance_quest_kills(player, "Hen");
    }
    // 选 0 号（Sword ×1）/ 选 1 号（Bow ×2）；`-1` 表示不选。
    let report = harness.world.finish_quest(first, 5, 0).expect("交付 0");
    assert_eq!(report.items, vec![("Sword".to_owned(), 1)]);
    assert_eq!(harness.count_in_inventory(first, 12), 1);
    assert_eq!(harness.count_in_inventory(first, 13), 0);

    let report = harness.world.finish_quest(second, 5, 1).expect("交付 1");
    assert_eq!(report.items, vec![("Bow".to_owned(), 2)]);
    assert_eq!(harness.count_in_inventory(second, 13), 2);
    assert_eq!(harness.count_in_inventory(second, 12), 0);

    // 越界的下标不选（Crystal `:11394-11420` 只在 `0 <= index < Count` 时发）。
    let third = harness.add_player(3, Point::new(102, 101));
    harness.world.accept_quest(third, giver, 5).expect("接取");
    harness.world.advance_quest_kills(third, "Hen");
    let report = harness.world.finish_quest(third, 5, 99).expect("越界下标");
    assert!(report.items.is_empty());
}

#[test]
fn finish_requires_the_finisher_npc_nearby() {
    let (mut harness, _envir) = Harness::new("finish-npc");
    let giver = harness.npc_id("任务使者");
    let player = harness.add_player(1, Point::new(100, 101));
    harness.world.accept_quest(player, giver, 1).expect("接取");
    for _ in 0..3 {
        harness.world.advance_quest_kills(player, "Hen");
    }
    // 走开（Crystal 在 `:11358-11366` 里要求交付 NPC 在同图 16 格内）。
    assert!(
        harness
            .world
            .place_player(player, MAP, Point::new(400, 400))
    );
    assert_eq!(
        harness.world.finish_quest(player, 1, -1),
        Err(QuestError::NoFinisherNearby(1))
    );
}

#[test]
fn finish_rejects_unfinished_quests() {
    let (mut harness, _envir) = Harness::new("finish-unfinished");
    let giver = harness.npc_id("任务使者");
    let player = harness.add_player(1, Point::new(100, 101));
    harness.world.accept_quest(player, giver, 1).expect("接取");
    assert_eq!(
        harness.world.finish_quest(player, 1, -1),
        Err(QuestError::NotCompleted(1)),
        "目标没达成不能交"
    );
    // 没接过的任务：交付/放弃都报 NotTaken。
    assert_eq!(
        harness.world.finish_quest(player, 2, -1),
        Err(QuestError::NotTaken(2))
    );
    assert_eq!(
        harness.world.abandon_quest(player, 2),
        Err(QuestError::NotTaken(2))
    );
}

// ---------------------------------------------------------------------------
// 5. 超时与放弃
// ---------------------------------------------------------------------------

#[test]
fn timed_quest_expires_and_is_abandoned() {
    let (mut harness, _envir) = Harness::new("timeout");
    let giver = harness.npc_id("任务使者");
    let player = harness.add_player(1, Point::new(100, 101));
    let report = harness
        .world
        .accept_quest(player, giver, 4)
        .expect("接取限时任务");
    assert_eq!(
        report.task_list,
        vec![format_text(
            Language::ZhCn,
            TextKey::QuestTaskKill,
            &[("monster", "Hen"), ("count", "0"), ("total", "1")],
        )]
    );
    let expires_at = harness.world.quest_progress(player, 4).unwrap().expires_at;
    assert_eq!(
        expires_at - harness.world.quest_progress(player, 4).unwrap().taken_at,
        10_000,
        "`time_limit_in_seconds = 10` → 10 秒"
    );
    harness.packets(player);

    harness.world.advance_clock(9_000);
    harness.world.tick();
    assert!(
        harness.world.quest_progress(player, 4).is_some(),
        "还没到点"
    );

    harness.world.advance_clock(2_000);
    harness.world.tick();
    assert!(
        harness.world.quest_progress(player, 4).is_none(),
        "超时后按放弃处理（`PlayerObject.cs:4364-4385`）"
    );
    let packets = harness.packets(player);
    assert!(packets.iter().any(|packet| matches!(
        packet,
        ServerPacket::ChangeQuest(change) if change.quest_state == QuestState::Remove
    )));
}

#[test]
fn abandon_quest_clears_the_progress() {
    let (mut harness, _envir) = Harness::new("abandon");
    let giver = harness.npc_id("任务使者");
    let player = harness.add_player(1, Point::new(100, 101));
    harness.world.accept_quest(player, giver, 1).expect("接取");
    harness.world.advance_quest_kills(player, "Hen");
    harness.packets(player);

    harness.world.abandon_quest(player, 1).expect("放弃");
    assert!(harness.world.quest_progress(player, 1).is_none());
    assert!(
        !harness
            .world
            .player(player)
            .unwrap()
            .completed_quests
            .contains(&1)
    );
    let packets = harness.packets(player);
    assert!(packets.iter().any(|packet| matches!(
        packet,
        ServerPacket::ChangeQuest(change) if change.quest_state == QuestState::Remove
    )));
}

#[test]
fn carry_items_are_given_on_accept_and_taken_on_finish() {
    let (mut harness, _envir) = Harness::new("carry");
    let giver = harness.npc_id("任务使者");
    let player = harness.add_player(1, Point::new(100, 101));
    harness.world.accept_quest(player, giver, 9).expect("接取");
    assert_eq!(
        harness.count_in_inventory(player, 14),
        2,
        "Crystal 会把 `[@CARRYITEMS]` 发给玩家（`PlayerObject.cs:11307-11339`）"
    );

    // 标记位任务：点亮标记 7（NPC 脚本 `SET [7] 1`）后目标达成。
    harness
        .world
        .player_mut(player)
        .unwrap()
        .npc
        .flags
        .insert(7);
    if let Some(progress) = harness.world.player_mut(player).unwrap().quests.first_mut() {
        progress.flags.insert(7);
    }
    let definition = harness.world.quest_definition(9).expect("定义");
    let progress = harness
        .world
        .quest_progress(player, 9)
        .expect("进度")
        .clone();
    assert!(mir_server::quests::is_objectives_complete(
        &definition.script,
        &progress
    ));

    harness.world.finish_quest(player, 9, -1).expect("交付");
    assert_eq!(
        harness.count_in_inventory(player, 14),
        0,
        "交付时收回携带物品"
    );
}

// ---------------------------------------------------------------------------
// 6. NPC 脚本集成
// ---------------------------------------------------------------------------

#[test]
fn npc_checkquest_reads_the_real_state() {
    let (mut harness, _envir) = Harness::new("checkquest");
    let giver = harness.npc_id("任务使者");
    let player = harness.add_player(1, Point::new(100, 101));
    harness
        .world
        .call_npc(player, giver, "@main")
        .expect("打开对话");
    harness.packets(player);

    // 没接任务 → `CHECKQUEST 1 ACTIVE` 不通过 → ELSESAY。
    let reply = harness
        .world
        .call_npc(player, giver, "@Check")
        .expect("点状态按钮");
    assert_eq!(reply.page, vec!["没有接过任务".to_owned()]);

    // 接了任务之后 → SAY 分支。
    harness.world.accept_quest(player, giver, 1).expect("接取");
    harness
        .world
        .call_npc(player, giver, "@main")
        .expect("回主页");
    let reply = harness
        .world
        .call_npc(player, giver, "@Check")
        .expect("点状态按钮");
    assert_eq!(reply.page, vec!["任务进行中".to_owned()]);
    harness.packets(player);

    // Crystal 的怪癖：第二个参数只要不是 `ACTIVE` 就按 `COMPLETE` 判定
    // （`NPCSegment.cs:2525-2534`），所以 `CHECKQUEST 1 1` 在「进行中」时不通过。
    let reply = harness
        .world
        .call_npc(player, giver, "@main")
        .expect("回主页");
    assert!(reply.page[0].contains("任务使者"));
    harness.packets(player);
    // `[@CheckNumeric]` 在主页里没有按钮白名单，先经过 `@Main` 再点。
    let reply = harness
        .world
        .call_npc(player, giver, "@CheckNumeric")
        .expect("点数字检查按钮");
    assert_eq!(reply.page, vec!["数字写法没通过".to_owned()]);

    // 交完任务之后 `CHECKQUEST 1 COMPLETE` 通过（同一个 `1` 写法）。
    for _ in 0..3 {
        harness.world.advance_quest_kills(player, "Hen");
    }
    harness.world.finish_quest(player, 1, -1).expect("交付");
    harness
        .world
        .call_npc(player, giver, "@main")
        .expect("回主页");
    let reply = harness
        .world
        .call_npc(player, giver, "@CheckNumeric")
        .expect("再点数字检查按钮");
    assert_eq!(reply.page, vec!["数字写法按 COMPLETE 判定".to_owned()]);
}

#[test]
fn npc_finishquest_action_completes_the_quest() {
    let (mut harness, _envir) = Harness::new("npc-actions");
    let giver = harness.npc_id("任务使者");
    let player = harness.add_player(1, Point::new(100, 101));
    harness
        .world
        .call_npc(player, giver, "@main")
        .expect("打开对话");

    // `ACCEPTQUEST 1`（本作扩展的动作）替玩家接了任务。
    harness
        .world
        .call_npc(player, giver, "@Accept")
        .expect("点接取按钮");
    assert!(harness.world.quest_progress(player, 1).is_some());

    // 没达成时 `FINISHQUEST` 不生效。
    harness
        .world
        .call_npc(player, giver, "@main")
        .expect("回主页");
    harness
        .world
        .call_npc(player, giver, "@TurnIn")
        .expect("点上交按钮");
    assert!(
        harness
            .world
            .quest_progress(player, 1)
            .unwrap()
            .is_in_progress()
    );

    // 打完怪之后 `FINISHQUEST 1` 真的交付了。
    for _ in 0..3 {
        harness.world.advance_quest_kills(player, "Hen");
    }
    harness.packets(player);
    harness
        .world
        .call_npc(player, giver, "@main")
        .expect("回主页");
    harness
        .world
        .call_npc(player, giver, "@TurnIn")
        .expect("点上交按钮");
    assert_eq!(harness.world.player(player).unwrap().gold, 100);
    assert!(
        harness
            .world
            .player(player)
            .unwrap()
            .completed_quests
            .contains(&1)
    );

    // `ABANDONQUEST 4`：先接一个限时任务，再让脚本动作把它放弃掉。
    harness
        .world
        .accept_quest(player, giver, 4)
        .expect("接限时任务");
    assert!(harness.world.quest_progress(player, 4).is_some());
    harness
        .world
        .call_npc(player, giver, "@main")
        .expect("回主页");
    harness
        .world
        .call_npc(player, giver, "@GiveUp")
        .expect("点放弃按钮");
    assert!(
        harness.world.quest_progress(player, 4).is_none(),
        "ABANDONQUEST 动作应该清掉进度"
    );
}

// ---------------------------------------------------------------------------
// 7. 持久化
// ---------------------------------------------------------------------------

#[test]
fn quest_progress_round_trips_through_the_byte_block() {
    let (mut harness, _envir) = Harness::new("codec");
    let giver = harness.npc_id("任务使者");
    let player = harness.add_player(1, Point::new(100, 101));
    harness.world.accept_quest(player, giver, 1).expect("接取");
    harness
        .world
        .accept_quest(player, giver, 4)
        .expect("接取限时");
    harness.world.advance_quest_kills(player, "Hen");
    for _ in 0..3 {
        harness.world.advance_quest_kills(player, "Hen");
    }
    harness.world.finish_quest(player, 1, -1).expect("交付 1");
    harness.world.sync_quest_items(player);

    let save = QuestSave {
        quests: harness.world.player(player).unwrap().quests.clone(),
        completed_quests: harness
            .world
            .player(player)
            .unwrap()
            .completed_quests
            .clone(),
    };
    let bytes = mir_server::quests::encode_quests(&save);
    let decoded = mir_server::quests::decode_quests(&bytes).expect("解码成功");
    assert_eq!(decoded, save);
    assert_eq!(decoded.quests.len(), 2);
    assert!(decoded.completed_quests.contains(&1));
    let restored: &mir_server::quests::QuestProgress = decoded
        .quests
        .iter()
        .find(|progress| progress.quest_index == 4)
        .expect("限时任务还在");
    assert_eq!(restored.state, QuestProgressState::InProgress);
    assert!(restored.expires_at > restored.taken_at);
    // 空块（新角色）解出空存档。
    assert_eq!(
        mir_server::quests::decode_quests(&[]).unwrap(),
        QuestSave::default()
    );
}

// ---------------------------------------------------------------------------
// 8. 真实数据冒烟
// ---------------------------------------------------------------------------

#[test]
fn real_database_quests_and_npc_links() {
    let Ok(data) = GameData::discover() else {
        eprintln!("跳过：resource/ 里没有 Server.MirDB 样本");
        return;
    };
    let Some(envir) = data.envir_dir() else {
        eprintln!("跳过：样本没有 Envir/ 目录");
        return;
    };
    // 1) 任务头部数量（任务书要求 > 100）。
    assert!(
        data.db.quests.len() > 100,
        "任务头部应上百，实际 {}",
        data.db.quests.len()
    );
    // 2) 脚本解析成功率 ≥ 90%。
    let mut ok = 0;
    let mut failed = Vec::new();
    for info in &data.db.quests {
        match QuestScript::load(&info.file_name, envir) {
            Ok(script) => {
                ok += 1;
                let _ = script;
            }
            Err(error) => failed.push(format!("{}({}): {error}", info.index, info.file_name)),
        }
    }
    let total = data.db.quests.len();
    println!(
        "真实任务脚本：{ok}/{total} 解析成功，失败样例 {:?}",
        &failed[..failed.len().min(5)]
    );
    assert!(
        ok * 10 >= total * 9,
        "至少 90% 的脚本应能解析，实际 {ok}/{total}"
    );

    // 2b) `Envir/Quests` 目录里的**全部** `.txt`（含没有被任何任务头部引用的 7 份）。
    let mut dirs = vec![envir.join("Quests")];
    let mut files = Vec::new();
    while let Some(dir) = dirs.pop() {
        for entry in std::fs::read_dir(&dir).expect("读任务目录") {
            let path = entry.expect("目录项").path();
            if path.is_dir() {
                dirs.push(path);
            } else if path.extension().is_some_and(|ext| ext == "txt") {
                files.push(path);
            }
        }
    }
    let mut parsed = 0;
    for path in &files {
        if QuestScript::load(path, envir).is_ok() {
            parsed += 1;
        }
    }
    println!(
        "Envir/Quests 目录：{} 份 .txt，解析成功 {parsed}",
        files.len()
    );
    assert!(
        files.len() >= 150,
        "样本里的任务脚本应上百，实际 {}",
        files.len()
    );
    assert_eq!(
        parsed,
        files.len(),
        "目录里的每一份任务脚本都应该能解析（不放过的坏行只记 warnings）"
    );

    // 3) NPC 与任务的关联只能来自 NPC 脚本的 `[Quests]` 段：
    //    这份样本的 `NpcInfo.CollectQuestIndexes` / `FinishQuestIndexes` **全是空的**。
    let with_db_links = data
        .db
        .npcs
        .iter()
        .filter(|npc| !npc.collect_quest_indexes.is_empty() || !npc.finish_quest_indexes.is_empty())
        .count();
    assert_eq!(
        with_db_links, 0,
        "样本数据库里没有 NPC 任务列表（所以脚本 [Quests] 是唯一来源）"
    );
    let linked = data
        .db
        .npcs
        .iter()
        .filter(|npc| {
            data.npc_script(&npc.file_name)
                .is_some_and(|script| !script.quest_indexes().is_empty())
        })
        .count();
    println!(
        "真实 NPC 脚本里带 [Quests] 段的：{linked} / {}",
        data.db.npcs.len()
    );
    assert!(linked > 50, "应有几十个 NPC 脚本关联任务，实际 {linked}");

    // 4) 统计任务动作 / 条件在真实脚本里的使用情况（写进 docs 的结论）。
    let mut check_quest = 0;
    let mut accept_actions = 0;
    for npc in &data.db.npcs {
        let Some(script) = data.npc_script(&npc.file_name) else {
            continue;
        };
        for page in script.sections() {
            for segment in &page.segments {
                for check in &segment.checks {
                    if check.kind == mir_db::npc_script::NpcCheckKind::Quest {
                        check_quest += 1;
                    }
                }
                for action in segment.actions.iter().chain(&segment.else_actions) {
                    match action.kind {
                        mir_db::npc_script::NpcActionKind::AcceptQuest
                        | mir_db::npc_script::NpcActionKind::FinishQuest
                        | mir_db::npc_script::NpcActionKind::AbandonQuest => accept_actions += 1,
                        _ => {}
                    }
                }
            }
        }
    }
    println!(
        "真实脚本：CHECKQUEST {check_quest} 处，ACCEPT/FINISH/ABANDONQUEST {accept_actions} 处"
    );
    assert!(check_quest > 0, "真实脚本里有用 CHECKQUEST");
    assert_eq!(
        accept_actions, 0,
        "Crystal 没有 ACCEPTQUEST/FINISHQUEST/ABANDONQUEST 动作，真实脚本里也不该出现"
    );
}

#[test]
fn real_quest_can_be_completed_end_to_end() {
    let Ok(data) = GameData::discover() else {
        eprintln!("跳过：resource/ 里没有 Server.MirDB 样本");
        return;
    };
    let mut world = World::new(999);
    world.seed_rng(7);
    world.set_game_data(Arc::new(data));
    let spawned = world.spawn_npcs(MAP);
    assert!(!spawned.is_empty(), "比奇省应该刷出 NPC");

    // 5 号任务「The Smith's 1st Test」：铁匠发布 + 交付，杀 10 只 Deer / 10 只 Scarecrow。
    let smith = world
        .npcs()
        .iter()
        .find(|npc| npc.name == "Blacksmith_Smith")
        .cloned()
        .expect("样本里应有铁匠");
    assert!(world.npc_gives_quest(&smith, 5));
    assert!(world.npc_finishes_quest(&smith, 5));

    let (sender, _receiver) = mpsc::unbounded_channel();
    let stats = combat::player_base_stats(20, MirClass::Warrior);
    world.enter(Player {
        object_id: 1,
        session_id: 1,
        character_id: 1,
        name: "测试者".to_owned(),
        map: MAP.to_owned(),
        position: Point::new(smith.position.x, smith.position.y + 1),
        direction: MirDirection::Down,
        level: 20,
        experience: 0,
        max_experience: 0,
        class: MirClass::Warrior,
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
        attack_mode: mir_core::enums::AttackMode::Peace,
        pk_points: 0,
        brown_time: 0,
        pk_point_time: 0,
        last_hitter: None,
        last_hit_time: 0,
        name_colour: mir_core::Color(-1),
        visible_monsters: BTreeSet::new(),
        visible_npcs: BTreeSet::new(),
        movement_time: 0,
        inventory: items::new_inventory(),
        equipment: items::new_equipment(),
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
        npc: NpcState::default(),
        group_id: None,
        group_invitation: None,
        group_invite_time: 0,
        allow_group: true,
        guild: None,
        trade: mir_server::TradeState::default(),
        trade_invitation: None,
        next_trade_time: 0,
        allow_trade: true,
        sender,
    });

    let report = world
        .accept_quest(1, smith.object_id, 5)
        .expect("接取真实任务");
    assert_eq!(report.name, "The Smith's 1st Test");

    for _ in 0..10 {
        world.advance_quest_kills(1, "Deer");
    }
    for _ in 0..10 {
        world.advance_quest_kills(1, "Scarecrow");
    }
    let report = world.finish_quest(1, 5, -1).expect("交付真实任务");
    assert_eq!(report.gold, 30);
    assert_eq!(report.exp, 120);
    assert_eq!(report.items, vec![("WornIronBracelet".to_owned(), 1)]);
    let player = world.player(1).expect("玩家");
    assert!(player.completed_quests.contains(&5));
    assert_eq!(player.gold, 30);
    assert_eq!(player.experience, 120);
}

#[test]
fn real_luke_script_gates_the_dialog_on_quest_completion() {
    let Ok(data) = GameData::discover() else {
        eprintln!("跳过：resource/ 里没有 Server.MirDB 样本");
        return;
    };
    let mut world = World::new(999);
    world.seed_rng(11);
    world.set_game_data(Arc::new(data));
    world.spawn_npcs(MAP);
    let luke = world
        .npcs()
        .iter()
        .find(|npc| npc.name == "Commander_Luke")
        .cloned()
        .expect("样本里应有 Luke");
    // Luke 的脚本 `[Quests]` 里是 `154`：它发布任务 154（`NPCScript.cs:711-714`）。
    assert!(world.npc_gives_quest(&luke, 154));

    let (sender, _receiver) = mpsc::unbounded_channel();
    let stats = combat::player_base_stats(20, MirClass::Warrior);
    world.enter(Player {
        object_id: 2,
        session_id: 2,
        character_id: 2,
        name: "路人".to_owned(),
        map: MAP.to_owned(),
        position: Point::new(luke.position.x, luke.position.y + 1),
        direction: MirDirection::Down,
        level: 20,
        experience: 0,
        max_experience: 0,
        class: MirClass::Warrior,
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
        attack_mode: mir_core::enums::AttackMode::Peace,
        pk_points: 0,
        brown_time: 0,
        pk_point_time: 0,
        last_hitter: None,
        last_hit_time: 0,
        name_colour: mir_core::Color(-1),
        visible_monsters: BTreeSet::new(),
        visible_npcs: BTreeSet::new(),
        movement_time: 0,
        inventory: items::new_inventory(),
        equipment: items::new_equipment(),
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
        npc: NpcState::default(),
        group_id: None,
        group_invitation: None,
        group_invite_time: 0,
        allow_group: true,
        guild: None,
        trade: mir_server::TradeState::default(),
        trade_invitation: None,
        next_trade_time: 0,
        allow_trade: true,
        sender,
    });

    // `CHECKQUEST 154 COMPLETE` 不通过 → `#ELSEACT GOTO @MayNotEnter`。
    // Luke 的 `NpcInfo.FlagNeeded = 951`，所以先点亮标记 951（真实流程里是前置任务 `SET` 的）。
    world.player_mut(2).unwrap().npc.flags.insert(951);
    let reply = world.call_npc(2, luke.object_id, "@main").expect("对话");
    assert_eq!(reply.page_key, "[@MAYNOTENTER]");
    assert!(reply.page.iter().any(|line| line.contains("HALT")));

    // 完成任务 154 之后（脚本是空的 `999.txt`，没有目标 → 接到即可交付），
    // 同一个页面改走 `GOTO @MAIN-1`。Luke.txt 里没有 `[@Main-1]` 这一页，
    // 所以跟随 GOTO 失败、停在 `[@MAIN]` 且正文为空（before 是 `[@MAYNOTENTER]` + HALT）。
    world
        .accept_quest(2, luke.object_id, 154)
        .expect("接取 154");
    world.finish_quest(2, 154, -1).expect("交付 154");
    let reply = world.call_npc(2, luke.object_id, "@main").expect("对话");
    assert_eq!(reply.page_key, "[@MAIN]");
    assert!(
        reply.page.is_empty(),
        "完成之后走 `#ACT GOTO`，没有正文：{:?}",
        reply.page
    );
}

#[test]
fn login_packets_carry_the_quest_catalogue() {
    let (mut harness, _envir) = Harness::new("login-packets");
    let giver = harness.npc_id("任务使者");
    let player = harness.add_player(1, Point::new(100, 101));
    harness.world.accept_quest(player, giver, 1).expect("接取");
    harness.packets(player);

    let packets = harness.world.quest_login_packets(player);
    // 1) 全库任务各一条 `S.NewQuestInfo`（Crystal `GetQuestInfo`，`PlayerObject.cs:1185`、`:1727-1731`）。
    let new_infos: Vec<i32> = packets
        .iter()
        .filter_map(|packet| match packet {
            ServerPacket::NewQuestInfo(info) => Some(info.info.index),
            _ => None,
        })
        .collect();
    assert_eq!(new_infos, (1..=9).collect::<Vec<i32>>(), "9 个任务全套下发");
    // 2) `S.CompleteQuest`（`GetCompletedQuests`，`:1188`）。
    assert!(packets.iter().any(|packet| matches!(
        packet,
        ServerPacket::CompleteQuest(complete) if complete.completed_quests.is_empty()
    )));
    // 3) 已接任务的 `S.ChangeQuest{Add}`（`:1207-1212`，登录重放时 trackQuest = false）。
    let change = packets
        .iter()
        .find_map(|packet| match packet {
            ServerPacket::ChangeQuest(change) => Some(change),
            _ => None,
        })
        .expect("已接任务应有一条 ChangeQuest");
    assert_eq!(change.quest.id, 1);
    assert_eq!(change.quest_state, QuestState::Add);
    assert!(!change.track_quest, "登录重放不追踪");

    // `ClientQuestInfo` 里的字段来自「头部 + 脚本」（`QuestInfo.cs:385-415`）。
    let info = harness.world.client_quest_info(5).expect("任务 5 定义");
    assert_eq!(info.reward_gold, 0);
    assert_eq!(info.rewards_select_item.len(), 2);
    assert_eq!(info.rewards_select_item[0].item.name, "Sword");
    assert_eq!(info.class_needed, mir_core::enums::RequiredClass::None);
    assert!(info.same_finish_npc(), "发布者与交付者是同一个 NPC");
    // 没有目标数据时一个任务包都不发（避免空世界塞 `S.CompleteQuest{[]}`）。
    let mut empty = World::new(999);
    let (sender, _receiver) = mpsc::unbounded_channel();
    empty.enter(Player {
        object_id: 9,
        session_id: 9,
        character_id: 9,
        name: "空世界".to_owned(),
        map: MAP.to_owned(),
        position: Point::new(0, 0),
        direction: MirDirection::Down,
        level: 1,
        experience: 0,
        max_experience: 0,
        class: MirClass::Warrior,
        gender: MirGender::Male,
        hair: 1,
        hp: 15,
        mp: 0,
        stats: combat::player_base_stats(1, MirClass::Warrior),
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
        attack_mode: mir_core::enums::AttackMode::Peace,
        pk_points: 0,
        brown_time: 0,
        pk_point_time: 0,
        last_hitter: None,
        last_hit_time: 0,
        name_colour: mir_core::Color(-1),
        visible_monsters: BTreeSet::new(),
        visible_npcs: BTreeSet::new(),
        movement_time: 0,
        inventory: items::new_inventory(),
        equipment: items::new_equipment(),
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
        npc: NpcState::default(),
        group_id: None,
        group_invitation: None,
        group_invite_time: 0,
        allow_group: true,
        guild: None,
        trade: mir_server::TradeState::default(),
        trade_invitation: None,
        next_trade_time: 0,
        allow_trade: true,
        sender,
    });
    assert!(empty.quest_login_packets(9).is_empty());
}

#[test]
fn npc_object_packet_lists_script_quests() {
    let (harness, _envir) = Harness::new("npc-quest-ids");
    let giver = harness
        .world
        .npc(harness.npc_id("任务使者"))
        .expect("使者")
        .clone();
    // 数据库列表里的任务（`NpcInfo.CollectQuestIndexes` = 1,3..9；任务 2 归工匠）。
    assert_eq!(giver.quest_indexes, vec![1, 3, 4, 5, 6, 7, 8, 9]);
    assert_eq!(giver.object_packet().quest_i_ds, giver.quest_indexes);
    // 工匠只靠脚本 `[Quests]`：刷出时把正数编号并进名单，客户端才能画任务图标。
    let craft_lady = harness
        .world
        .npc(harness.npc_id("工匠"))
        .expect("工匠")
        .clone();
    assert_eq!(craft_lady.quest_indexes, vec![2]);
    assert_eq!(craft_lady.object_packet().quest_i_ds, vec![2]);
    // 旁观者不关联任务。
    let bystander = harness
        .world
        .npc(harness.npc_id("旁观者"))
        .expect("旁观者")
        .clone();
    assert!(bystander.quest_indexes.is_empty());
}

// ---------------------------------------------------------------------------
// 9. 未使用的脚手架（避免 dead_code 告警）
// ---------------------------------------------------------------------------

#[test]
fn harness_helpers_are_wired() {
    let (mut harness, _envir) = Harness::new("helpers");
    let giver = harness.npc_id("任务使者");
    let player = harness.add_player_with_level(1, Point::new(100, 101), 3);
    assert_eq!(harness.world.player(player).unwrap().level, 3);
    harness
        .world
        .call_npc(player, giver, "@main")
        .expect("对话");
    assert!(!page_text(&harness.packets(player)).is_empty());
    let _ = QuestProgressState::NotStarted;
    let _ = <MirDb as Clone>::clone(&database());
    let _ = mir_core::enums::QuestType::General;
}
