//! NPC 商店买卖的端到端用例（无 TCP）：卖 Chicken 的真实价格、买货的包序与 Rate、`[Types]` 过滤。
//!
//! 全部确定性：世界随机数固定种子；脚本与数据库在临时目录构造；唯一读真实 `resource/`
//! 的用例在 CI（没有素材）上打印跳过原因并返回。
//!
//! 对照 Crystal：
//! * 卖：`PlayerObject.SellItem`（`PlayerObject.cs:7988-8091`）——先 `S.SellItem{Success=true}`
//!   再 `GainGold(Price()/2)`（`:8082-8084`）；
//! * 买：`NPCScript.Buy`（`NPCScript.cs:1138-1237`）——`cost = Price() * PriceRate`（`:1187-1189`），
//!   先扣钱 `S.LoseGold`（`:1208-1209`）再 `GainItem` → `S.GainedItem`（`:1217`）；
//! * `[Types]`：`script.Types.Count != 0 && !Contains(temp.Info.Type)` → 系统聊天 + 失败
//!   （`:8041-8046`）；`[Types]` 只影响卖，不影响买。

mod common;

use std::path::PathBuf;
use std::sync::Arc;

use mir_core::Point;
use mir_core::enums::{ItemType, PanelType};
use mir_db::{MapInfo, NpcInfo};
use mir_protocol::ServerPacket;
use mir_server::gamedata::GameData;
use mir_server::npc::{BUY_SELL_KEY, NpcError, item_price};
use mir_world::{Map, MapCell, MapVersion};

use common::Harness;

const MAP: &str = common::MAP;
/// Jev 的 BichonProvince（map `0`）是 700×700；用同尺寸的空地让真实 NPC 坐标都“图内合法”。
const REAL_MAP_SIZE: i32 = 700;

// ---------------------------------------------------------------------------
// 脚手架
// ---------------------------------------------------------------------------

fn real_data() -> Option<Arc<GameData>> {
    match GameData::discover() {
        Ok(data) => Some(Arc::new(data)),
        Err(error) => {
            eprintln!("跳过真实数据用例：Jev 样本库不可用（{error}）");
            None
        }
    }
}

fn blank_map(name: &str, size: i32) -> Arc<Map> {
    let cells = usize::try_from(size * size).expect("地图尺寸");
    Map {
        width: size,
        height: size,
        cells: vec![MapCell::default(); cells],
        file_name: name.to_owned(),
        version: MapVersion::V0,
        doors: Vec::new(),
    }
    .into()
}

/// 临时 `Envir/`（脚本目录），退出时删除。
struct Envir {
    root: PathBuf,
}

