//! t34：`drop.QuestRequired`（Crystal `DropInfo.QuestRequired`，掉落表行尾的 `Q`）。
//!
//! Crystal `MonsterObject.Drop()`（`MonsterObject.cs:1119-1142`）的顺序是：
//! 1. 生成掉落实例；2. `EXPOwner.CheckGroupQuestItem(item)`（`:1130-1138`）截胡进任务物品栏；
//! 3. 截不到才 `if (drop.QuestRequired) continue;`（`:1140`，`Q` 物品绝不落地）。
//!
//! 本作把 `Q` 标记随 `DropReward::quest_required` 一起带出（与 `items` 一一对应），
//! `items.rs::drop_monster_loot` 先截胡、截不到再按标记跳过落地——与 Crystal 顺序一致。
//! 截胡本身的行为由 `tests/quests.rs` 的任务物品用例覆盖。

use mir_core::MirRng;
use mir_db::drops::{DropKind, DropTable};

fn rng() -> MirRng {
    MirRng::from_seed(0x5EED)
}

#[test]
fn quest_required_flag_is_parsed_from_the_q_suffix() {
    let table = DropTable::parse("1/1 QuestEgg Q\n1/1 Egg\n");
    assert_eq!(table.drops.len(), 2);
    assert!(table.drops[0].quest_required, "行尾 `Q` → QuestRequired");
    assert_eq!(table.drops[0].kind, DropKind::Item("QuestEgg".to_owned()));
    assert!(!table.drops[1].quest_required);
}

#[test]
fn roll_keeps_quest_required_aligned_with_items() {
    // `1/1` 必掉；Q 物品仍进入奖励列表，但带 `true` 标记（由 items.rs 决定不落地）。
    let table = DropTable::parse("1/1 QuestEgg Q\n1/1 Egg\n");
    let reward = table.roll(&mut rng(), 1);
    assert_eq!(reward.items, vec!["QuestEgg".to_owned(), "Egg".to_owned()]);
    assert_eq!(reward.quest_required, vec![true, false]);
}

#[test]
fn group_drop_children_keep_their_own_quest_required_flags() {
    // 组内两条：Q 物品在前、普通物品在后（`GROUP` 只保留命中的子规则）。
    let table = DropTable::parse("1/1 GROUP\n1/1 QuestEgg Q\n1/1 Egg\n");
    let reward = table.roll(&mut rng(), 1);
    assert_eq!(reward.items.len(), reward.quest_required.len());
    for (name, flag) in reward.items.iter().zip(&reward.quest_required) {
        assert_eq!(*flag, name == "QuestEgg", "{name} 的 Q 标记错位");
    }
}

#[test]
fn empty_or_gold_only_rolls_keep_the_flag_array_aligned() {
    let empty = DropTable::parse("").roll(&mut rng(), 1);
    assert!(empty.items.is_empty());
    assert!(
        empty.quest_required.is_empty(),
        "空掉落时标记数组也必须为空（与 items 等长）"
    );

    let gold = DropTable::parse("1/1 Gold 100\n").roll(&mut rng(), 1);
    assert!(gold.items.is_empty());
    assert!(
        gold.quest_required.is_empty(),
        "只有金币时标记数组也必须为空（与 items 等长）"
    );
    assert!(gold.gold > 0);
}

#[test]
fn quest_required_only_table_still_reports_the_item_for_interception() {
    let table = DropTable::parse("1/1 QuestEgg Q\n");
    let reward = table.roll(&mut rng(), 1);
    assert_eq!(reward.items, vec!["QuestEgg".to_owned()]);
    assert_eq!(
        reward.quest_required,
        vec![true],
        "Q 物品必须先进入奖励列表，才有机会被 CheckGroupQuestItem 截胡"
    );
}
