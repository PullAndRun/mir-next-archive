//! 装备纸娃娃面板（`E`）：14 个装备槽 + 属性汇总。
//!
//! 槽位顺序与 Crystal `EquipmentSlot`（`Shared/Enums.cs`）一致：
//! 武器 / 衣服 / 头盔 / 火把 / 项链 / 左手镯 / 右手镯 / 左戒指 / 右戒指 /
//! 护身符 / 腰带 / 靴子 / 宝石 / 坐骑。
//!
//! 数据来自 `S.UserInformation.equipment`（以及之后 `S.EquipItem` / `S.RemoveItem` 的
//! 增量更新），图标与名称来自 `S.NewItemInfo`。汇总属性直接累加每件装备的
//! `ItemInfo.stats`（攻/防/魔防/魔法/道术/准确/敏捷/攻速/幸运）。
//!
//! 布局（[`equipment_layout`]）是纯函数，绘制与命中测试共用；无窗口测试可以只验证矩形。

use mir_core::enums::{EquipmentSlot, ItemType, Stat};
use mir_core::{Language, Point};
use mir_render::{Frame, Image, scale_to_fit};

use crate::geom::Rect;
use crate::net::{EQUIPMENT_SLOTS, InventoryItem, WorldSnapshot};
use crate::ui::PanelContext;
use crate::ui::hud::{BODY_COLOR, GOLD_COLOR, MUTED_COLOR, SIZE_BODY, SIZE_SMALL, TITLE_COLOR};
use crate::ui::labels::{self, Label};

/// 每个槽位的图标方框边长。
pub const SLOT_SIZE: u32 = 28;
/// 每行高度（图标 + 间距）。
pub const ROW_HEIGHT: u32 = 30;
/// 图标与文字之间的间距。
pub const ICON_TEXT_GAP: u32 = 4;
/// 名称文字宽度。
pub const LABEL_WIDTH: u32 = 92;
/// 两列之间的间距。
pub const COLUMN_GAP: u32 = 8;
/// 面板内边距。
pub const PADDING: u32 = 8;
/// 标题高度。
pub const TITLE_HEIGHT: u32 = 24;
/// 汇总行高度。
pub const SUMMARY_LINE_HEIGHT: u32 = 15;
/// 汇总行数：DC/AC/MAC/准确/敏捷（魔法/道术按需追加）。
pub const SUMMARY_LINES: u32 = 5;
/// 提示行高度。
pub const HINT_HEIGHT: u32 = 14;
/// 图标最大显示边长（留出边框）。
pub const MAX_ICON_SIZE: u32 = SLOT_SIZE - 4;

/// 槽位列数。
pub const COLUMNS: usize = 2;
/// 槽位行数。
pub const ROWS: usize = EQUIPMENT_SLOTS / COLUMNS;

/// 装备面板布局。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EquipmentLayout {
    /// 面板矩形。
    pub panel: Rect,
    /// 14 个槽位的图标方框。
    pub slots: [Rect; EQUIPMENT_SLOTS],
    /// 每个槽位名/物品名的文字区。
    pub labels: [Rect; EQUIPMENT_SLOTS],
    /// 属性汇总区域。
    pub summary: Rect,
    /// 底部提示区域。
    pub hint: Rect,
}

impl EquipmentLayout {
    /// 命中测试：鼠标在哪个装备槽上。
    #[must_use]
    pub fn slot_at(&self, point: Point) -> Option<usize> {
        self.slots.iter().position(|rect| rect.contains(point))
    }
}

/// 面板宽度。
#[must_use]
pub fn panel_width() -> u32 {
    PADDING * 2
        + COLUMNS as u32 * (SLOT_SIZE + ICON_TEXT_GAP + LABEL_WIDTH)
        + (COLUMNS as u32 - 1) * COLUMN_GAP
}

/// 面板高度。
#[must_use]
pub fn panel_height() -> u32 {
    TITLE_HEIGHT
        + ROWS as u32 * ROW_HEIGHT
        + (SUMMARY_LINES + 1) * SUMMARY_LINE_HEIGHT
        + 2
        + HINT_HEIGHT
        + PADDING
        + 4
}

