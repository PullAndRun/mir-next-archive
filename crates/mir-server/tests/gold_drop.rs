//! 金币掉落链路取证（第 26 轮 EQ-1 配套）：真实 Jev 数据 + 确定性单测。
//!
//! # 数据集
//!
//! **本文件显式钉住 `Jev`（英文）数据集**：断言使用 `Hen`/`Chicken`/`Scarecrow0`
//! 等 Jev 名字与 `Provinces\...` 反斜杠掉落路径，验证的是 **Jev 数据契约**。
//! 默认数据集（`cjlaaa` 中文，见 `docs/world/data-languages.md`）的中文对应物是
//! `鸡`/`鸡肉`/`稻草人`，行为用例见 `docs/world/economy.md`。
//!
//! # 背景
//!
//! EQ-1 的 C 段卡在「修理要 2 金币，角色 0 金币」，工部打怪 90s 没见到金币。
//! 本文件用 `Server.MirDB` 的 `MonsterInfo.DropPath` + `Envir/Drops/**` 的真实数据回答
//! 「哪些怪掉金币」，并把「击杀 → 金币堆 → `S.ObjectGold` → 拾取 → `S.GainedGold`」
//! 做成不依赖随机性的用例。
//!
//! # 真实数据事实（Jev v88 样本，`DB_CANDIDATES[0]`）
//!
//! | 怪 | 等级 | 经验 | `DropPath` | 金币 |
//! |---|---|---|---|---|
//! | `Hen` | 2 | 9 | `Provinces\Hen` | **无**（`1/1 Chicken`、`1/20 Feather`） |
//! | `Deer` | 12 | 18 | `Provinces\Deer` | **无**（`1/1 Venison`、`1/2 DeerMeat Q`） |
//! | `Scarecrow` | 10 | 15 | `Provinces\Scarecrow` | `1/10 Gold 50`（[25,75)） |
//! | `Scarecrow0` | 10 | 15 | `Provinces\Scarecrow0` | **`1/1 Gold 2000`**（[1000,3000)） |
//!
//! 注意 `Envir/Drops/` 根目录下还有三个**同名空文件**（`Hen.txt`/`Deer.txt`/`Scarecrow.txt`），
//! 它们没有被任何 `DropPath` 引用（真实路径在 `Provinces\` 子目录），别拿错文件下结论。
//!
//! 结论：**1~9 级怪没有任何金币掉落**；最早能掉金币的是 10 级 `Scarecrow`（1/10），
//! 而 `Scarecrow0` 是一次必掉 2000 基准的刷金怪。Hen/Deer 只掉可卖给 NPC 的肉/羽毛，
//! 金币要靠「卖店」而不是「捡钱」——这是数据事实，不改数据。
//!
//! 全部用例在缺 `resource/` 的 CI 上打印跳过原因并直接返回（与 `tests/gameshop.rs` 同口径）。

mod common;

use std::sync::Arc;

use mir_core::Point;
use mir_core::enums::MirDirection;
use mir_db::drops::{DropInfo, DropKind};
use mir_protocol::ServerPacket;
use mir_server::gamedata::GameData;

use common::Harness;

const MAP: &str = common::MAP;

/// 真实 Jev 数据（显式钉住，不随默认数据集漂移）；缺失（CI）时返回 `None`，调用方跳过。
fn real_data() -> Option<Arc<GameData>> {
    if let Some(data) = common::jev_data() {
        Some(data)
    } else {
        eprintln!("skip: Jev Server.MirDB not available under resource/");
        None
    }
}

/// 递归收集一个掉落表里所有 `Gold` 行的基准值（组内金币也算）。
fn gold_bases(drops: &[DropInfo], out: &mut Vec<u32>) {
    for drop in drops {
        match &drop.kind {
            DropKind::Gold(gold) => out.push(*gold),
            DropKind::Group(group) => gold_bases(&group.children, out),
            DropKind::Item(_) => {}
        }
    }
}