impl Envir {
    fn new(tag: &str) -> Self {
        let root = std::env::temp_dir().join(format!(
            "mir-next-npc-shop-{tag}-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("NPCs/Test")).expect("建 NPC 目录");
        Self { root }
    }

    fn script(&self, text: &str) -> &Self {
        std::fs::write(self.root.join("NPCs/Test/Shop.txt"), text).expect("写脚本");
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

/// 合成一份「真实物品模板 + 自定义商店脚本」的数据：NPC 刷在 map `3` 的 (10,11)，
/// 玩家由 [`common::Harness`] 放在 (10,10)。
fn synthetic_shop(tag: &str, script: &str, rate: u16) -> (Harness, Envir, Arc<GameData>) {
    let Some(real) = real_data() else {
        panic!("这个用例需要真实模板；CI 无 resource/ 时应由调用方先检查");
    };
    let envir = Envir::new(tag);
    envir.script(script);
    let mut db = real.db.clone();
    db.maps = vec![MapInfo {
        index: 3,
        file_name: MAP.to_owned(),
        title: "测试省".to_owned(),
        ..MapInfo::default()
    }];
    db.npcs = vec![NpcInfo {
        index: 1,
        map_index: 3,
        file_name: "Test/Shop".to_owned(),
        name: "测试商人".to_owned(),
        location: Point::new(10, 11),
        rate,
        ..NpcInfo::default()
    }];
    let data = Arc::new(GameData::from_db(db, Some(envir.drop_dir())));
    let mut harness = Harness::new(0x5A_02);
    harness.world.set_game_data(Arc::clone(&data));
    let spawned = harness.world.spawn_npcs(MAP);
    assert_eq!(spawned.len(), 1, "应刷出 1 个 NPC");
    (harness, envir, data)
}

fn npc_id(harness: &Harness) -> u32 {
    harness.world.npcs()[0].object_id
}

fn open_shop(harness: &mut Harness, player: u32) {
    let id = npc_id(harness);
    harness.world.call_npc(player, id, "@main").expect("主页");
    harness
        .world
        .call_npc(player, id, "@BuySell")
        .expect("买卖页");
    harness.clear(player);
}

fn packet_position(packets: &[ServerPacket], matcher: impl Fn(&ServerPacket) -> bool) -> usize {
    packets
        .iter()
        .position(matcher)
        .unwrap_or_else(|| panic!("包里找不到目标，实际 {packets:?}"))
}

// ---------------------------------------------------------------------------
// 1. 卖：真实 Jev 的 Chicken 卖给真实屠夫，金币 = Price()/2，包序 SellItem → GainedGold
// ---------------------------------------------------------------------------

#[test]
fn selling_a_real_chicken_at_a_real_butcher_credits_half_price() {
    let Some(data) = real_data() else {
        return;
    };
    let mut harness = Harness::new(0x5A_01);
    harness.world.set_game_data(Arc::clone(&data));
    harness.world.insert_map("0", blank_map("0", REAL_MAP_SIZE));
    let player = harness.add_player(1, "卖肉测试", 1, 0);
    harness.teleport(player, "0", Point::new(313, 272));

    let spawned = harness.world.spawn_npcs("0");
    assert!(!spawned.is_empty(), "map0 应刷出真实 NPC");
    let butcher = harness
        .world
        .npcs()
        .iter()
        .find(|npc| npc.name == "Merchant_Sarah")
        .cloned()
        .expect("Jev map0 应有 Merchant_Sarah（Butcher1，Types=15）");
    assert_eq!(butcher.position, Point::new(313, 271));
    assert_eq!(butcher.rate, 100, "屠夫 Rate 默认 100%");

    harness
        .world
        .call_npc(player, butcher.object_id, "@main")
        .expect("进屠夫主页");
    // 真实 Butcher1 主页只挂了 `<Sell/@Sell> Meat.`（没有 @BuySell），走 [@SELL] 页。
    harness
        .world
        .call_npc(player, butcher.object_id, "@Sell")
        .expect("进卖肉页");

    // Chicken 是 Hen 的 1/1 掉落；用「全新」实例保证价格确定（80 → 卖 40）。
    let info = data.item_by_name("Chicken").cloned().expect("Chicken 模板");
    assert_eq!(info.item_type, ItemType::Meat);
    let chicken = harness.world.create_fresh_item(&info);
    let expected = item_price(&info, &chicken) / 2;
    assert_eq!(expected, 40, "Chicken 全新价 80 → 卖 40");
    assert!(
        harness.world.add_item_to_inventory(player, chicken.clone()),
        "背包应能放下"
    );
    harness.clear(player);

    let report = harness
        .world
        .sell_item(player, chicken.unique_id, 1)
        .expect("Chicken 应能卖给屠夫");
    assert_eq!(report.gold, expected);
    assert_eq!(report.count, 1);
    assert_eq!(
        harness.world.player(player).expect("玩家").gold,
        i64::from(expected),
        "金币必须进角色金币（GainGold，PlayerObject.cs:8084）"
    );
    assert!(
        harness
            .world
            .player(player)
            .expect("玩家")
            .inventory
            .iter()
            .flatten()
            .all(|item| item.unique_id != chicken.unique_id),
        "卖出后 Chicken 必须从背包消失（新手出生装还会留在包里）"
    );

    let packets = harness.packets(player);
    let sold_at = packet_position(
        &packets,
        |packet| matches!(packet, ServerPacket::SellItem(sold) if sold.success),
    );
    let gold_at = packet_position(
        &packets,
        |packet| matches!(packet, ServerPacket::GainedGold(gold) if gold.gold == expected),
    );
    assert!(
        sold_at < gold_at,
        "包序：S.SellItem 先、S.GainedGold 后（PlayerObject.cs:8082-8084），实际 {packets:?}"
    );
    assert!(
        !packets
            .iter()
            .any(|packet| matches!(packet, ServerPacket::LoseGold(_))),
        "卖东西不能发 S.LoseGold"
    );
}

// ---------------------------------------------------------------------------
// 2. 买：Rate 150% 时 cost = Price()*1.5，包序 LoseGold → GainedItem
// ---------------------------------------------------------------------------

#[test]
fn buying_an_item_applies_the_npc_rate_and_sends_lose_gold_before_gained_item() {
    let Some(_) = real_data() else {
        return;
    };
    let (mut harness, _envir, data) = synthetic_shop(
        "buy-rate",
        "[@Main]\n欢迎\n<买卖/@BuySell>\n\n[@BuySell]\n#SAY\n买点什么？\n\n[Types]\n15\n\n[Trade]\nChicken\n",
        150,
    );
    let player = harness.add_player(1, "买家", 1, 1_000);
    open_shop(&mut harness, player);

    let goods = harness.world.npc_goods(&harness.world.npcs()[0].clone());
    assert_eq!(goods.len(), 1, "商品表应有 1 件 Chicken");
    let unique_id = goods[0].unique_id;
    let info = data.item_by_name("Chicken").cloned().expect("Chicken 模板");
    let base = item_price(&info, &goods[0]);
    assert_eq!(base, 80);
    harness.clear(player);

    let report = harness
        .world
        .buy_item(player, unique_id, 1, PanelType::Buy)
        .expect("应该能买");
    assert_eq!(report.cost, 120, "Rate=150% → 80*1.5");
    assert_eq!(report.item.item_index, info.index);
    assert_eq!(
        harness.world.player(player).expect("玩家").gold,
        880,
        "金币 1000 - 120"
    );
    assert_eq!(
        harness
            .world
            .player(player)
            .expect("玩家")
            .inventory
            .iter()
            .flatten()
            .filter(|item| item.item_index == info.index)
            .count(),
        1,
        "背包里应出现 1 件 Chicken"
    );

    let packets = harness.packets(player);
    let lose_at = packet_position(
        &packets,
        |packet| matches!(packet, ServerPacket::LoseGold(lost) if lost.gold == 120),
    );
    let gain_at = packet_position(
        &packets,
        |packet| matches!(packet, ServerPacket::GainedItem(gained) if gained.item.item_index == info.index),
    );
    assert!(
        lose_at < gain_at,
        "包序：S.LoseGold 先、S.GainedItem 后（NPCScript.cs:1208-1217），实际 {packets:?}"
    );
}

// ---------------------------------------------------------------------------
// 3. 卖：`[Types]` 不匹配 → 系统聊天 + 失败（物品不动、金币不动）
// ---------------------------------------------------------------------------

#[test]
fn selling_a_type_outside_the_shop_types_is_refused_with_a_system_chat() {
    let Some(_) = real_data() else {
        return;
    };
    // `[Types] 1`（Weapon）→ 只收武器；Chicken 是 Meat(15)，必须被拒。
    let (mut harness, _envir, data) = synthetic_shop(
        "sell-types",
        "[@Main]\n欢迎\n<买卖/@BuySell>\n\n[@BuySell]\n#SAY\n卖点什么？\n\n[Types]\n1\n\n[Trade]\nChicken\n",
        100,
    );
    let player = harness.add_player(1, "卖错东西", 1, 0);
    open_shop(&mut harness, player);
    assert_eq!(
        harness.world.player(player).expect("玩家").npc.page_key,
        BUY_SELL_KEY,
        "`[@BuySell]` 打开后当前页 key 是 [@BUYSELL]"
    );

    let info = data.item_by_name("Chicken").cloned().expect("Chicken 模板");
    let chicken = harness.world.create_fresh_item(&info);
    assert!(harness.world.add_item_to_inventory(player, chicken.clone()));
    harness.clear(player);

    let error = harness
        .world
        .sell_item(player, chicken.unique_id, 1)
        .expect_err("类型不允许必须拒绝");
    assert_eq!(error, NpcError::TypeNotAllowed);
    assert_eq!(
        harness.world.player(player).expect("玩家").gold,
        0,
        "被拒时不能加金币"
    );
    assert!(
        harness
            .world
            .player(player)
            .expect("玩家")
            .inventory
            .iter()
            .flatten()
            .any(|item| item.unique_id == chicken.unique_id),
        "被拒时物品必须留在背包里（新手出生装也在包里，用 UUID 判定）"
    );

    let packets = harness.packets(player);
    assert!(
        packets.iter().any(|packet| matches!(
            packet,
            ServerPacket::Chat(chat) if chat.type_ == mir_core::enums::ChatType::System
        )),
        "应发一条系统聊天（CannotSellItemHere，PlayerObject.cs:8043），实际 {packets:?}"
    );
    assert!(
        packets
            .iter()
            .any(|packet| matches!(packet, ServerPacket::SellItem(sold) if !sold.success)),
        "应回 S.SellItem{{Success=false}}，实际 {packets:?}"
    );
    assert!(
        !packets
            .iter()
            .any(|packet| matches!(packet, ServerPacket::GainedGold(_))),
        "被拒时不能发 S.GainedGold"
    );
}