/// 计算装备面板布局（左上角，避开状态栏与聊天面板）。
#[must_use]
pub fn equipment_layout(frame: Rect) -> EquipmentLayout {
    let width = panel_width().min(frame.width.saturating_sub(16)).max(160);
    let height = panel_height();
    // 固定左上角：面板高度 352，800×600 下不会压到聊天面板；更小的窗口由裁剪兜底。
    let panel = Rect::new(8, 60, width, height);
    let mut slots = [Rect::default(); EQUIPMENT_SLOTS];
    let mut labels = [Rect::default(); EQUIPMENT_SLOTS];
    for index in 0..EQUIPMENT_SLOTS {
        let column = (index % COLUMNS) as u32;
        let row = (index / COLUMNS) as u32;
        let x = panel.x
            + i32::try_from(
                PADDING + column * (SLOT_SIZE + ICON_TEXT_GAP + LABEL_WIDTH + COLUMN_GAP),
            )
            .unwrap_or(0);
        let y = panel.y
            + i32::try_from(TITLE_HEIGHT + row * ROW_HEIGHT + (ROW_HEIGHT - SLOT_SIZE) / 2)
                .unwrap_or(0);
        slots[index] = Rect::new(x, y, SLOT_SIZE, SLOT_SIZE);
        labels[index] = Rect::new(
            x + i32::try_from(SLOT_SIZE + ICON_TEXT_GAP).unwrap_or(0),
            y,
            LABEL_WIDTH,
            SLOT_SIZE,
        );
    }
    let summary_height = (SUMMARY_LINES + 1) * SUMMARY_LINE_HEIGHT + 2;
    let summary = Rect::new(
        panel.x + i32::try_from(PADDING).unwrap_or(0),
        panel.y + i32::try_from(TITLE_HEIGHT + ROWS as u32 * ROW_HEIGHT).unwrap_or(0),
        panel.width.saturating_sub(PADDING * 2),
        summary_height,
    );
    let hint = Rect::new(
        panel.x + i32::try_from(PADDING).unwrap_or(0),
        panel.bottom() - i32::try_from(HINT_HEIGHT + 4).unwrap_or(0),
        panel.width.saturating_sub(PADDING * 2),
        HINT_HEIGHT,
    );
    EquipmentLayout {
        panel,
        slots,
        labels,
        summary,
        hint,
    }
}

/// 装备属性汇总（`(min, max)` 对 + 单值）。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct EquipmentTotals {
    /// 攻击区间（`MinDC..MaxDC`）。
    pub dc: (i32, i32),
    /// 防御区间（`MinAC..MaxAC`）。
    pub ac: (i32, i32),
    /// 魔防区间（`MinMAC..MaxMAC`）。
    pub mac: (i32, i32),
    /// 魔法区间（`MinMC..MaxMC`）。
    pub mc: (i32, i32),
    /// 道术区间（`MinSC..MaxSC`）。
    pub sc: (i32, i32),
    /// 准确。
    pub accuracy: i32,
    /// 敏捷。
    pub agility: i32,
    /// 攻击速度。
    pub attack_speed: i32,
    /// 幸运。
    pub luck: i32,
}

impl EquipmentTotals {
    /// 把一件装备的属性累加进来。
    pub fn add_item(&mut self, snapshot: &WorldSnapshot, item: &InventoryItem) {
        let Some(info) = snapshot.item_info(item.item_index) else {
            return;
        };
        self.dc.0 += info.stat(Stat::MinDC);
        self.dc.1 += info.stat(Stat::MaxDC);
        self.ac.0 += info.stat(Stat::MinAC);
        self.ac.1 += info.stat(Stat::MaxAC);
        self.mac.0 += info.stat(Stat::MinMAC);
        self.mac.1 += info.stat(Stat::MaxMAC);
        self.mc.0 += info.stat(Stat::MinMC);
        self.mc.1 += info.stat(Stat::MaxMC);
        self.sc.0 += info.stat(Stat::MinSC);
        self.sc.1 += info.stat(Stat::MaxSC);
        self.accuracy += info.stat(Stat::Accuracy);
        self.agility += info.stat(Stat::Agility);
        self.attack_speed += info.stat(Stat::AttackSpeed);
        self.luck += info.stat(Stat::Luck);
    }
}