fn table_gold(table: &mir_db::drops::DropTable) -> Vec<(u32, u32)> {
    fn walk(drops: &[DropInfo], out: &mut Vec<(u32, u32)>) {
        for drop in drops {
            match &drop.kind {
                DropKind::Gold(gold) => out.push((drop.chance, *gold)),
                DropKind::Group(group) => walk(&group.children, out),
                DropKind::Item(_) => {}
            }
        }
    }
    let mut out = Vec::new();
    walk(&table.drops, &mut out);
    out
}

/// 把玩家挪到怪左边，一直砍到怪死（虚拟时钟，不 sleep）。
///
/// 真实怪有 AC/敏捷，所以用超高 DC/命中把随机性压掉；玩家血量也调高，避免在重试
/// 期间被怪反杀。最多砍 8 刀——正常 1 刀就死，循环只是防「命中判定偶尔不中」。
fn kill_monster(harness: &mut Harness, player: u32, monster: u32) {
    let (target, map) = {
        let monster = harness.world.monster(monster).expect("怪物还在");
        (monster.position, monster.map.clone())
    };
    let from = Point::new(target.x - 1, target.y);
    assert!(
        harness.world.place_player(player, &map, from),
        "玩家应能站到怪物旁边"
    );
    if let Some(p) = harness.world.player_mut(player) {
        p.direction = MirDirection::Right;
        p.hp = 1_000_000;
        p.stats.hp = 1_000_000;
        p.stats.min_dc = 10_000;
        p.stats.max_dc = 10_000;
        p.stats.accuracy = 10_000;
    }
    for _ in 0..8 {
        if harness
            .world
            .monster(monster)
            .is_none_or(|monster| monster.dead)
        {
            return;
        }
        if let Some(p) = harness.world.player_mut(player) {
            p.action_time = 0;
            p.attack_time = 0;
        }
        let _ = harness.world.attack(player, MirDirection::Right);
        // Crystal 的伤害延迟 300ms 落地（`MapObject.Attacked` 入队），之后 tick 结算死亡。
        harness.world.advance_clock(300);
        harness.world.tick();
    }
    assert!(
        harness
            .world
            .monster(monster)
            .is_none_or(|monster| monster.dead),
        "8 刀内应击杀 {}",
        harness
            .world
            .monster(monster)
            .map_or_else(|| "?".to_owned(), |monster| monster.kind.name.clone())
    );
}

/// 用真实数据里的怪名刷一只怪（模板来自 `Envir.MonsterInfoList`）。
fn spawn_real_monster(harness: &mut Harness, data: &GameData, name: &str, position: Point) -> u32 {
    let info = data
        .monster_info_by_name(name)
        .unwrap_or_else(|| panic!("Jev 数据里应有怪物模板 {name}"));
    let kind = data
        .monster(info.index)
        .unwrap_or_else(|| panic!("{name} 的 MonsterIndex {} 没有运行期种类", info.index))
        .clone();
    harness.world.spawn_monster(MAP, kind, position)
}

// ---------------------------------------------------------------------------
// A1. 数据事实：Hen/Deer 不掉金币，Scarecrow 掉，Scarecrow0 必掉
// ---------------------------------------------------------------------------

