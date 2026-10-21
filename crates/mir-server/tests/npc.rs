//! NPC 系统的端到端测试：脚本装载、NPC 刷出、进图下发、对话执行、商店买卖、修理。
//!
//! 全部确定性：
//! * 世界随机数固定种子（[`World::seed_rng`]）；
//! * 时间用 [`World::advance_clock`] 快进（不需要，NPC 逻辑没有计时器）；
//! * 脚本与数据库都在内存/临时目录里构造（每个用例一套独立目录，互不干扰），
//!   只有最后一个 `real_database_*` 用例读 `resource/` 里的真实数据，缺失时跳过。
//!
//! 每个断言尽量对应 Crystal 的一处行为，注释里写了出处（`文件.cs:行号`）。

use std::collections::{BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use mir_core::Point;
use mir_core::enums::{
    BindMode, ItemGrade, ItemType, MirClass, MirDirection, MirGender, PanelType, RequiredClass,
    Spell, Stat,
};
use mir_db::{MapInfo, MirDb, MovementInfo, NpcInfo};
use mir_protocol::ServerPacket;
use mir_protocol::data::{ItemInfo, Stats, UserItem};
use mir_server::MonsterKind;
use mir_server::NpcState;
use mir_server::buffs::BuffKind;
use mir_server::combat;
use mir_server::gamedata::GameData;
use mir_server::items;
use mir_server::npc::{
    BUY_KEY, NPC_TALK_RANGE, NpcError, SELL_KEY, item_price, parse_buff_kind, repair_price,
    strip_color_markup,
};
use mir_server::world::{Player, World};
use tokio::sync::mpsc;

const MAP: &str = "0";

// ---------------------------------------------------------------------------
// 测试脚手架
// ---------------------------------------------------------------------------

/// 每个用例一个独立的 `Envir/` 目录，退出时删除。
struct Envir {
    root: PathBuf,
}

impl Envir {
    fn new(tag: &str) -> Self {
        let root = std::env::temp_dir().join(format!(
            "mir-next-npc-{tag}-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("NPCs/Test")).expect("建 NPC 目录");
        Self { root }
    }

    /// 写一份 NPC 脚本（相对 `Envir/NPCs/` 的文件名）。
    fn script(&self, name: &str, text: &str) -> &Self {
        let path = self.root.join("NPCs").join(format!("{name}.txt"));
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).expect("建脚本目录");
        }
        std::fs::write(&path, text).expect("写脚本");
        self
    }

    /// 给 [`GameData::from_db`] 用的掉落目录参数（父目录正好是 `Envir/`）。
    fn drop_dir(&self) -> PathBuf {
        self.root.join("Drops")
    }
}

impl Drop for Envir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

/// 一件物品模板。
fn item(
    index: i32,
    name: &str,
    item_type: ItemType,
    weight: u8,
    durability: u16,
    price: u32,
    stack_size: u16,
) -> ItemInfo {
    ItemInfo {
        index,
        name: name.to_owned(),
        item_type,
        grade: ItemGrade::None,
        weight,
        image: u16::try_from(index).unwrap_or(0),
        durability,
        stack_size,
        price,
        stats: Stats::from_iter([(Stat::MaxDC, 2)]),
        ..ItemInfo::default()
    }
}

/// 测试用的最小数据库：4 件物品、1 张图 + 1 张目的地、2 个 NPC。
fn database() -> MirDb {
    MirDb {
        items: vec![
            item(1, "WoodenSword", ItemType::Weapon, 10, 5_000, 1_000, 1),
            item(2, "HpDrug", ItemType::Potion, 1, 0, 50, 20),
            ItemInfo {
                bind: BindMode::DontSell,
                ..item(3, "BoundSword", ItemType::Weapon, 10, 5_000, 1_000, 1)
            },
            item(4, "BrokenSword", ItemType::Weapon, 10, 5_000, 1_000, 1),
            ItemInfo {
                start_item: true,
                required_class: RequiredClass::Warrior,
                ..item(5, "StarterSword", ItemType::Weapon, 5, 1_000, 10, 1)
            },
            ItemInfo {
                start_item: true,
                required_class: RequiredClass::Wizard,
                ..item(6, "StarterStaff", ItemType::Weapon, 3, 1_000, 10, 1)
            },
            ItemInfo {
                shape: i16::from(Spell::FireBall.value()),
                ..item(7, "FireBallBook", ItemType::Book, 1, 0, 100, 1)
            },
            item(8, "TestArmour", ItemType::Armour, 10, 1_000, 500, 1),
        ],
        maps: vec![
            MapInfo {
                index: 1,
                file_name: MAP.to_owned(),
                title: "比奇省".to_owned(),
                big_map: 7,
                movements: vec![MovementInfo {
                    map_index: 2,
                    source: Point::new(330, 330),
                    destination: Point::new(10, 10),
                    need_hole: false,
                    need_move: false,
                    conquest_index: 0,
                    show_on_big_map: true,
                    icon: 5,
                }],
                ..MapInfo::default()
            },
            MapInfo {
                index: 2,
                file_name: "1".to_owned(),
                title: "边界村".to_owned(),
                ..MapInfo::default()
            },
        ],
        npcs: vec![
            NpcInfo {
                index: 1,
                map_index: 1,
                file_name: "Test/Shop".to_owned(),
                name: "测试商人".to_owned(),
                location: Point::new(100, 100),
                image: 5,
                rate: 100,
                show_on_big_map: true,
                big_map_icon: 2,
                can_teleport_to: true,
                ..NpcInfo::default()
            },
            NpcInfo {
                index: 2,
                map_index: 1,
                file_name: "Test/Repair".to_owned(),
                name: "测试铁匠".to_owned(),
                location: Point::new(101, 100),
                image: 6,
                rate: 50,
                ..NpcInfo::default()
            },
        ],
        ..MirDb::default()
    }
}

/// 商店脚本：`[@BuySell]` 是买卖同页，商品有剑与药水。
const SHOP_SCRIPT: &str = "\
[@Main]\n\
欢迎光临 <$USERNAME>，你的等级是 <$LEVEL>。\n\
你拿着 {<$WEAPON>/CORAL}。\n\
<买卖/@BuySell> <关闭/@exit>\n\
\n\
[@BuySell]\n\
#SAY\n\
你要买点什么？\n\
<返回/@Main>\n\
\n\
[@Sell]\n\
#SAY\n\
你要卖点什么？\n\
<返回/@Main>\n\
\n\
[Types]\n\
1\n\
2\n\
3\n\
4\n\
\n\
[Trade]\n\
WoodenSword\n\
HpDrug 5\n";

/// 铁匠脚本：等级不够时走 `#ELSEACT GOTO`，够等级才说话。
const REPAIR_SCRIPT: &str = "\
[@Main]\n\
#IF\n\
LEVEL > 10\n\
#SAY\n\
老主顾了。\n\
<修理/@Repair>\n\
#ELSEACT\n\
GOTO @LowLevel\n\
\n\
[@LowLevel]\n\
#SAY\n\
先去练练再来。\n\
<修理/@Repair>\n\
\n\
[@Repair]\n\
#SAY\n\
把要修的东西给我。\n\
<返回/@Main>\n\
\n\
[Types]\n\
1\n\
";

struct Harness {
    world: World,
    receivers: HashMap<u32, mpsc::UnboundedReceiver<Vec<u8>>>,
}

impl Harness {
    /// 建世界并刷出两个 NPC（`Shop` / `Repair`）。
    fn new(tag: &str) -> (Self, Envir) {
        let envir = Envir::new(tag);
        envir.script("Test/Shop", SHOP_SCRIPT);
        envir.script("Test/Repair", REPAIR_SCRIPT);
        let data = GameData::from_db(database(), Some(envir.drop_dir()));
        let mut world = World::new(999);
        world.seed_rng(0x5EED);
        world.set_game_data(Arc::new(data));
        let spawned = world.spawn_npcs(MAP);
        assert_eq!(spawned.len(), 2, "两个 NPC 都应该刷出来");
        (
            Self {
                world,
                receivers: HashMap::new(),
            },
            envir,
        )
    }

    /// 用自定义的 `Test/Shop` 脚本建世界（`#ACT` 动作测试用）。
    fn new_with_script(tag: &str, script: &str) -> (Self, Envir) {
        let envir = Envir::new(tag);
        envir.script("Test/Shop", script);
        envir.script("Test/Repair", REPAIR_SCRIPT);
        let data = GameData::from_db(database(), Some(envir.drop_dir()));
        let mut world = World::new(999);
        world.seed_rng(0x5EED);
        world.set_game_data(Arc::new(data));
        let spawned = world.spawn_npcs(MAP);
        assert_eq!(spawned.len(), 2, "两个 NPC 都应该刷出来");
        (
            Self {
                world,
                receivers: HashMap::new(),
            },
            envir,
        )
    }

    /// 进世界 + 站到 NPC 旁边，返回玩家对象 ID 与 NPC 对象 ID。
    fn ready(&mut self) -> (u32, u32) {
        let npc_id = self.npc_id("测试商人");
        let player = self.add_player(1, Point::new(100, 101));
        self.packets(player);
        (player, npc_id)
    }

    fn add_player(&mut self, object_id: u32, position: Point) -> u32 {
        let stats = combat::player_base_stats(10, MirClass::Warrior);
        let (sender, receiver) = mpsc::unbounded_channel();
        self.world.enter(Player {
            object_id,
            session_id: u64::from(object_id),
            character_id: i64::from(object_id),
            name: format!("玩家{object_id}"),
            map: MAP.to_owned(),
            position,
            direction: MirDirection::Down,
            level: 10,
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

    fn packets(&mut self, object_id: u32) -> Vec<ServerPacket> {
        let receiver = self.receivers.get_mut(&object_id).expect("玩家通道");
        let mut packets = Vec::new();
        while let Ok(frame) = receiver.try_recv() {
            let index = i16::from_le_bytes([frame[2], frame[3]]);
            packets.push(ServerPacket::decode(index, &frame[4..]).expect("解码服务端包"));
        }
        packets
    }

    fn npc_id(&self, name: &str) -> u32 {
        self.world
            .npcs()
            .iter()
            .find(|npc| npc.name == name)
            .map_or_else(|| panic!("找不到 NPC {name}"), |npc| npc.object_id)
    }

    fn give_item(&mut self, object_id: u32, info: &ItemInfo) -> UserItem {
        let item = self.world.create_fresh_item(info);
        assert!(
            self.world.add_item_to_inventory(object_id, item.clone()),
            "背包应该有空位"
        );
        item
    }
}

/// 找 `NPCResponse` 并返回正文。
fn npc_pages(packets: &[ServerPacket]) -> Vec<Vec<String>> {
    packets
        .iter()
        .filter_map(|packet| match packet {
            ServerPacket::NPCResponse(response) => Some(response.page.clone()),
            _ => None,
        })
        .collect()
}

// ---------------------------------------------------------------------------
// 1. 脚本装载
// ---------------------------------------------------------------------------

#[test]
fn scripts_load_from_the_database_directory() {
    let (harness, _envir) = Harness::new("scripts");
    let script = harness
        .world
        .npc_script("Test/Shop")
        .expect("脚本应该能加载");
    assert_eq!(script.file_name(), "Test/Shop");
    assert!(script.section("@main").is_some());
    assert!(script.section("[@BUYSELL]").is_some());
    // 按钮与 `#IF` 都在（`[@Sell]` 没有被任何按钮指向，但解析器照样收进来）。
    let main = script.section("@main").unwrap();
    assert_eq!(
        main.buttons,
        vec!["[@BUYSELL]".to_owned(), "[@EXIT]".to_owned()]
    );
    // `[Trade]` 两行（`NPCScript.cs:650-684`）。
    assert_eq!(script.goods().len(), 2);
    assert_eq!(script.goods()[0].name, "WoodenSword");
    assert_eq!(script.goods()[1].count, 5);
    // `[Types]` 四行。
    assert_eq!(script.types(), &[1, 2, 3, 4]);

    // 商店货架：`CreateShopItem` 的规则（`Envir.cs:4426-4439`）。
    let npc = harness
        .world
        .npc(harness.npc_id("测试商人"))
        .unwrap()
        .clone();
    let goods = harness.world.npc_goods(&npc);
    assert_eq!(goods.len(), 2);
    assert_eq!(goods[0].unique_id, script.goods()[0].unique_id);
    assert_eq!(goods[0].item_index, 1);
    assert_eq!(goods[0].max_dura, 5_000);
    assert!(goods[0].is_shop_item);
    assert_eq!(goods[1].count, 5);
}

#[test]
fn insert_directive_pulls_in_another_page() {
    let envir = Envir::new("insert");
    envir.script("Test/Shared", "[@Extra]\n#SAY\n来自共享脚本\n");
    envir.script(
        "Test/Main",
        "#INSERT [NPCs/Test/Shared.txt] @Main\n[@Main]\n#SAY\n主脚本\n",
    );
    let data = GameData::from_db(database(), Some(envir.drop_dir()));
    let script = data.npc_script("Test/Main").expect("脚本可加载");
    assert_eq!(
        script.section("@main").unwrap().segments[0].say,
        vec!["主脚本"]
    );
    // `#INSERT` 追加到文件尾 → 被插入的页是独立页面（Crystal `NPCScript.cs:322`）。
    assert_eq!(
        script.section("@extra").unwrap().segments[0].say,
        vec!["来自共享脚本"]
    );
    assert!(script.warnings().is_empty(), "{:?}", script.warnings());
}

// ---------------------------------------------------------------------------
// 2. NPC 刷出与进图下发
// ---------------------------------------------------------------------------

#[test]
fn spawned_npcs_carry_database_fields() {
    let (harness, _envir) = Harness::new("spawn");
    assert_eq!(harness.world.npcs().len(), 2);
    let npc = harness.world.npc(harness.npc_id("测试商人")).unwrap();
    assert_eq!(npc.map, MAP);
    assert_eq!(npc.index, 1);
    assert_eq!(npc.position, Point::new(100, 100));
    assert_eq!(npc.image, 5);
    assert_eq!(npc.rate, 100);
    assert!(npc.show_on_big_map);
    assert_eq!(npc.big_map_icon, 2);
    assert!(npc.can_teleport_to);
    assert_eq!(npc.file_name, "Test/Shop");

    // `ClientNPCInfo` 的 12 个字段（`Shared/Data/ClientData.cs:283-320`）。
    let client = npc.client_info();
    assert_eq!(client.index, 1);
    assert_eq!(client.file_name, "Test/Shop");
    assert_eq!(client.name, "测试商人");
    assert_eq!(client.location, Point::new(100, 100));
    assert_eq!(client.image, 5);
    assert_eq!(client.rate, 100);
    assert!(client.show_on_big_map);
    assert_eq!(client.big_map_icon, 2);
    assert_eq!(client.object_id, npc.object_id);
    assert_eq!(client.icon, 2, "Crystal 用 BigMapIcon 当 Icon");
    assert!(client.can_teleport_to);
}

#[test]
fn map_entry_info_lists_npcs_and_movements() {
    let (harness, _envir) = Harness::new("entry");
    let info = harness
        .world
        .map_entry_info(MAP, "比奇省")
        .expect("应该有进图资料");
    assert_eq!(info.title, "比奇省");
    assert_eq!(info.big_map, 7);
    // 传送点：目的地是 1 号图（`PlayerObject.cs:1030-1045`）。
    assert_eq!(info.movements.len(), 1);
    assert_eq!(info.movements[0].destination, 2);
    assert_eq!(info.movements[0].title, "边界村");
    assert_eq!(info.movements[0].icon, 5);
    // NPC 只收 `ShowOnBigMap` 的（`PlayerObject.cs:1047-1052`）。
    assert_eq!(info.npcs.len(), 1);
    assert_eq!(info.npcs[0].name, "测试商人");
    assert_eq!(
        info.npcs[0].map_index, 1,
        "ClientNPCInfo.MapIndex 是地图主键"
    );
}

#[test]
fn entering_world_sends_object_npc_and_npcs_block_movement() {
    let (mut harness, _envir) = Harness::new("visible");
    let npc_id = harness.npc_id("测试商人");
    let player = harness.add_player(1, Point::new(100, 101));
    let packets = harness.packets(player);
    let objects: Vec<_> = packets
        .iter()
        .filter_map(|packet| match packet {
            ServerPacket::ObjectNPC(npc) => Some(npc),
            _ => None,
        })
        .collect();
    assert_eq!(objects.len(), 2, "两个 NPC 都在视野里，应该各发一条");
    assert_eq!(objects[0].object_id, npc_id);
    assert_eq!(objects[0].image, 5);
    assert_eq!(objects[0].name, "测试商人");

    // NPC 挡路（`NPCObject.Blocking => Visible`，`NPCObject.cs:418-421`）。
    assert_eq!(
        harness.world.walk(player, MirDirection::Up),
        Err(mir_server::world::MoveError::Occupied { x: 100, y: 100 })
    );
}

// ---------------------------------------------------------------------------
// 3. 对话
// ---------------------------------------------------------------------------

#[test]
fn call_npc_returns_rendered_dialog() {
    let (mut harness, _envir) = Harness::new("dialog");
    let npc_id = harness.npc_id("测试商人");
    let player = harness.add_player(1, Point::new(100, 101));
    harness.packets(player);

    let reply = harness
        .world
        .call_npc(player, npc_id, "@main")
        .expect("对话应当成功");
    // 占位符替换 + 颜色标记剥掉（`NPCSegment.cs:1266-1641`）。
    assert_eq!(reply.page_key, "[@MAIN]".to_owned());
    assert_eq!(
        reply.page,
        vec![
            "欢迎光临 玩家1，你的等级是 10。".to_owned(),
            "你拿着 无。".to_owned(),
            "<买卖/@BuySell> <关闭/@exit>".to_owned(),
        ]
    );
    let packets = harness.packets(player);
    let pages = npc_pages(&packets);
    assert_eq!(pages.len(), 1);
    assert_eq!(pages[0], reply.page);
    assert!(reply.unsupported_checks.is_empty());
}

#[test]
fn button_jump_requires_a_button_that_was_shown() {
    let (mut harness, _envir) = Harness::new("buttons");
    let npc_id = harness.npc_id("测试商人");
    let player = harness.add_player(1, Point::new(100, 101));
    harness.packets(player);
    harness
        .world
        .call_npc(player, npc_id, "@main")
        .expect("进主页");

    // `[@BuySell]` 是主页展示过的按钮 → 允许（`NPCScript.cs:833-856`）。
    let reply = harness
        .world
        .call_npc(player, npc_id, "@BuySell")
        .expect("按钮跳转");
    assert_eq!(reply.page_key, "[@BUYSELL]");
    assert_eq!(
        reply.page,
        vec!["你要买点什么？".to_owned(), "<返回/@Main>".to_owned()]
    );
    // 这一页同时打开买卖面板（Crystal `ProcessSpecial`，`NPCScript.cs:927-953`）。
    assert_eq!(reply.panels, vec!["buy", "sell"]);

    // `[@Sell]` 从没在当前页出现过 → 拒绝。
    assert_eq!(
        harness.world.call_npc(player, npc_id, "@Sell"),
        Err(NpcError::NotACurrentButton {
            key: "[@SELL]".to_owned()
        })
    );

    // `[@Main]` 永远允许（Crystal `key != MainKey` 才校验）。
    let reply = harness
        .world
        .call_npc(player, npc_id, "@main")
        .expect("回主页");
    assert_eq!(reply.page_key, "[@MAIN]");
}

#[test]
fn failed_check_runs_else_actions_and_follows_goto() {
    let (mut harness, _envir) = Harness::new("elsegoto");
    let npc_id = harness.npc_id("测试铁匠");
    // 玩家 10 级，脚本要求 `LEVEL > 10` → 检查失败 → `#ELSEACT GOTO @LowLevel`。
    let player = harness.add_player(1, Point::new(101, 101));
    harness.packets(player);

    let reply = harness
        .world
        .call_npc(player, npc_id, "@main")
        .expect("对话成功");
    assert_eq!(reply.page_key, "[@LOWLEVEL]");
    assert_eq!(
        reply.page,
        vec!["先去练练再来。".to_owned(), "<修理/@Repair>".to_owned()]
    );

    // 每个页面一条 `S.NPCResponse`（Crystal `Response` 逐页发，`NPCScript.cs:895-900`）。
    let pages = npc_pages(&harness.packets(player));
    assert_eq!(pages.len(), 2, "主页（空正文）+ GOTO 目标页");
    assert!(pages[0].is_empty(), "失败分支只有 ELSEACT，没有正文");
    assert_eq!(pages[1], reply.page);
}

#[test]
fn high_level_player_gets_the_success_branch() {
    let (mut harness, _envir) = Harness::new("success");
    let npc_id = harness.npc_id("测试铁匠");
    let player = harness.add_player(1, Point::new(101, 101));
    harness.packets(player);
    // 把等级抬到 11（脚本要求 `LEVEL > 10`）。
    harness.world.player_mut(player).unwrap().level = 11;

    let reply = harness
        .world
        .call_npc(player, npc_id, "@main")
        .expect("对话成功");
    assert_eq!(reply.page_key, "[@MAIN]");
    assert_eq!(reply.page[0], "老主顾了。");
}

#[test]
fn exit_closes_the_dialog() {
    let (mut harness, _envir) = Harness::new("exit");
    let npc_id = harness.npc_id("测试商人");
    let player = harness.add_player(1, Point::new(100, 101));
    harness.packets(player);
    harness
        .world
        .call_npc(player, npc_id, "@main")
        .expect("进主页");
    assert_eq!(
        harness.world.player(player).unwrap().npc.npc_object_id,
        npc_id
    );

    let reply = harness
        .world
        .call_npc(player, npc_id, "@exit")
        .expect("退出");
    assert!(reply.closed);
    assert!(reply.page.is_empty());
    let state = &harness.world.player(player).unwrap().npc;
    assert_eq!(state.npc_object_id, 0);
    assert!(state.buttons.is_empty());
    // 一条空正文的 `S.NPCResponse`：客户端收到空页就关窗
    // （`Client/MirScenes/GameScene.cs:3891-3907` 的 `NPCResponse` 处理）。
    let pages = npc_pages(&harness.packets(player));
    assert_eq!(pages.last(), Some(&Vec::new()));

    // `CLOSE` 也当退出处理（本作扩展，见 `docs/world/npc.md`）。
    harness
        .world
        .call_npc(player, npc_id, "@main")
        .expect("重新进主页");
    let reply = harness
        .world
        .call_npc(player, npc_id, "close")
        .expect("退出");
    assert!(reply.closed);
    assert_eq!(reply.page_key, "[@EXIT]");
}

#[test]
fn dead_players_and_distant_npcs_are_rejected() {
    let (mut harness, _envir) = Harness::new("reject");
    let npc_id = harness.npc_id("测试商人");
    let player = harness.add_player(1, Point::new(100, 130));
    harness.packets(player);
    assert_eq!(
        harness.world.call_npc(player, npc_id, "@main"),
        Err(NpcError::OutOfRange {
            object_id: npc_id,
            range: NPC_TALK_RANGE
        })
    );

    // 走近之后可以对话，但死了就不行（Crystal `if (Dead) return;`，`PlayerObject.cs:7902`）。
    let map = harness.world.player(player).expect("玩家").map.clone();
    assert!(
        harness
            .world
            .place_player(player, &map, Point::new(100, 101))
    );
    harness
        .world
        .call_npc(player, npc_id, "@main")
        .expect("对话");
    harness.world.player_mut(player).unwrap().dead = true;
    assert_eq!(
        harness.world.call_npc(player, npc_id, "@main"),
        Err(NpcError::Dead { player })
    );
}

// ---------------------------------------------------------------------------
// 4. 商店：买
// ---------------------------------------------------------------------------

/// 打开商店，返回 (玩家, `WoodenSword` 的货号, `HpDrug` 的货号)。
fn open_shop(harness: &mut Harness) -> (u32, u64, u64) {
    let npc_id = harness.npc_id("测试商人");
    let player = harness.add_player(1, Point::new(100, 101));
    harness.packets(player);
    harness
        .world
        .call_npc(player, npc_id, "@main")
        .expect("进主页");
    harness
        .world
        .call_npc(player, npc_id, "@BuySell")
        .expect("进买卖页");
    let packets = harness.packets(player);
    let goods = packets
        .iter()
        .find_map(|packet| match packet {
            ServerPacket::NPCGoods(goods) => Some(goods.clone()),
            _ => None,
        })
        .expect("买卖页应该发商品表");
    assert!((goods.rate - 1.0).abs() < f32::EPSILON, "rate = Rate/100");
    assert_eq!(goods.type_, PanelType::Buy);
    assert!(
        goods.hide_added_stats,
        "Settings.GoodsHideAddedStats = true"
    );
    let sword = goods.list[0].unique_id;
    let drug = goods.list[1].unique_id;
    assert_eq!(goods.list[0].item_index, 1);
    (player, sword, drug)
}

#[test]
fn buying_an_item_costs_gold_and_sends_both_packets() {
    let (mut harness, _envir) = Harness::new("buy");
    let (player, sword, _drug) = open_shop(&mut harness);
    harness.world.player_mut(player).unwrap().gold = 5_000;
    harness.packets(player);

    let report = harness
        .world
        .buy_item(player, sword, 1, PanelType::Buy)
        .expect("应该能买");
    let info = harness.world.item_info(1).cloned().unwrap();
    assert_eq!(report.cost, item_price(&info, &report.item));
    assert_eq!(report.cost, 1_000);
    assert_eq!(harness.world.player(player).unwrap().gold, 4_000);

    let packets = harness.packets(player);
    let lost: Vec<u32> = packets
        .iter()
        .filter_map(|packet| match packet {
            ServerPacket::LoseGold(lost) => Some(lost.gold),
            _ => None,
        })
        .collect();
    assert_eq!(
        lost,
        vec![1_000],
        "扣钱先发 S.LoseGold（`NPCScript.cs:1209`）"
    );
    let gained: Vec<UserItem> = packets
        .iter()
        .filter_map(|packet| match packet {
            ServerPacket::GainedItem(gained) => Some(gained.item.clone()),
            _ => None,
        })
        .collect();
    assert_eq!(gained.len(), 1);
    assert_eq!(gained[0].item_index, 1);
    assert_eq!(gained[0].max_dura, 5_000);
    assert!(!gained[0].is_shop_item, "买到手的是 CreateFreshItem");
    // 背包里真的多了一件。
    let inventory = &harness.world.player(player).unwrap().inventory;
    assert_eq!(
        inventory
            .iter()
            .flatten()
            .filter(|item| item.item_index == 1)
            .count(),
        1
    );
}

#[test]
fn buying_without_gold_changes_nothing() {
    let (mut harness, _envir) = Harness::new("buy-poor");
    let (player, sword, _drug) = open_shop(&mut harness);
    harness.world.player_mut(player).unwrap().gold = 999;
    harness.packets(player);

    let error = harness
        .world
        .buy_item(player, sword, 1, PanelType::Buy)
        .expect_err("钱不够");
    assert_eq!(
        error,
        NpcError::NotEnoughGold {
            cost: 1_000,
            gold: 999
        }
    );
    assert_eq!(harness.world.player(player).unwrap().gold, 999);
    let packets = harness.packets(player);
    assert!(
        !packets
            .iter()
            .any(|packet| matches!(packet, ServerPacket::GainedItem(_))),
        "失败时不应该发 GainedItem"
    );
}

#[test]
fn buying_with_a_full_bag_is_refused() {
    let (mut harness, _envir) = Harness::new("buy-full");
    let (player, sword, _drug) = open_shop(&mut harness);
    harness.world.player_mut(player).unwrap().gold = 5_000;
    // 背包塞满 46 格（每格一件不可堆叠的剑）。
    let info = harness.world.item_info(1).cloned().unwrap();
    {
        let player_state = harness.world.player_mut(player).unwrap();
        for (index, slot) in player_state.inventory.iter_mut().enumerate() {
            *slot = Some(UserItem {
                unique_id: 9_000 + index as u64,
                ..harness_free_item(&info)
            });
        }
    }
    harness.packets(player);

    let error = harness
        .world
        .buy_item(player, sword, 1, PanelType::Buy)
        .expect_err("背包满");
    assert_eq!(error, NpcError::InventoryFull);
    assert_eq!(
        harness.world.player(player).unwrap().gold,
        5_000,
        "失败不能扣钱（Crystal 先查 `CanGainItem` 再扣钱，`NPCScript.cs:1200-1209`）"
    );
}

/// 造一件「不入库的」物品（只用于把背包塞满）。
fn harness_free_item(info: &ItemInfo) -> UserItem {
    UserItem {
        unique_id: 1,
        item_index: info.index,
        current_dura: info.durability,
        max_dura: info.durability,
        count: 1,
        ..UserItem::default()
    }
}

// ---------------------------------------------------------------------------
// 5. 商店：卖
// ---------------------------------------------------------------------------

#[test]
fn selling_an_item_gives_half_its_price() {
    let (mut harness, _envir) = Harness::new("sell");
    let npc_id = harness.npc_id("测试商人");
    let player = harness.add_player(1, Point::new(100, 101));
    harness.packets(player);
    // 直接进 `[@SELL]`（真实脚本里也有这个页面；这里手动把页面状态摆好）。
    harness
        .world
        .call_npc(player, npc_id, "@main")
        .expect("主页");
    {
        let state = &mut harness.world.player_mut(player).unwrap().npc;
        state.npc_object_id = npc_id;
        state.page_key = SELL_KEY.to_owned();
    }
    let info = harness.world.item_info(1).cloned().unwrap();
    let item = harness.give_item(player, &info);
    harness.packets(player);

    let report = harness
        .world
        .sell_item(player, item.unique_id, 1)
        .expect("应该能卖");
    assert_eq!(report.gold, 500, "Price()/2 = 500");
    assert_eq!(report.count, 1);
    let packets = harness.packets(player);
    let sold: Vec<bool> = packets
        .iter()
        .filter_map(|packet| match packet {
            ServerPacket::SellItem(sold) => Some(sold.success),
            _ => None,
        })
        .collect();
    assert_eq!(sold, vec![true], "先发成功包（`PlayerObject.cs:8082`）");
    let gained: Vec<u32> = packets
        .iter()
        .filter_map(|packet| match packet {
            ServerPacket::GainedGold(gold) => Some(gold.gold),
            _ => None,
        })
        .collect();
    assert_eq!(gained, vec![500]);
    assert!(
        harness
            .world
            .player(player)
            .unwrap()
            .inventory
            .iter()
            .all(Option::is_none)
    );
}

#[test]
fn selling_a_bound_item_is_refused() {
    let (mut harness, _envir) = Harness::new("sell-bound");
    let npc_id = harness.npc_id("测试商人");
    let player = harness.add_player(1, Point::new(100, 101));
    harness.packets(player);
    harness
        .world
        .call_npc(player, npc_id, "@main")
        .expect("主页");
    {
        let state = &mut harness.world.player_mut(player).unwrap().npc;
        state.npc_object_id = npc_id;
        state.page_key = SELL_KEY.to_owned();
    }
    let info = harness.world.item_info(3).cloned().unwrap();
    let item = harness.give_item(player, &info);
    harness.packets(player);

    assert_eq!(
        harness.world.sell_item(player, item.unique_id, 1),
        Err(NpcError::CannotSell(item.unique_id)),
        "BindMode.DontSell（`PlayerObject.cs:8027-8031`）"
    );
    let packets = harness.packets(player);
    let sold: Vec<bool> = packets
        .iter()
        .filter_map(|packet| match packet {
            ServerPacket::SellItem(sold) => Some(sold.success),
            _ => None,
        })
        .collect();
    assert_eq!(
        sold,
        vec![false],
        "失败也要回一条 SellItem 且 Success=false"
    );
    assert_eq!(harness.world.player(player).unwrap().gold, 0);
}

#[test]
fn selling_outside_a_shop_page_is_refused() {
    let (mut harness, _envir) = Harness::new("sell-page");
    let npc_id = harness.npc_id("测试商人");
    let player = harness.add_player(1, Point::new(100, 101));
    harness.packets(player);
    harness
        .world
        .call_npc(player, npc_id, "@main")
        .expect("主页");
    let info = harness.world.item_info(1).cloned().unwrap();
    let item = harness.give_item(player, &info);
    harness.packets(player);
    // 主页（`[@MAIN]`）不是卖东西的页面。
    assert_eq!(
        harness.world.sell_item(player, item.unique_id, 1),
        Err(NpcError::NotShopPage {
            page: "[@MAIN]".to_owned()
        })
    );
}

// ---------------------------------------------------------------------------
// 6. 修理
// ---------------------------------------------------------------------------

/// 进铁匠的修理页，返回 (玩家, 待修物品)。
fn open_repair(harness: &mut Harness) -> (u32, UserItem) {
    let npc_id = harness.npc_id("测试铁匠");
    let player = harness.add_player(1, Point::new(101, 101));
    harness.packets(player);
    harness
        .world
        .call_npc(player, npc_id, "@main")
        .expect("主页");
    // 主页的 `LEVEL > 10` 不满足 → GOTO `[@LowLevel]`，那里有修理按钮。
    harness
        .world
        .call_npc(player, npc_id, "@Repair")
        .expect("进修理页");
    let mut item = harness.give_item(player, &harness.world.item_info(4).cloned().unwrap());
    // 打个 20% 的耐久出来（5000 → 1000）。
    if let Some(slot) = harness
        .world
        .player_mut(player)
        .unwrap()
        .inventory
        .iter_mut()
        .flatten()
        .find(|slot| slot.unique_id == item.unique_id)
    {
        slot.current_dura = 1_000;
        item = slot.clone();
    }
    harness.packets(player);
    (player, item)
}

#[test]
fn repairing_restores_durability_and_costs_gold() {
    let (mut harness, _envir) = Harness::new("repair");
    let (player, item) = open_repair(&mut harness);
    harness.world.player_mut(player).unwrap().gold = 1_000;
    harness.packets(player);

    let info = harness.world.item_info(4).cloned().unwrap();
    // 铁匠的 `Rate = 50` → `PriceRate = 0.5`（`NPCScript.cs:111-133`），
    // 200 * 0.5 = 100（截断）。
    let expected_cost = repair_price(&info, &item) / 2;
    let report = harness
        .world
        .repair_item(player, item.unique_id)
        .expect("应该能修");
    // 铁匠 rate = 50 → PriceRate = 0.5（`NPCScript.cs:111-133`）。
    assert_eq!(report.cost, expected_cost);
    assert_eq!(report.cost, 100);
    assert_eq!(report.max_dura, 4_867, "5000 - (5000-1000)/30");
    assert_eq!(report.current_dura, report.max_dura);
    assert_eq!(harness.world.player(player).unwrap().gold, 900);

    let packets = harness.packets(player);
    // 顺序照抄 Crystal：先 `S.RepairItem`（`:8094`），再 `S.LoseGold`（`:8149`），
    // 最后 `S.ItemRepaired`（`:8157`）。
    let indices: Vec<i16> = packets
        .iter()
        .filter_map(|packet| match packet {
            ServerPacket::RepairItem(_) => Some(1),
            ServerPacket::LoseGold(_) => Some(2),
            ServerPacket::ItemRepaired(_) => Some(3),
            _ => None,
        })
        .collect();
    assert_eq!(indices, vec![1, 2, 3]);
    let repaired = packets
        .iter()
        .find_map(|packet| match packet {
            ServerPacket::ItemRepaired(repaired) => Some(repaired.clone()),
            _ => None,
        })
        .expect("要有 ItemRepaired");
    assert_eq!(repaired.unique_id, item.unique_id);
    assert_eq!(repaired.max_dura, 4_867);
    assert_eq!(repaired.current_dura, 4_867);
}

#[test]
fn repairing_without_gold_changes_nothing() {
    let (mut harness, _envir) = Harness::new("repair-poor");
    let (player, item) = open_repair(&mut harness);
    harness.world.player_mut(player).unwrap().gold = 50;
    harness.packets(player);

    assert_eq!(
        harness.world.repair_item(player, item.unique_id),
        Err(NpcError::NotEnoughGold {
            cost: 100,
            gold: 50
        })
    );
    let packets = harness.packets(player);
    assert_eq!(
        packets
            .iter()
            .filter(|packet| matches!(packet, ServerPacket::RepairItem(_)))
            .count(),
        1,
        "只有开头那条 RepairItem，没有 ItemRepaired"
    );
    assert!(
        !packets
            .iter()
            .any(|packet| matches!(packet, ServerPacket::ItemRepaired(_)))
    );
    assert_eq!(harness.world.player(player).unwrap().gold, 50);
    // 耐久没有变化。
    let stored = harness
        .world
        .player(player)
        .unwrap()
        .inventory
        .iter()
        .flatten()
        .find(|slot| slot.unique_id == item.unique_id)
        .cloned()
        .expect("物品还在");
    assert_eq!(stored.current_dura, 1_000);
}

// ---------------------------------------------------------------------------
// 7. 金币双向变动
// ---------------------------------------------------------------------------

#[test]
fn gold_moves_both_ways_through_the_npc() {
    let (mut harness, _envir) = Harness::new("gold");
    let (player, sword, _drug) = open_shop(&mut harness);
    harness.world.player_mut(player).unwrap().gold = 2_000;
    harness.packets(player);
    harness
        .world
        .buy_item(player, sword, 1, PanelType::Buy)
        .expect("买");
    // 换到 `[@SELL]` 页把刚买的剑卖回去。
    {
        let state = &mut harness.world.player_mut(player).unwrap().npc;
        state.page_key = SELL_KEY.to_owned();
    }
    let bought = harness
        .world
        .player(player)
        .unwrap()
        .inventory
        .iter()
        .flatten()
        .find(|item| item.item_index == 1)
        .cloned()
        .expect("刚买的剑");
    harness
        .world
        .sell_item(player, bought.unique_id, 1)
        .expect("卖");

    let packets = harness.packets(player);
    let lost: u32 = packets
        .iter()
        .filter_map(|packet| match packet {
            ServerPacket::LoseGold(gold) => Some(gold.gold),
            _ => None,
        })
        .sum();
    let gained: u32 = packets
        .iter()
        .filter_map(|packet| match packet {
            ServerPacket::GainedGold(gold) => Some(gold.gold),
            _ => None,
        })
        .sum();
    assert_eq!(lost, 1_000);
    assert_eq!(gained, 500);
    assert_eq!(harness.world.player(player).unwrap().gold, 1_500);
}

// ---------------------------------------------------------------------------
// 8. 其它工具
// ---------------------------------------------------------------------------

#[test]
fn color_markup_is_unwrapped() {
    assert_eq!(
        strip_color_markup("你拿着 {<$WEAPON>/CORAL}。"),
        "你拿着 <$WEAPON>。"
    );
    assert_eq!(
        strip_color_markup("{Level48~55./KHAKI} Required"),
        "Level48~55. Required"
    );
    assert_eq!(strip_color_markup("没有颜色"), "没有颜色");
    // 未闭合的原样保留（不吞字）。
    assert_eq!(strip_color_markup("{没闭合/red"), "{没闭合/red");
}

#[test]
fn prices_follow_crystal_formulas() {
    let info = item(1, "Sword", ItemType::Weapon, 10, 5_000, 1_000, 1);
    let full = UserItem {
        item_index: 1,
        max_dura: 5_000,
        current_dura: 5_000,
        count: 1,
        ..UserItem::default()
    };
    assert_eq!(item_price(&info, &full), 1_000);
    // floor(5000 * 0.1 / 2 + 500 * 0.5 + 500)
    let half = UserItem {
        current_dura: 2_500,
        ..full.clone()
    };
    assert_eq!(item_price(&info, &half), 875);
    assert_eq!(repair_price(&info, &half), 125);
    let worn = UserItem {
        current_dura: 1_000,
        ..full.clone()
    };
    assert_eq!(item_price(&info, &worn), 800);
    assert_eq!(repair_price(&info, &worn), 200);
    // 堆叠物品：`Price()` 乘数量（`ItemData.cs:541`）。
    let potion = item(2, "Potion", ItemType::Potion, 1, 0, 50, 20);
    let stack = UserItem {
        item_index: 2,
        count: 5,
        current_dura: 0,
        max_dura: 0,
        ..UserItem::default()
    };
    assert_eq!(item_price(&potion, &stack), 250);
    assert_eq!(repair_price(&potion, &stack), 0, "Durability == 0 → 0");
}

// ---------------------------------------------------------------------------
// 9. 真实数据冒烟
// ---------------------------------------------------------------------------

/// `resource/` 里的 Jev 数据集（缺失时跳过）：比奇省的真实 NPC 与脚本。
#[test]
fn real_database_npcs_and_scripts() {
    let Ok(data) = GameData::discover() else {
        eprintln!("跳过：resource/ 里没有 Server.MirDB 样本");
        return;
    };
    let Some(envir) = data.envir_dir().map(Path::to_path_buf) else {
        eprintln!("跳过：数据库不在文件系统上");
        return;
    };
    if !envir.join("NPCs").is_dir() {
        eprintln!("跳过：{} 下没有 NPCs 目录", envir.display());
        return;
    }
    let mut world = World::new(999);
    world.seed_rng(2024);
    world.set_game_data(Arc::new(data));
    let spawned = world.spawn_npcs(MAP);
    assert!(!spawned.is_empty(), "比奇省应该有 NPC");
    eprintln!("比奇省 NPC 数量：{}", spawned.len());

    // 至少有一份脚本能解析，且有页面/按钮。
    let mut parsed = 0;
    let mut with_pages = 0;
    for npc in world.npcs() {
        if let Some(script) = world.npc_script(&npc.file_name) {
            parsed += 1;
            if !script.sections().is_empty() {
                with_pages += 1;
            }
        }
    }
    eprintln!("能解析的脚本：{parsed}，其中有页面的：{with_pages}");
    assert!(parsed > 0, "至少一份脚本能解析");
    assert!(with_pages > 0, "至少一份脚本有页面");

    // 进图资料里应该有真实的传送点（比奇省 movements 很多）与 NPC。
    let info = world.map_entry_info(MAP, "比奇省").expect("应该有进图资料");
    assert!(!info.movements.is_empty(), "比奇省应该有大地图传送点");
    eprintln!(
        "进图资料：movements={} npcs={}",
        info.movements.len(),
        info.npcs.len()
    );
}

/// 真实数据上的对话/商店冒烟：找一个带 `[Trade]` 的 NPC，确认商品表非空。
#[test]
fn real_database_shop_has_goods() {
    let Ok(data) = GameData::discover() else {
        eprintln!("跳过：resource/ 里没有 Server.MirDB 样本");
        return;
    };
    if data.envir_dir().is_none() {
        eprintln!("跳过：数据库不在文件系统上");
        return;
    }
    let mut world = World::new(999);
    world.set_game_data(Arc::new(data));
    world.spawn_npcs(MAP);
    let mut checked = 0;
    for npc in world.npcs() {
        let goods = world.npc_goods(npc);
        if goods.is_empty() {
            continue;
        }
        checked += 1;
        assert!(
            goods.iter().all(|goods| goods.item_index > 0),
            "商品必须解析到真实模板"
        );
        if checked == 1 {
            eprintln!("{} 的商品数：{}", npc.name, goods.len());
        }
    }
    if checked == 0 {
        eprintln!("跳过：比奇省的 NPC 里没有带商品的（数据裁剪过）");
        return;
    }
    assert!(checked > 0);
}

/// 常量表对外可见，且与 Crystal 的页面 key 一致（`NPCScript.cs:44-81`）。
#[test]
fn constants_are_exposed() {
    assert_eq!(BUY_KEY, "[@BUY]");
    assert!(mir_server::npc::BUY_PAGES.contains(&BUY_KEY));
    assert!(mir_server::npc::SELL_PAGES.contains(&SELL_KEY));
    assert_eq!(mir_server::npc::REPAIR_PAGES, &["[@REPAIR]"]);
}

// ---------------------------------------------------------------------------
// 9. `#ACT` 脚本动作：TAKECHECKITEM / TAKEITEM / GIVENAMELIST / GIVEBUFF
// ---------------------------------------------------------------------------

/// 系统聊天（`S.Chat{System}`）里有这条文案吗。
fn has_system_chat(packets: &[ServerPacket], expected: &str) -> bool {
    packets.iter().any(|packet| match packet {
        ServerPacket::Chat(chat) => chat.message == expected,
        _ => false,
    })
}

/// 「交出一件 `CHECKITEM` 选中的物品」脚本。
const TAKE_CHECK_ITEM_SCRIPT: &str = "\
[@Main]\n\
#IF\n\
CHECKITEM WoodenSword 1\n\
#ACT\n\
TAKECHECKITEM\n\
#SAY\n\
收下了。\n";

#[test]
fn take_check_item_hands_over_the_item_and_sends_delete_item() {
    let (mut harness, _envir) = Harness::new_with_script("takecheckitem", TAKE_CHECK_ITEM_SCRIPT);
    let (player, npc_id) = harness.ready();
    let info = harness
        .world
        .item_info_by_name("WoodenSword")
        .expect("模板")
        .clone();
    let sword = harness.give_item(player, &info);
    harness.packets(player);

    let reply = harness
        .world
        .call_npc(player, npc_id, "@main")
        .expect("对话应当成功");
    assert_eq!(reply.page, vec!["收下了。".to_owned()]);
    // 动作被认领，不再进 `unsupported_actions`。
    assert!(reply.unsupported_actions.is_empty(), "{reply:?}");
    // 物品真的没了。
    assert!(
        harness
            .world
            .find_inventory_item(player, sword.unique_id)
            .is_none(),
        "交出的物品应从背包消失"
    );
    let packets = harness.packets(player);
    let deleted: Vec<(u64, u16)> = packets
        .iter()
        .filter_map(|packet| match packet {
            ServerPacket::DeleteItem(item) => Some((item.unique_id, item.count)),
            _ => None,
        })
        .collect();
    assert_eq!(deleted, vec![(sword.unique_id, 1)], "应发一条 S.DeleteItem");
    assert!(
        has_system_chat(&packets, "已交出 WoodenSword ×1。"),
        "{packets:?}"
    );
}

#[test]
fn take_item_keyword_shares_the_take_check_item_path() {
    let (mut harness, _envir) = Harness::new_with_script(
        "takeitem",
        "[@Main]\n#IF\nCHECKITEM WoodenSword 1\n#ACT\nTAKEITEM\n#SAY\n收下了。\n",
    );
    let (player, npc_id) = harness.ready();
    let info = harness
        .world
        .item_info_by_name("WoodenSword")
        .expect("模板")
        .clone();
    let sword = harness.give_item(player, &info);
    harness.packets(player);

    let reply = harness
        .world
        .call_npc(player, npc_id, "@main")
        .expect("对话应当成功");
    assert!(reply.unsupported_actions.is_empty(), "{reply:?}");
    assert!(
        harness
            .world
            .find_inventory_item(player, sword.unique_id)
            .is_none()
    );
    let packets = harness.packets(player);
    assert!(packets.iter().any(|packet| matches!(
        packet,
        ServerPacket::DeleteItem(item) if item.unique_id == sword.unique_id
    )));
}

#[test]
fn take_check_item_without_a_checked_item_refuses_and_says_why() {
    let (mut harness, _envir) = Harness::new_with_script(
        "takecheckitem-missing",
        "[@Main]\n#ACT\nTAKECHECKITEM\n#SAY\n没东西可收。\n",
    );
    let (player, npc_id) = harness.ready();
    // 背包里有一把剑，但这一页没有 `CHECKITEM`，动作不该凭空拿走任何东西。
    let info = harness
        .world
        .item_info_by_name("WoodenSword")
        .expect("模板")
        .clone();
    let sword = harness.give_item(player, &info);
    harness.packets(player);

    let reply = harness
        .world
        .call_npc(player, npc_id, "@main")
        .expect("对话仍然成功");
    assert!(reply.unsupported_actions.is_empty());
    assert!(
        harness
            .world
            .find_inventory_item(player, sword.unique_id)
            .is_some(),
        "没有 CHECKITEM 选中物品时不能删东西"
    );
    let packets = harness.packets(player);
    assert!(
        !packets
            .iter()
            .any(|packet| matches!(packet, ServerPacket::DeleteItem(_)))
    );
    assert!(
        has_system_chat(&packets, "没有可交出的物品。"),
        "应给玩家一条失败原因：{packets:?}"
    );
}

#[test]
fn give_buff_adds_the_buff_with_the_scripted_duration() {
    let (mut harness, _envir) = Harness::new_with_script(
        "givebuff",
        "[@Main]\n#ACT\nGIVEBUFF magicshield 30\n#SAY\n好了。\n",
    );
    let (player, npc_id) = harness.ready();

    let reply = harness
        .world
        .call_npc(player, npc_id, "@main")
        .expect("对话应当成功");
    assert!(reply.unsupported_actions.is_empty(), "{reply:?}");
    assert!(
        harness.world.has_buff(player, BuffKind::MagicShield),
        "脚本给了魔法盾"
    );
    let packets = harness.packets(player);
    let duration = packets
        .iter()
        .find_map(|packet| match packet {
            ServerPacket::AddBuff(buff) => Some(buff.buff.expire_time),
            _ => None,
        })
        .expect("应发 S.AddBuff");
    // 30 秒（允许实现里有几百毫秒的时钟误差）。
    assert!(
        (29_000..=30_000).contains(&duration),
        "时长应约 30 秒，实际 {duration}"
    );
    assert!(
        has_system_chat(&packets, "获得增益：magicshield，持续 30 秒。"),
        "{packets:?}"
    );
}

#[test]
fn give_buff_defaults_to_one_minute_when_the_duration_is_missing() {
    let (mut harness, _envir) = Harness::new_with_script(
        "givebuff-default",
        "[@Main]\n#ACT\nGIVEBUFF Fury\n#SAY\n好了。\n",
    );
    let (player, npc_id) = harness.ready();
    harness
        .world
        .call_npc(player, npc_id, "@main")
        .expect("对话应当成功");
    assert!(harness.world.has_buff(player, BuffKind::Fury));
    let packets = harness.packets(player);
    let duration = packets
        .iter()
        .find_map(|packet| match packet {
            ServerPacket::AddBuff(buff) => Some(buff.buff.expire_time),
            _ => None,
        })
        .expect("应发 S.AddBuff");
    assert!(
        (59_000..=60_000).contains(&duration),
        "缺时长时默认 60 秒，实际 {duration}"
    );
}

#[test]
fn give_buff_rejects_unknown_names_and_bad_durations() {
    let (mut harness, _envir) = Harness::new_with_script(
        "givebuff-bad",
        "[@Main]\n#ACT\nGIVEBUFF NoSuchBuff 10\nGIVEBUFF Haste abc\n#SAY\n完了。\n",
    );
    let (player, npc_id) = harness.ready();
    harness
        .world
        .call_npc(player, npc_id, "@main")
        .expect("对话应当成功");
    // 两条都失败：名字不认识、时长不是数字。
    assert!(!harness.world.has_buff(player, BuffKind::Haste));
    let packets = harness.packets(player);
    assert!(
        has_system_chat(&packets, "未知的增益类型：NoSuchBuff。"),
        "{packets:?}"
    );
    assert!(
        has_system_chat(&packets, "增益时长必须是 1~2592000 之间的秒数，收到：abc。"),
        "{packets:?}"
    );
}

#[test]
fn give_name_list_offers_buttons_and_requests_input() {
    let (mut harness, _envir) = Harness::new_with_script(
        "givenamelist",
        "[@Main]\n#ACT\nGIVENAMELIST Alice Bob\n#SAY\n选一个。\n\
         \n[@Alice]\n#SAY\n我是爱丽丝。\n\
         \n[@Bob]\n#SAY\n我是鲍勃。\n",
    );
    let (player, npc_id) = harness.ready();

    let reply = harness
        .world
        .call_npc(player, npc_id, "@main")
        .expect("对话应当成功");
    assert!(reply.unsupported_actions.is_empty(), "{reply:?}");
    // 名字进白名单（与脚本按钮同格式）。
    let buttons = harness.world.player(player).unwrap().npc.buttons.clone();
    assert!(buttons.contains("[@ALICE]"), "{buttons:?}");
    assert!(buttons.contains("[@BOB]"), "{buttons:?}");
    // 输入框请求（`S.NPCRequestInput`）。
    let packets = harness.packets(player);
    let input = packets
        .iter()
        .find_map(|packet| match packet {
            ServerPacket::NPCRequestInput(input) => Some(input.clone()),
            _ => None,
        })
        .expect("应发 S.NPCRequestInput");
    assert_eq!(input.npcid, npc_id);
    assert_eq!(input.page_name, "[@MAIN]");
    assert!(
        has_system_chat(&packets, "请从下列名字中选择，或直接输入名字。"),
        "{packets:?}"
    );

    // 「点了某个名字」就等于发回 `@名字`：白名单放行，进入对应页面。
    let reply = harness
        .world
        .call_npc(player, npc_id, "@alice")
        .expect("点名字按钮应当成功");
    assert_eq!(reply.page_key, "[@ALICE]");
    assert_eq!(reply.page, vec!["我是爱丽丝。".to_owned()]);
}

#[test]
fn give_name_list_without_names_reports_empty_list() {
    let (mut harness, _envir) = Harness::new_with_script(
        "givenamelist-empty",
        "[@Main]\n#ACT\nGIVENAMELIST\n#SAY\n没有名字。\n",
    );
    let (player, npc_id) = harness.ready();
    harness
        .world
        .call_npc(player, npc_id, "@main")
        .expect("对话应当成功");
    let packets = harness.packets(player);
    assert!(
        has_system_chat(&packets, "名字列表为空。"),
        "应提示名字列表为空：{packets:?}"
    );
    // 即使列表为空，也仍然请求输入（玩家可以自己敲名字）。
    assert!(
        packets
            .iter()
            .any(|packet| matches!(packet, ServerPacket::NPCRequestInput(_)))
    );
}

#[test]
fn book_shop_goods_can_sell_skill_books() {
    let script = "[@Main]\n#SAY\n书店\n<关闭/@exit>\n[@BUY]\n#SAY\n技能书\n<关闭/@exit>\n[Trade]\nFireBallBook 1\n";
    let (harness, _envir) = Harness::new_with_script("book-shop", script);
    let npc = harness
        .world
        .npc(harness.npc_id("测试商人"))
        .expect("书店 NPC")
        .clone();
    let goods = harness.world.npc_goods(&npc);
    assert_eq!(goods.len(), 1);
    let book = harness
        .world
        .item_info(goods[0].item_index)
        .expect("书模板");
    assert_eq!(book.item_type, ItemType::Book);
    assert_eq!(book.shape, i16::from(Spell::FireBall.value()));
    assert_eq!(goods[0].count, 1);
}

#[test]
fn level_one_newbie_gets_the_first_skill_book_for_free() {
    let script = "[@Main]\n#SAY\n书店\n<买/@BuySell> <关闭/@exit>\n[@BuySell]\n#SAY\n技能书\n<关闭/@exit>\n[Trade]\nFireBallBook 1\n";
    let (mut harness, _envir) = Harness::new_with_script("newbie-book", script);
    let (player, npc_id) = harness.ready();
    if let Some(player) = harness.world.player_mut(player) {
        player.level = 1;
        player.gold = 0;
    }
    harness
        .world
        .call_npc(player, npc_id, "@main")
        .expect("打开书店");
    harness
        .world
        .call_npc(player, npc_id, "@BuySell")
        .expect("打开买卖页");
    let npc = harness.world.npc(npc_id).expect("NPC").clone();
    let goods = harness.world.npc_goods(&npc);
    let report = harness
        .world
        .buy_item(player, goods[0].unique_id, 1, PanelType::Buy)
        .expect("新手第一本书应免费");
    assert_eq!(report.cost, 0, "新手第一本技能书免金币");
    assert_eq!(harness.world.player(player).unwrap().gold, 0);
}

#[test]
fn starter_items_filter_by_class_from_start_item_flag() {
    let (harness, _envir) = Harness::new("starter-items");
    let warrior = harness.world.starter_items_for(MirClass::Warrior);
    assert_eq!(
        warrior
            .iter()
            .map(|item| item.item_index)
            .collect::<Vec<_>>(),
        vec![5],
        "战士只拿到 Warrior 的 StartItem"
    );
    let wizard = harness.world.starter_items_for(MirClass::Wizard);
    assert_eq!(
        wizard
            .iter()
            .map(|item| item.item_index)
            .collect::<Vec<_>>(),
        vec![6],
        "法师只拿到 Wizard 的 StartItem"
    );

    // 真正走一次 enter：1 级空背包角色应把出生装放进背包。
    let mut harness = harness;
    let player = harness.add_player(1, Point::new(10, 10));
    let mut stored = harness.world.leave(player).expect("离开");
    stored.level = 1;
    stored.inventory = items::new_inventory();
    harness.world.enter(stored);
    let equipped: Vec<i32> = harness
        .world
        .player(player)
        .expect("玩家")
        .inventory
        .iter()
        .flatten()
        .map(|item| item.item_index)
        .collect();
    assert_eq!(equipped, vec![5], "1 级战士进世界应自动获得出生装");
}

#[test]
fn parse_buff_kind_ignores_case_and_separators() {
    assert_eq!(parse_buff_kind("MagicShield"), Some(BuffKind::MagicShield));
    assert_eq!(parse_buff_kind("magic_shield"), Some(BuffKind::MagicShield));
    assert_eq!(parse_buff_kind("MAGIC-SHIELD"), Some(BuffKind::MagicShield));
    assert_eq!(parse_buff_kind("exp"), Some(BuffKind::Exp));
    // 本作没有实现的 Crystal `BuffType`（`Weak`）与拼错的名字都返回 `None`。
    assert_eq!(parse_buff_kind("Weak"), None);
    assert_eq!(parse_buff_kind("NoSuchBuff"), None);
}

// ---------------------------------------------------------------------------
// 7. 耐久损耗与装备失效（Crystal `HumanObject.DamageWeapon` / `DamageDura`）
// ---------------------------------------------------------------------------

/// 把一件物品直接塞进**装备栏**（`EquipmentSlot.Weapon = 0`），返回它的 `unique_id`。
fn equip_weapon(harness: &mut Harness, player: u32) -> UserItem {
    let info = harness.world.item_info(4).cloned().expect("铁剑模板");
    let item = equip_item(harness, player, &info, items::EQUIPMENT_SLOT_WEAPON);
    harness.world.refresh_stats(player);
    item
}

/// 把物品塞进指定装备槽（`current_dura` 先设满）。
fn equip_item(harness: &mut Harness, player: u32, info: &ItemInfo, slot: usize) -> UserItem {
    let mut item = UserItem {
        unique_id: 900_000 + u64::from(slot as u32),
        item_index: info.index,
        current_dura: info.durability,
        max_dura: info.durability,
        count: 1,
        ..UserItem::default()
    };
    if let Some(player_ref) = harness.world.player_mut(player) {
        player_ref.equipment[slot] = Some(item.clone());
    }
    item.unique_id = 900_000 + u64::from(slot as u32);
    item
}

#[test]
fn attacking_wears_the_weapon_by_one_to_four_points() {
    let (mut harness, _envir) = Harness::new("wear-weapon");
    let player = harness.add_player(1, Point::new(101, 101));
    let item = equip_weapon(&mut harness, player);
    harness.packets(player);

    let before = harness.world.player(player).unwrap().equipment[items::EQUIPMENT_SLOT_WEAPON]
        .as_ref()
        .unwrap()
        .current_dura;

    // 打 200 次，统计每次的损耗：Crystal `Random.Next(4) + 1` → 1..=4。
    let mut losses = Vec::new();
    for _ in 0..200 {
        let loss = harness.world.damage_weapon(player).expect("有武器就该磨损");
        losses.push(i32::from(before) - i32::from(loss.current_dura));
        // 复原，保证每次都从满耐久开始算。
        harness.world.player_mut(player).unwrap().equipment[items::EQUIPMENT_SLOT_WEAPON]
            .as_mut()
            .unwrap()
            .current_dura = before;
    }
    assert!(
        losses.iter().all(|loss| (1..=4).contains(loss)),
        "每次磨损应在 1..=4 点（Crystal `Random.Next(4)+1`），实际 {losses:?}"
    );
    assert!(
        losses.iter().any(|loss| *loss > 1),
        "200 次里应该出现过 >1 点的磨损（否则 RNG 没生效）"
    );
    let _ = item;
}

#[test]
fn broken_weapon_loses_its_stats_and_notifies_once() {
    let (mut harness, _envir) = Harness::new("wear-broken");
    let player = harness.add_player(1, Point::new(101, 101));
    equip_weapon(&mut harness, player);
    harness.world.refresh_stats(player);
    let attack_with_full = harness.world.player(player).unwrap().stats.max_dc;
    harness.packets(player);

    // 把耐久打到 1，再磨一次 → 归零。
    if let Some(item) = harness
        .world
        .player_mut(player)
        .unwrap()
        .equipment
        .get_mut(items::EQUIPMENT_SLOT_WEAPON)
        .and_then(Option::as_mut)
    {
        item.current_dura = 1;
    }
    let loss = harness.world.damage_weapon(player).expect("应该能磨损");
    assert!(loss.broke, "这一下应该把武器打坏");
    assert_eq!(loss.current_dura, 0);

    // 装备失效：不再提供属性（`apply_equipment_stats` 的 `CurrentDura == 0` 早退）。
    let attack_broken = harness.world.player(player).unwrap().stats.max_dc;
    assert!(
        attack_broken < attack_with_full,
        "耐久归零后武器不该再加攻击（{attack_broken} 应小于 {attack_with_full}）"
    );

    let packets = harness.packets(player);
    assert!(
        packets
            .iter()
            .any(|packet| matches!(packet, ServerPacket::DuraChanged(changed) if changed.current_dura == 0)),
        "打坏时要发 S.DuraChanged(76)"
    );
    let notices = packets
        .iter()
        .filter(
            |packet| matches!(packet, ServerPacket::Chat(chat) if chat.message.contains("耐久")),
        )
        .count();
    assert_eq!(
        notices, 1,
        "「装备损坏」提示只发一次（TextKey::ItemBroken）"
    );

    // 再磨一次：已经 0 耐久，什么都不该发生（Crystal `DamageItem` 早退）。
    assert!(
        harness.world.damage_weapon(player).is_none(),
        "耐久为 0 的装备不再参与磨损"
    );
}

#[test]
fn taking_damage_wears_every_slot_except_the_weapon() {
    let (mut harness, _envir) = Harness::new("wear-armour");
    let player = harness.add_player(1, Point::new(101, 101));
    // 0 = 武器，1 = 盔甲（`EquipmentSlot.Armour`）。
    let weapon = equip_weapon(&mut harness, player);
    let armour_info = harness.world.item_info(4).cloned().unwrap();
    let armour = equip_item(&mut harness, player, &armour_info, 1);
    harness.packets(player);

    let losses = harness.world.damage_armour(player);
    assert!(
        losses
            .iter()
            .all(|loss| loss.slot != items::EQUIPMENT_SLOT_WEAPON),
        "受击磨损不碰武器槽（Crystal `DamageDura` 的 `i != Weapon`）"
    );
    assert!(losses.iter().any(|loss| loss.slot == 1), "盔甲槽应该 -1");
    let weapon_now = harness.world.player(player).unwrap().equipment[items::EQUIPMENT_SLOT_WEAPON]
        .as_ref()
        .unwrap()
        .current_dura;
    assert_eq!(weapon_now, weapon.current_dura, "武器耐久不该被受击磨损改");
    let armour_now = harness.world.player(player).unwrap().equipment[1]
        .as_ref()
        .unwrap()
        .current_dura;
    assert_eq!(armour_now, armour.current_dura - 1, "每个槽位固定 -1");
}

#[test]
fn repairing_a_full_durability_item_is_rejected_with_a_notice() {
    let (mut harness, _envir) = Harness::new("repair-full");
    let (player, item) = open_repair(&mut harness);
    harness.world.player_mut(player).unwrap().gold = 10_000;
    // 修满：`open_repair` 已经把耐久压到 1000，先修一次。
    harness
        .world
        .repair_item(player, item.unique_id)
        .expect("第一次能修");
    harness.packets(player);

    let error = harness
        .world
        .repair_item(player, item.unique_id)
        .expect_err("满耐久不该能修");
    assert_eq!(error, NpcError::DurabilityFull(item.unique_id));
    let packets = harness.packets(player);
    assert!(
        !packets
            .iter()
            .any(|packet| matches!(packet, ServerPacket::ItemRepaired(_))),
        "满耐久时不该发 ItemRepaired"
    );
    assert!(
        packets
            .iter()
            .any(|packet| matches!(packet, ServerPacket::Chat(chat) if chat.message.contains("耐久是满的"))),
        "要给「耐久已满」提示（TextKey::NpcRepairFull）"
    );
}

#[test]
fn repairing_without_gold_explains_the_cost() {
    let (mut harness, _envir) = Harness::new("repair-poor-notice");
    let (player, item) = open_repair(&mut harness);
    harness.world.player_mut(player).unwrap().gold = 50;
    harness.packets(player);

    assert!(harness.world.repair_item(player, item.unique_id).is_err());
    let packets = harness.packets(player);
    let notice = packets
        .iter()
        .find_map(|packet| match packet {
            ServerPacket::Chat(chat)
                if chat.message.contains("100") && chat.message.contains("50") =>
            {
                Some(chat.message.clone())
            }
            _ => None,
        })
        .expect("要给「金币不足」提示（TextKey::NpcRepairNoGold）");
    assert!(notice.contains("金币"), "提示里应有金币字样：{notice}");
}

#[test]
fn durability_survives_an_inventory_round_trip() {
    // `CurrentDurability` 在 `characters.inventory` 的字节块里（`UserItem` 的第 3 个字段），
    // 所以「重登后耐久保持」= 编码→解码后 `current_dura` 不变。
    let (mut harness, _envir) = Harness::new("dura-roundtrip");
    let player = harness.add_player(1, Point::new(101, 101));
    let info = harness.world.item_info(4).cloned().unwrap();
    let item = harness.give_item(player, &info);
    if let Some(slot) = harness
        .world
        .player_mut(player)
        .unwrap()
        .inventory
        .iter_mut()
        .flatten()
        .find(|slot| slot.unique_id == item.unique_id)
    {
        slot.current_dura = 1_234;
    }
    let inventory = harness.world.player(player).unwrap().inventory.clone();
    let blob = items::encode_slots(&inventory);
    let decoded = items::decode_inventory(&blob).expect("背包字节块应能解析");
    let stored = decoded
        .iter()
        .flatten()
        .find(|slot| slot.unique_id == item.unique_id)
        .expect("物品还在");
    assert_eq!(stored.current_dura, 1_234, "耐久必须随角色存档一起持久化");
    assert_eq!(stored.max_dura, info.durability);
}

#[test]
fn monster_hit_wears_armour_in_the_same_settlement() {
    let (mut harness, _envir) = Harness::new("hit-wear");
    let player = harness.add_player(1, Point::new(100, 101));
    let info = harness.world.item_info(8).cloned().expect("测试护甲模板");
    let mut armour = harness_free_item(&info);
    armour.current_dura = 100;
    armour.max_dura = 100;
    harness.world.player_mut(player).unwrap().equipment[1] = Some(armour);
    let kind = MonsterKind {
        name: "打手".to_owned(),
        image: mir_core::enums::Monster::Oma,
        ai: 0,
        level: 1,
        view_range: 7,
        cool_eye: 0,
        attack_speed_ms: 500,
        move_speed_ms: 1_000,
        experience: 0,
        can_push: false,
        stats: combat::CombatStats {
            hp: 100,
            min_dc: 5,
            max_dc: 5,
            accuracy: 100,
            ..combat::CombatStats::default()
        },
    };
    let monster = harness.world.spawn_monster(MAP, kind, Point::new(100, 102));
    {
        let monster = harness.world.monster_mut(monster).unwrap();
        monster.target = Some(player);
        monster.operate_time = -1;
        monster.action_time = -1;
        monster.attack_time = -1;
        monster.move_time = -1;
        monster.search_time = -1;
        monster.alone_time = -1;
    }
    harness.world.tick();
    harness.world.advance_clock(1_000);
    harness.world.tick();
    let dura = harness.world.player(player).unwrap().equipment[1]
        .as_ref()
        .unwrap()
        .current_dura;
    assert!(
        dura < 100,
        "怪物命中应在同一次伤害结算里磨损护甲，实际耐久 {dura}"
    );
}

#[test]
fn durability_drop_is_sent_once_per_dura_delay() {
    let (mut harness, _envir) = Harness::new("dura-flush");
    let player = harness.add_player(1, Point::new(101, 101));
    let item = equip_weapon(&mut harness, player);
    if let Some(slot) = harness.world.player_mut(player).unwrap().equipment[0].as_mut() {
        slot.current_dura = 100;
        slot.max_dura = 100;
    }
    // 对齐到一次边界：即使 Harness 初始化期间已经 tick 过，也在下一次
    // 完整 DuraDelay 后把相位设成「当前 + 10s」，不再假设周期从哪次 tick 开始。
    harness.world.tick();
    harness.world.advance_clock(items::DURABILITY_FLUSH_MS);
    harness.world.tick();
    harness.packets(player);

    let loss = harness.world.damage_weapon(player).expect("武器应磨损");
    assert!(loss.current_dura < 100);
    // 扣耐久后立即 tick 不应发 76（周期还没到）。
    harness.world.tick();
    assert!(
        !harness
            .packets(player)
            .iter()
            .any(|packet| matches!(packet, ServerPacket::DuraChanged(_))),
        "未到 DuraDelay 不应发 76"
    );

    // 以 1s 步长推进，最多 15s；不追 1ms 边界，避免并行时钟相位导致假红。
    let mut changed = Vec::new();
    for _ in 0..15 {
        harness.world.advance_clock(1_000);
        harness.world.tick();
        changed.extend(
            harness
                .packets(player)
                .into_iter()
                .filter_map(|packet| match packet {
                    ServerPacket::DuraChanged(changed) => Some(changed),
                    _ => None,
                }),
        );
        if !changed.is_empty() {
            break;
        }
    }
    assert_eq!(
        changed.len(),
        1,
        "一个周期应恰好发一条 76（1s 步进，最多 15s）"
    );
    assert_eq!(changed[0].unique_id, item.unique_id);
    assert_eq!(changed[0].current_dura, loss.current_dura);

    // 无变化时下一个周期不再发。
    harness.world.advance_clock(items::DURABILITY_FLUSH_MS);
    harness.world.tick();
    assert!(
        !harness
            .packets(player)
            .iter()
            .any(|packet| matches!(packet, ServerPacket::DuraChanged(_))),
        "无脏槽不应发 76"
    );
}

#[test]
fn multiple_durability_losses_in_one_period_are_coalesced() {
    let (mut harness, _envir) = Harness::new("dura-coalesce");
    let player = harness.add_player(1, Point::new(101, 101));
    let item = equip_weapon(&mut harness, player);
    if let Some(slot) = harness.world.player_mut(player).unwrap().equipment[0].as_mut() {
        slot.current_dura = 1_000;
        slot.max_dura = 1_000;
    }
    // 同样对齐相位，不依赖 Harness 之前是否 tick 过。
    harness.world.tick();
    harness.world.advance_clock(items::DURABILITY_FLUSH_MS);
    harness.world.tick();
    harness.packets(player);

    let mut latest = None;
    for _ in 0..3 {
        latest = Some(harness.world.damage_weapon(player).expect("武器应磨损"));
    }
    harness.world.tick();
    assert!(
        !harness
            .packets(player)
            .iter()
            .any(|packet| matches!(packet, ServerPacket::DuraChanged(_))),
        "未到 DuraDelay 不应发 76"
    );

    let mut changed = Vec::new();
    for _ in 0..15 {
        harness.world.advance_clock(1_000);
        harness.world.tick();
        changed.extend(
            harness
                .packets(player)
                .into_iter()
                .filter_map(|packet| match packet {
                    ServerPacket::DuraChanged(changed) => Some(changed),
                    _ => None,
                }),
        );
        if !changed.is_empty() {
            break;
        }
    }
    assert_eq!(changed.len(), 1, "同一件装备一周期合并成一条 76");
    assert_eq!(changed[0].unique_id, item.unique_id);
    assert_eq!(changed[0].current_dura, latest.unwrap().current_dura);
}