/// 汇总当前所有已装备物品的属性。
#[must_use]
pub fn totals(snapshot: &WorldSnapshot) -> EquipmentTotals {
    let mut totals = EquipmentTotals::default();
    for item in snapshot.equipment.iter().flatten() {
        totals.add_item(snapshot, item);
    }
    totals
}

/// 区间文案：`12-25`；上下限相同时只显示一个数。
#[must_use]
pub fn range_text(range: (i32, i32)) -> String {
    if range.0 == range.1 {
        range.0.to_string()
    } else {
        format!("{}-{}", range.0, range.1)
    }
}

/// 汇总行文案（`攻击 DC 12-25`、`准确 +3`…），每行一条。
#[must_use]
pub fn summary_lines(totals: &EquipmentTotals, language: Language) -> Vec<String> {
    let mut lines = vec![
        format!("{} {}", Label::Attack.text(language), range_text(totals.dc)),
        format!(
            "{} {}",
            Label::Defence.text(language),
            range_text(totals.ac)
        ),
        format!(
            "{} {}",
            Label::MagicDefence.text(language),
            range_text(totals.mac)
        ),
        format!("{} +{}", Label::Accuracy.text(language), totals.accuracy),
        format!("{} +{}", Label::Agility.text(language), totals.agility),
    ];
    if totals.mc.0 != 0 || totals.mc.1 != 0 {
        lines.push(format!(
            "{} {}",
            Label::MagicAttack.text(language),
            range_text(totals.mc)
        ));
    }
    if totals.sc.0 != 0 || totals.sc.1 != 0 {
        lines.push(format!(
            "{} {}",
            Label::Taoism.text(language),
            range_text(totals.sc)
        ));
    }
    if totals.attack_speed != 0 {
        lines.push(format!(
            "{} +{}",
            Label::AttackSpeed.text(language),
            totals.attack_speed
        ));
    }
    if totals.luck != 0 {
        lines.push(format!("{} +{}", Label::Luck.text(language), totals.luck));
    }
    lines
}

/// 物品类型对应的装备槽；戒指/手镯优先放空槽，其余返回 `None`。
#[must_use]
pub fn equipment_slot_for_item(snapshot: &WorldSnapshot, item_type: ItemType) -> Option<usize> {
    let paired = |left: usize, right: usize| {
        if snapshot.equipment_item(left).is_none() {
            Some(left)
        } else if snapshot.equipment_item(right).is_none() {
            Some(right)
        } else {
            Some(left)
        }
    };
    match item_type {
        ItemType::Weapon => Some(EquipmentSlot::Weapon as usize),
        ItemType::Armour => Some(EquipmentSlot::Armour as usize),
        ItemType::Helmet => Some(EquipmentSlot::Helmet as usize),
        ItemType::Torch => Some(EquipmentSlot::Torch as usize),
        ItemType::Necklace => Some(EquipmentSlot::Necklace as usize),
        ItemType::Bracelet => paired(
            EquipmentSlot::BraceletL as usize,
            EquipmentSlot::BraceletR as usize,
        ),
        ItemType::Ring => paired(EquipmentSlot::RingL as usize, EquipmentSlot::RingR as usize),
        ItemType::Amulet => Some(EquipmentSlot::Amulet as usize),
        ItemType::Belt => Some(EquipmentSlot::Belt as usize),
        ItemType::Boots => Some(EquipmentSlot::Boots as usize),
        ItemType::Stone => Some(EquipmentSlot::Stone as usize),
        ItemType::Mount => Some(EquipmentSlot::Mount as usize),
        _ => None,
    }
}

/// 槽位名称（`labels::equipment_slot_name` 的下标版本）。
#[must_use]
pub fn slot_name(slot: usize, language: Language) -> &'static str {
    EquipmentSlot::ALL
        .get(slot)
        .map_or("?", |slot| labels::equipment_slot_name(*slot, language))
}