#[test]
fn real_jev_data_hen_and_deer_have_no_gold_but_scarecrow_does() {
    let Some(data) = real_data() else {
        return;
    };

    // Hen：`Provinces\Hen` = `1/1 Chicken` + `1/20 Feather`，没有 Gold 行。
    let hen = data.monster_drop_table("Hen").expect("Hen 应有掉落表");
    assert!(
        table_gold(&hen).is_empty(),
        "数据事实：Hen 的掉落表里没有金币，实际 {:?}",
        table_gold(&hen)
    );
    assert!(
        hen.drops
            .iter()
            .any(|drop| drop.kind == DropKind::Item("Chicken".to_owned()) && drop.chance == 1),
        "Hen 必掉 Chicken（Crystal 掉落表原文 1/1 Chicken），实际 {:?}",
        hen.drops
    );

    // Deer：`Provinces\Deer` = `1/1 Venison` + `1/2 DeerMeat Q`，没有 Gold 行。
    let deer = data.monster_drop_table("Deer").expect("Deer 应有掉落表");
    assert!(
        table_gold(&deer).is_empty(),
        "数据事实：Deer 的掉落表里没有金币，实际 {:?}",
        table_gold(&deer)
    );
    assert!(
        deer.drops
            .iter()
            .any(|drop| drop.kind == DropKind::Item("Venison".to_owned()) && drop.chance == 1),
        "Deer 必掉 Venison，实际 {:?}",
        deer.drops
    );

    // Scarecrow：`1/10 Gold 50`（`DropInfo.Gold` = 50 → 实际 [25,75)）。
    let scarecrow = data
        .monster_drop_table("Scarecrow")
        .expect("Scarecrow 应有掉落表");
    assert_eq!(
        table_gold(&scarecrow),
        vec![(10, 50)],
        "Scarecrow 的 Gold 行应为 1/10 基准 50，实际 {:?}",
        table_gold(&scarecrow)
    );

    // Scarecrow0：`1/1 Gold 2000`，这正是确定性端到端用例用的必掉怪。
    let scarecrow0 = data
        .monster_drop_table("Scarecrow0")
        .expect("Scarecrow0 应有掉落表");
    assert_eq!(
        table_gold(&scarecrow0),
        vec![(1, 2_000)],
        "Scarecrow0 的 Gold 行应为 1/1 基准 2000，实际 {:?}",
        table_gold(&scarecrow0)
    );

    // 用 DB 里的等级字段复核「1 级怪根本没有金币」：1..=9 级的所有怪都没有 Gold 行。
    let mut level_rows = Vec::new();
    for info in &data.db.monsters {
        if !(1..10).contains(&info.level) {
            continue;
        }
        let mut golds = Vec::new();
        if let Some(table) = data.monster_drop_table(&info.name) {
            gold_bases(&table.drops, &mut golds);
        }
        level_rows.push((info.level, info.name.clone(), golds));
    }
    let offenders: Vec<_> = level_rows.iter().filter(|row| !row.2.is_empty()).collect();
    assert!(
        offenders.is_empty(),
        "数据事实：Jev 数据里 1..9 级怪都不掉金币，意外发现 {offenders:?}"
    );

    // 打印取证表：最低等级、且掉落表带 Gold 的怪。
    let mut rows = Vec::new();
    for info in &data.db.monsters {
        let mut golds = Vec::new();
        if let Some(table) = data.monster_drop_table(&info.name) {
            gold_bases(&table.drops, &mut golds);
        }
        if !golds.is_empty() && info.level > 0 {
            rows.push((info.level, info.name.clone(), info.experience, golds));
        }
    }
    rows.sort_by(|a, b| (a.0, &a.1).cmp(&(b.0, &b.1)));
    eprintln!("=== 真实 Jev 数据：等级 > 0 且带 Gold 的怪（按等级排序）===");
    for (level, name, exp, golds) in rows.iter().take(12) {
        eprintln!("lv{level:<3} {name:<24} exp={exp:<6} gold={golds:?}");
    }
    let first = rows.first().expect("Jev 数据里至少应有一只掉金币的怪");
    assert_eq!(
        first.0, 10,
        "最低等级的金币怪应是 10 级（Scarecrow/Scarecrow0），实际 {first:?}"
    );
}

// ---------------------------------------------------------------------------
// A2. 确定性端到端：杀会掉金币的怪 → 金币堆 + ObjectGold → 拾取 → GainedGold
// ---------------------------------------------------------------------------