/// 画装备面板。
pub fn draw(
    frame: &mut Frame,
    layout: &EquipmentLayout,
    context: &PanelContext<'_>,
    hover: Option<usize>,
    icon: &mut dyn FnMut(u16) -> Option<Image>,
) {
    let snapshot = context.snapshot;
    let text = context.text;
    let language = context.language;
    crate::ui::draw_panel(frame, layout.panel);
    text.draw_clipped(
        frame,
        layout.panel.x + 10,
        layout.panel.y + 4,
        &format!("{} (E)", Label::Equipment.text(language)),
        SIZE_BODY,
        TITLE_COLOR,
        Rect::new(
            layout.panel.x,
            layout.panel.y,
            layout.panel.width,
            TITLE_HEIGHT,
        ),
    );

    for index in 0..EQUIPMENT_SLOTS {
        let slot = layout.slots[index];
        let hovered = hover == Some(index);
        let border = if hovered {
            [168, 208, 255, 255]
        } else {
            [62, 72, 88, 255]
        };
        frame.fill_rect(slot.x, slot.y, slot.width, slot.height, [0, 0, 0, 255]);
        frame.fill_rect(
            slot.x + 1,
            slot.y + 1,
            slot.width.saturating_sub(2),
            slot.height.saturating_sub(2),
            [34, 40, 50, 235],
        );
        draw_border(frame, slot, border);
        match snapshot.equipment_item(index) {
            Some(item) => {
                let image = snapshot.item_image(item.item_index);
                let loaded = image.and_then(&mut *icon);
                if let Some(image) = loaded {
                    let image = scale_to_fit(&image, MAX_ICON_SIZE, MAX_ICON_SIZE);
                    let x = slot.x
                        + (i32::try_from(slot.width).unwrap_or(0) - i32::from(image.info.width))
                            / 2;
                    let y = slot.y
                        + (i32::try_from(slot.height).unwrap_or(0) - i32::from(image.info.height))
                            / 2;
                    frame.blit(&image, x, y);
                } else {
                    text.draw_clipped(
                        frame,
                        slot.x + 3,
                        slot.y + 6,
                        &item.item_index.to_string(),
                        SIZE_SMALL,
                        MUTED_COLOR,
                        slot,
                    );
                }
                let name = snapshot.item_info(item.item_index).map_or_else(
                    || labels::unknown_item(language, item.item_index),
                    |info| info.name.clone(),
                );
                text.draw_clipped(
                    frame,
                    layout.labels[index].x,
                    layout.labels[index].y + 7,
                    &name,
                    SIZE_SMALL,
                    BODY_COLOR,
                    layout.labels[index],
                );
            }
            None => {
                text.draw_clipped(
                    frame,
                    layout.labels[index].x,
                    layout.labels[index].y + 7,
                    slot_name(index, language),
                    SIZE_SMALL,
                    MUTED_COLOR,
                    layout.labels[index],
                );
            }
        }
    }

    text.draw_clipped(
        frame,
        layout.summary.x,
        layout.summary.y,
        Label::Totals.text(language),
        SIZE_SMALL,
        TITLE_COLOR,
        layout.summary,
    );
    let totals = totals(snapshot);
    let mut y = layout.summary.y + i32::try_from(SUMMARY_LINE_HEIGHT).unwrap_or(0);
    for line in summary_lines(&totals, language)
        .iter()
        .take(SUMMARY_LINES as usize)
    {
        text.draw_clipped(
            frame,
            layout.summary.x,
            y,
            line,
            SIZE_SMALL,
            if line.starts_with(Label::Attack.text(language)) {
                GOLD_COLOR
            } else {
                BODY_COLOR
            },
            layout.summary,
        );
        y = y.saturating_add(i32::try_from(SUMMARY_LINE_HEIGHT).unwrap_or(0));
    }

    text.draw_clipped(
        frame,
        layout.hint.x,
        layout.hint.y,
        Label::InventoryTooltipHint.text(language),
        SIZE_SMALL,
        MUTED_COLOR,
        layout.hint,
    );
}