#[test]
fn killing_a_real_scarecrow0_drops_gold_then_pickup_credits_and_broadcasts() {
    let Some(data) = real_data() else {
        return;
    };
    let mut harness = Harness::new(0x26_01);
    harness.world.set_game_data(Arc::clone(&data));
    let player = harness.add_player(1, "金币测试", 1, 0);
    let monster = spawn_real_monster(&mut harness, &data, "Scarecrow0", Point::new(12, 10));
    harness.clear(player);

    kill_monster(&mut harness, player, monster);
    assert!(
        harness
            .world
            .monster(monster)
            .is_none_or(|monster| monster.dead),
        "Scarecrow0 应被击杀"
    );

    // 1/1 Gold 2000 → 实际区间 [1000,3000)；超过 2000 时按 MaxDropGold 拆堆。
    let piles: Vec<(u32, u32, Option<u32>)> = harness
        .world
        .ground_items()
        .iter()
        .filter(|ground| ground.is_gold())
        .map(|ground| (ground.gold, ground.object_id, ground.owner))
        .collect();
    assert!(!piles.is_empty(), "Scarecrow0 的 1/1 Gold 必须落地");
    let total: u32 = piles.iter().map(|(gold, ..)| *gold).sum();
    assert!(
        (1_000..3_000).contains(&total),
        "金币总量应落在 [1000,3000)，实际 {piles:?}"
    );
    assert!(
        piles.iter().all(|(gold, ..)| *gold <= 2_000),
        "单堆不得超过 MaxDropGold=2000，实际 {piles:?}"
    );
    assert!(
        piles.iter().all(|(_, _, owner)| *owner == Some(player)),
        "金币归属应是击杀者（MonsterObject.DropGold → Owner = EXPOwner）"
    );

    // 击杀者必须收到 `S.ObjectGold`（`ItemObject.Dropped()` → `Spawned()` 广播）。
    let packets = harness.packets(player);
    let object_gold: Vec<u32> = packets
        .iter()
        .filter_map(|packet| match packet {
            ServerPacket::ObjectGold(gold) => Some(gold.gold),
            _ => None,
        })
        .collect();
    assert!(
        !object_gold.is_empty(),
        "击杀者应收到 S.ObjectGold，实际包：{packets:?}"
    );
    assert!(
        object_gold.iter().all(|gold| *gold <= 2_000),
        "ObjectGold 单堆金额应 ≤ 2000，实际 {object_gold:?}"
    );

    // 踩到金币格再拾取：等价于客户端走到物品格后发 `C.PickUp`。
    let (amount, object_id, _) = piles[0];
    assert!(
        harness.world.place_player(
            player,
            MAP,
            harness.world.ground_item(object_id).unwrap().position
        ),
        "玩家应能站到金币格"
    );
    harness.clear(player);
    let report = harness.world.pickup(player).expect("捡金币");
    assert_eq!(report.gold, amount, "拾取报告的金币数应等于该堆金额");
    assert_eq!(
        harness.world.player(player).expect("玩家在线").gold,
        i64::from(amount),
        "金币应进角色金币"
    );
    assert!(
        harness.world.ground_item(object_id).is_none(),
        "拾取后金币堆应从地上移除"
    );
    assert!(
        harness.packets(player).iter().any(|packet| matches!(
            packet,
            ServerPacket::GainedGold(gained) if gained.gold == amount
        )),
        "本人应收到 S.GainedGold（PlayerObject.GainGold → S.GainedGold）"
    );
}

// ---------------------------------------------------------------------------
// A3. 反向取证：Hen 掉物品但绝不掉金币（避免「打鸡没金币」被误判为 bug）
// ---------------------------------------------------------------------------

#[test]
fn killing_a_real_hen_drops_items_but_never_gold() {
    let Some(data) = real_data() else {
        return;
    };
    let mut harness = Harness::new(0x26_02);
    harness.world.set_game_data(Arc::clone(&data));
    let player = harness.add_player(1, "杀鸡测试", 1, 0);
    let monster = spawn_real_monster(&mut harness, &data, "Hen", Point::new(12, 10));
    harness.clear(player);

    kill_monster(&mut harness, player, monster);
    let golds: Vec<u32> = harness
        .world
        .ground_items()
        .iter()
        .filter(|ground| ground.is_gold())
        .map(|ground| ground.gold)
        .collect();
    assert!(
        golds.is_empty(),
        "数据事实：Hen 的掉落表没有 Gold，杀鸡不该出现金币堆，实际 {golds:?}"
    );
    assert!(
        !harness.world.ground_items().is_empty(),
        "但 Hen 必掉 Chicken（1/1），应至少有一件物品落地"
    );
}