/// 画一个方框的四条边。
fn draw_border(frame: &mut Frame, rect: Rect, color: [u8; 4]) {
    frame.fill_rect(rect.x, rect.y, rect.width, 1, color);
    frame.fill_rect(rect.x, rect.bottom() - 1, rect.width, 1, color);
    frame.fill_rect(rect.x, rect.y, 1, rect.height, color);
    frame.fill_rect(rect.right() - 1, rect.y, 1, rect.height, color);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::net::{InventoryItem, ItemInfoView};
    use mir_core::Point;

    fn snapshot_with_equipment() -> WorldSnapshot {
        let mut snapshot = WorldSnapshot::default();
        snapshot.item_infos.insert(
            10,
            ItemInfoView {
                index: 10,
                name: "炼狱".to_owned(),
                item_type: ItemType::Weapon,
                stats: [(Stat::MinDC, 5), (Stat::MaxDC, 12), (Stat::Accuracy, 2)]
                    .into_iter()
                    .collect(),
                ..ItemInfoView::default()
            },
        );
        snapshot.item_infos.insert(
            11,
            ItemInfoView {
                index: 11,
                name: "重盔甲".to_owned(),
                item_type: ItemType::Armour,
                stats: [(Stat::MinAC, 3), (Stat::MaxAC, 7), (Stat::Agility, 1)]
                    .into_iter()
                    .collect(),
                ..ItemInfoView::default()
            },
        );
        snapshot.equipment = vec![None; EQUIPMENT_SLOTS];
        snapshot.equipment[EquipmentSlot::Weapon as usize] = Some(InventoryItem {
            unique_id: 1,
            item_index: 10,
            count: 1,
            current_dura: 5_000,
            max_dura: 5_000,
        });
        snapshot.equipment[EquipmentSlot::Armour as usize] = Some(InventoryItem {
            unique_id: 2,
            item_index: 11,
            count: 1,
            current_dura: 4_000,
            max_dura: 4_000,
        });
        snapshot
    }

    #[test]
    fn layout_has_fourteen_slots_inside_the_panel() {
        let layout = equipment_layout(Rect::screen(800, 600));
        assert_eq!(layout.slots.len(), EQUIPMENT_SLOTS);
        for (index, slot) in layout.slots.iter().enumerate() {
            assert!(layout.panel.contains(slot.center()), "槽 {index} 越界");
            assert_eq!(layout.slot_at(slot.center()), Some(index));
            assert!(
                layout.panel.contains(layout.labels[index].center()),
                "名称 {index} 越界"
            );
        }
        assert_eq!(layout.slot_at(Point::new(0, 0)), None);
        assert!(layout.panel.bottom() <= 600);
    }

    #[test]
    fn totals_sum_every_equipped_item() {
        let snapshot = snapshot_with_equipment();
        let totals = totals(&snapshot);
        assert_eq!(totals.dc, (5, 12));
        assert_eq!(totals.ac, (3, 7));
        assert_eq!(totals.accuracy, 2);
        assert_eq!(totals.agility, 1);
        let lines = summary_lines(&totals, Language::ZhCn);
        assert!(lines[0].contains("5-12"), "{lines:?}");
        assert!(lines.iter().any(|line| line.contains("准确 +2")));
        assert!(summary_lines(&totals, Language::En)[0].starts_with("DC"));
    }

    #[test]
    fn item_types_map_to_the_right_slots_and_paired_slots_pick_empty() {
        let mut snapshot = snapshot_with_equipment();
        assert_eq!(
            equipment_slot_for_item(&snapshot, ItemType::Weapon),
            Some(0)
        );
        assert_eq!(
            equipment_slot_for_item(&snapshot, ItemType::Ring),
            Some(EquipmentSlot::RingL as usize)
        );
        snapshot.equipment[EquipmentSlot::RingL as usize] = Some(InventoryItem {
            unique_id: 3,
            item_index: 10,
            count: 1,
            current_dura: 0,
            max_dura: 0,
        });
        assert_eq!(
            equipment_slot_for_item(&snapshot, ItemType::Ring),
            Some(EquipmentSlot::RingR as usize),
            "左戒已占时放右戒"
        );
        assert_eq!(equipment_slot_for_item(&snapshot, ItemType::Potion), None);
        assert_eq!(slot_name(0, Language::ZhCn), "武器");
        assert_eq!(slot_name(1, Language::En), "Armour");
    }
}
