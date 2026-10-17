//! 客户端 HUD / 面板状态与绘制入口。
//!
//! 这个模块把「服务器快照 + 鼠标键盘」翻译成画面和 [`UiAction`]：
//!
//! * [`hud`]：顶部状态栏、左下聊天面板（含输入行）；
//! * [`inventory`]：46 格背包、金币、选中/悬停；
//! * [`shop`]：NPC 商店货架与价格；
//! * [`dialog`]：NPC 对话页与 `<文本/@跳转键>` 按钮；
//! * [`skills`]：F3 技能列表与 1..9 施法目标选择；
//! * [`labels`]：中文/英文标签表。
//!
//! UI 不直接依赖网络层：点击产生的 [`UiAction`] 由 `app.rs` 映射成 [`crate::net::NetCommand`]。
//! 布局函数与命中测试共用同一份矩形，所以单元测试可以在不开窗口的情况下验证按钮位置。

pub mod dialog;
pub mod hud;
pub mod inventory;
pub mod labels;
pub mod shop;
pub mod skills;
pub mod text;

use std::collections::HashMap;

use mir_core::Point;
use mir_core::enums::{MirDirection, Spell};
use mir_render::{Frame, Image, MapScene};

use crate::geom::Rect;
use crate::net::{InventoryItem, WorldSnapshot};
use crate::ui::dialog::ParsedPage;
pub use crate::ui::text::UiText;
use mir_core::Language;

/// 面板绘制上下文（快照 + 文字 + 语言）。
#[derive(Clone, Copy)]
pub struct PanelContext<'a> {
    /// 世界快照。
    pub snapshot: &'a WorldSnapshot,
    /// 文字门面。
    pub text: &'a UiText,
    /// 界面语言。
    pub language: Language,
}

/// 一次点击/按键产生的界面动作。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UiAction {
    /// 点 NPC 按钮（`C.CallNPC`）；`key = "@exit"` 表示关窗。
    CallNpc {
        /// NPC 对象 ID。
        object_id: u32,
        /// 脚本页键。
        key: String,
    },
    /// 购买商店物品（`C.BuyItem`，数量 1）。
    BuyItem {
        /// 商店物品唯一编号。
        unique_id: u64,
    },
    /// 施法（`C.Magic`）。
    CastMagic {
        /// 技能。
        spell: Spell,
        /// 朝向。
        direction: MirDirection,
        /// 目标对象 ID。
        target_id: u32,
        /// 目标格。
        location: Point,
    },
    /// 重新绑定技能快捷键（`C.MagicKey`）。
    BindMagic {
        /// 技能。
        spell: Spell,
        /// 新按键。
        key: u8,
        /// 旧按键。
        old_key: u8,
    },
}

/// 界面状态：面板开关、选中项、鼠标位置、物品图标缓存。
pub struct UiState {
    /// 界面语言。
    pub language: Language,
    /// 背包面板是否打开（`I`）。
    pub show_inventory: bool,
    /// 技能面板是否打开（`F3`）。
    pub show_skills: bool,
    /// 鼠标位置（物理像素，与帧坐标一致）。
    pub mouse: Point,
    /// 背包选中的格子。
    pub selected_slot: Option<usize>,
    /// 商店选中的行。
    pub selected_goods: Option<usize>,
    /// 技能选中的行。
    pub selected_magic: Option<usize>,
    /// 当前解析后的 NPC 对话页（`None` 表示没有对话）。
    pub dialog: Option<ParsedPage>,
    /// 已同步的对话版本号。
    pub dialog_version: u64,
    /// 商店面板是否打开。
    pub shop_open: bool,
    /// 本次对话是否已被关掉（空页/`Esc`）；此时即使收到迟到的货架包也不再弹面板。
    pub shop_suppressed: bool,
    /// 已同步的货架版本号。
    pub goods_version: u64,
    /// 对话 NPC 的对象 ID。
    pub npc_object_id: Option<u32>,
    /// `items.wzl` 图标缓存（`None` 表示该图像号没有素材）。
    pub icons: HashMap<u16, Option<Image>>,
}

impl UiState {
    /// 新建界面状态。
    #[must_use]
    pub fn new(language: Language) -> Self {
        Self {
            language,
            show_inventory: false,
            show_skills: false,
            mouse: Point::new(0, 0),
            selected_slot: None,
            selected_goods: None,
            selected_magic: None,
            dialog: None,
            dialog_version: 0,
            shop_open: false,
            shop_suppressed: false,
            goods_version: 0,
            npc_object_id: None,
            icons: HashMap::new(),
        }
    }

    /// 跟随快照更新对话/商店/选中项。
    pub fn sync(&mut self, snapshot: &WorldSnapshot) {
        match &snapshot.npc_dialog {
            Some(dialog) if dialog.version != self.dialog_version => {
                self.dialog_version = dialog.version;
                if dialog.lines.is_empty() {
                    self.dialog = None;
                    self.shop_open = false;
                    self.selected_goods = None;
                    self.npc_object_id = None;
                    self.shop_suppressed = true;
                } else {
                    self.dialog = Some(ParsedPage::parse(&dialog.lines));
                    self.shop_suppressed = false;
                    if dialog.object_id.is_some() {
                        self.npc_object_id = dialog.object_id;
                    }
                }
            }
            Some(_) => {}
            None => {
                // 快照里没有对话时只收对话面板；商店由 `npc_goods` 自己的存在性决定
                // （假包注入、或服务器先发货架后发对话页时不会被误关）。
                self.dialog = None;
            }
        }
        match &snapshot.npc_goods {
            Some(goods) if goods.version != self.goods_version => {
                self.goods_version = goods.version;
                self.shop_open = !goods.is_empty() && !self.shop_suppressed;
                self.selected_goods = None;
            }
            Some(_) => {}
            None => {
                self.shop_open = false;
                self.selected_goods = None;
            }
        }
        if self
            .selected_slot
            .is_some_and(|slot| slot >= snapshot.inventory.len())
        {
            self.selected_slot = None;
        }
        if self.selected_goods.is_some_and(|index| {
            snapshot
                .npc_goods
                .as_ref()
                .is_none_or(|goods| index >= goods.list.len())
        }) {
            self.selected_goods = None;
        }
        if self
            .selected_magic
            .is_some_and(|index| index >= snapshot.magics.len())
        {
            self.selected_magic = None;
        }
    }

    /// 关掉 NPC 对话与商店（`Esc` 用；调用方负责先发 `@exit`）。
    pub fn close_npc(&mut self) {
        self.dialog = None;
        self.shop_open = false;
        self.shop_suppressed = true;
        self.selected_goods = None;
        self.npc_object_id = None;
    }

    /// 关掉所有可切换面板。
    pub fn close_panels(&mut self) {
        self.show_inventory = false;
        self.show_skills = false;
        self.close_npc();
    }

    /// 选中的背包物品。
    #[must_use]
    pub fn selected_inventory_item<'a>(
        &self,
        snapshot: &'a WorldSnapshot,
    ) -> Option<&'a InventoryItem> {
        snapshot.inventory_item(self.selected_slot?)
    }

    /// 按键施法：返回需要发送的动作（绑定 + 施法）。
    #[must_use]
    pub fn press_magic_key(&mut self, snapshot: &WorldSnapshot, key: u8) -> Vec<UiAction> {
        let Some(press) = skills::press_magic_key(snapshot, key) else {
            return Vec::new();
        };
        let mut actions = Vec::new();
        if let Some(bind) = press.bind {
            actions.push(UiAction::BindMagic {
                spell: bind.spell,
                key: bind.key,
                old_key: bind.old_key,
            });
        }
        actions.push(UiAction::CastMagic {
            spell: press.cast.spell,
            direction: press.cast.direction,
            target_id: press.cast.target_id,
            location: press.cast.location,
        });
        actions
    }

    /// 鼠标左键：按「对话 > 商店 > 背包 > 技能 > 世界 NPC」的顺序派发。
    ///
    /// `world_cell` 是鼠标对应的地图格（由 `MapScene::screen_to_cell` 算好），
    /// 点空地返回 `None`。
    pub fn on_click(
        &mut self,
        snapshot: &WorldSnapshot,
        text: &UiText,
        frame: Rect,
        world_cell: Option<Point>,
    ) -> Option<UiAction> {
        self.sync(snapshot);
        if let Some(page) = &self.dialog {
            let layout =
                dialog::dialog_layout(frame, page, |label| text.text_width(label, hud::SIZE_BODY));
            if layout.panel.contains(self.mouse) {
                if let Some(index) = layout.button_index_at(self.mouse) {
                    let button = page.buttons.get(index)?;
                    return Some(UiAction::CallNpc {
                        object_id: self.npc_object_id.unwrap_or(0),
                        key: button.key.clone(),
                    });
                }
                // 点在对话面板空白处：吃掉这次点击，避免误点到底下的面板。
                return None;
            }
        }
        if self.shop_open {
            if let Some(goods) = &snapshot.npc_goods {
                let layout = shop::shop_layout(frame, goods.list.len());
                if layout.panel.contains(self.mouse) {
                    if let Some(index) = layout.row_at(self.mouse) {
                        self.selected_goods = Some(index);
                        if let Some(entry) = goods.list.get(index) {
                            return Some(UiAction::BuyItem {
                                unique_id: entry.unique_id,
                            });
                        }
                    }
                    return None;
                }
            }
        }
        if self.show_inventory {
            let panel = inventory::inventory_panel_rect(frame);
            if panel.contains(self.mouse) {
                self.selected_slot =
                    inventory::inventory_slot_at(panel, self.mouse).or(self.selected_slot);
                return None;
            }
        }
        if self.show_skills {
            let layout = skills::skill_layout(frame, snapshot.magics.len());
            if layout.panel.contains(self.mouse) {
                if let Some(index) = layout.row_at(self.mouse) {
                    self.selected_magic = Some(index);
                    let (spell, bound_key) = snapshot
                        .magics
                        .get(index)
                        .map(|magic| (magic.spell, magic.key))?;
                    if bound_key == 0 {
                        // 未绑定：按第 N 个技能临时绑到 N 号键再施放。
                        let key = u8::try_from(index + 1).unwrap_or(skills::SKILL_KEYS);
                        let actions = self.press_magic_key(snapshot, key.min(skills::SKILL_KEYS));
                        return actions.into_iter().next();
                    }
                    return skills::cast_request(snapshot, spell).map(|cast| UiAction::CastMagic {
                        spell: cast.spell,
                        direction: cast.direction,
                        target_id: cast.target_id,
                        location: cast.location,
                    });
                }
                return None;
            }
        }
        if let Some(cell) = world_cell {
            if let Some(npc) = snapshot.npc_at(cell) {
                let in_range = snapshot
                    .self_player
                    .as_ref()
                    .is_some_and(|me| me.position.in_range(npc.position, 8));
                if in_range {
                    self.npc_object_id = Some(npc.object_id);
                    return Some(UiAction::CallNpc {
                        object_id: npc.object_id,
                        key: "@main".to_owned(),
                    });
                }
            }
        }
        None
    }
}

/// 画所有面板。
///
/// 绘制顺序：状态栏 → 聊天 → 背包 → 商店 → 技能 → 对话（最上层）。
pub fn draw(
    state: &mut UiState,
    frame: &mut Frame,
    scene: &mut MapScene,
    snapshot: &WorldSnapshot,
    text: &UiText,
    chat_input: Option<&str>,
) {
    state.sync(snapshot);
    let screen = Rect::screen(frame.width, frame.height);
    let context = PanelContext {
        snapshot,
        text,
        language: state.language,
    };
    hud::draw_status(frame, snapshot, text, state.language);
    hud::draw_chat(frame, snapshot, text, chat_input, state.language);

    if state.show_inventory {
        let panel = inventory::inventory_panel_rect(screen);
        let hover = inventory::inventory_slot_at(panel, state.mouse);
        let selected = state.selected_slot;
        let UiState { icons, .. } = state;
        let mut icon_fn = |image: u16| -> Option<Image> { load_icon(scene, icons, image) };
        inventory::draw(frame, panel, &context, selected, hover, &mut icon_fn);
    }

    if state.shop_open {
        if let Some(goods) = &snapshot.npc_goods {
            let layout = shop::shop_layout(screen, goods.list.len());
            let hover = layout.row_at(state.mouse);
            let selected = state.selected_goods;
            let UiState { icons, .. } = state;
            let mut icon_fn = |image: u16| -> Option<Image> { load_icon(scene, icons, image) };
            shop::draw(
                frame,
                &layout,
                &context,
                goods,
                selected,
                hover,
                &mut icon_fn,
            );
        }
    }

    if state.show_skills {
        let layout = skills::skill_layout(screen, snapshot.magics.len());
        skills::draw(frame, &layout, &context, state.selected_magic);
    }

    if let Some(page) = &state.dialog {
        draw_dialog(frame, screen, page, text, state.language, state.mouse);
    }
}

/// 取物品图标：先查 UI 缓存，没有就让地图场景从 `items.wzl` 解码一次。
fn load_icon(
    scene: &mut MapScene,
    icons: &mut HashMap<u16, Option<Image>>,
    image: u16,
) -> Option<Image> {
    if let Some(cached) = icons.get(&image) {
        return cached.clone();
    }
    let loaded = scene.item_icon(image);
    icons.insert(image, loaded.clone());
    loaded
}

/// 面板底板：半透明深色 + 1px 边框 + 淡阴影。
pub fn draw_panel(frame: &mut Frame, rect: Rect) {
    frame.fill_rect_alpha(
        rect.x + 2,
        rect.y + 2,
        rect.width,
        rect.height,
        [0, 0, 0, 90],
    );
    frame.fill_rect_alpha(
        rect.x,
        rect.y,
        rect.width,
        rect.height,
        hud::PANEL_BACKGROUND,
    );
    frame.fill_rect(rect.x, rect.y, rect.width, 1, hud::PANEL_BORDER);
    frame.fill_rect(rect.x, rect.bottom() - 1, rect.width, 1, hud::PANEL_BORDER);
    frame.fill_rect(rect.x, rect.y, 1, rect.height, hud::PANEL_BORDER);
    frame.fill_rect(rect.right() - 1, rect.y, 1, rect.height, hud::PANEL_BORDER);
}

/// 画 NPC 对话面板。
fn draw_dialog(
    frame: &mut Frame,
    screen: Rect,
    page: &ParsedPage,
    text: &UiText,
    language: Language,
    mouse: Point,
) {
    let layout =
        dialog::dialog_layout(screen, page, |label| text.text_width(label, hud::SIZE_BODY));
    draw_panel(frame, layout.panel);
    text.draw_clipped(
        frame,
        layout.panel.x + 12,
        layout.panel.y + 4,
        labels::Label::Dialog.text(language),
        hud::SIZE_BODY,
        hud::TITLE_COLOR,
        Rect::new(
            layout.panel.x,
            layout.panel.y,
            layout.panel.width,
            dialog::TITLE_HEIGHT,
        ),
    );
    text.draw_right(
        frame,
        layout.panel.right() - 12,
        layout.panel.y + 6,
        labels::Label::NpcHint.text(language),
        hud::SIZE_SMALL,
        hud::MUTED_COLOR,
    );
    let mut y = layout.lines_area.y;
    for line in &page.lines {
        text.draw_clipped(
            frame,
            layout.lines_area.x,
            y,
            line,
            hud::SIZE_BODY,
            hud::BODY_COLOR,
            layout.lines_area,
        );
        y = y.saturating_add(i32::try_from(dialog::LINE_HEIGHT).unwrap_or(0));
    }
    for (index, rect) in layout.buttons.iter().enumerate() {
        let hovered = rect.contains(mouse);
        let fill = if hovered {
            [92, 82, 42, 255]
        } else {
            [44, 52, 66, 255]
        };
        frame.fill_rect(rect.x, rect.y, rect.width, rect.height, fill);
        frame.fill_rect(rect.x, rect.y, rect.width, 1, [140, 156, 180, 255]);
        frame.fill_rect(rect.x, rect.bottom() - 1, rect.width, 1, [36, 44, 56, 255]);
        let label = page
            .buttons
            .get(index)
            .map(|button| {
                if button.label.is_empty() {
                    dialog::close_button_label(language).to_owned()
                } else {
                    button.label.clone()
                }
            })
            .unwrap_or_default();
        text.draw_center(
            frame,
            *rect,
            &label,
            hud::SIZE_SMALL,
            if hovered {
                [255, 255, 220, 255]
            } else {
                [255, 240, 180, 255]
            },
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::net::{
        ChatLine, InventoryItem, ItemInfoView, MagicView, MapSnapshot, NpcDialog, SelfPlayer,
        ShopEntry, ShopGoods, WorldEntity,
    };
    use mir_core::Language;
    use mir_core::enums::{ChatType, MirClass, MirDirection, MirGender, Spell};
    use mir_render::EntityKind;

    fn text() -> UiText {
        UiText::fallback()
    }

    fn item(unique_id: u64, item_index: i32, count: u16) -> InventoryItem {
        InventoryItem {
            unique_id,
            item_index,
            count,
            current_dura: 100,
            max_dura: 100,
        }
    }

    fn snapshot() -> WorldSnapshot {
        let mut snapshot = WorldSnapshot {
            self_player: Some(SelfPlayer {
                object_id: 7,
                name: "测试者".to_owned(),
                position: Point::new(10, 10),
                direction: MirDirection::Down,
                hp: 30,
                mp: 20,
                hp_max: 40,
                mp_max: 40,
                level: 5,
                class: MirClass::Warrior,
                gender: MirGender::Male,
                hair: 0,
                gold: 4_321,
                dead: false,
            }),
            map: Some(MapSnapshot {
                title: "比奇省".to_owned(),
                file_name: "0".to_owned(),
                ..MapSnapshot::default()
            }),
            inventory: vec![Some(item(11, 1, 2)), None],
            magics: vec![MagicView {
                name: "治愈术".to_owned(),
                spell: Spell::Healing,
                level: 1,
                key: 1,
                base_cost: 5,
                icon: 0,
                range: 3,
            }],
            chat: vec![ChatLine {
                speaker: Some("甲".to_owned()),
                text: "你好".to_owned(),
                kind: ChatType::Normal,
            }],
            ..WorldSnapshot::default()
        };
        snapshot.item_infos.insert(
            1,
            ItemInfoView {
                index: 1,
                name: "金创药".to_owned(),
                image: 5,
                price: 50,
                stack_size: 100,
            },
        );
        snapshot
    }

    fn with_npc(mut snapshot: WorldSnapshot) -> WorldSnapshot {
        snapshot.entities.insert(
            42,
            WorldEntity {
                object_id: 42,
                kind: EntityKind::Npc,
                name: "商人".to_owned(),
                position: Point::new(11, 11),
                direction: MirDirection::Down,
                image: 1,
                dead: false,
                hp_percent: None,
            },
        );
        snapshot
    }

    fn with_dialog(mut snapshot: WorldSnapshot) -> WorldSnapshot {
        snapshot.npc_dialog_version = 1;
        snapshot.npc_dialog = Some(NpcDialog {
            lines: vec![
                "欢迎光临".to_owned(),
                "<买东西/@BUY>".to_owned(),
                "<离开/@exit>".to_owned(),
            ],
            object_id: Some(42),
            version: 1,
        });
        snapshot
    }

    fn with_shop(mut snapshot: WorldSnapshot) -> WorldSnapshot {
        snapshot.npc_goods_version = 1;
        snapshot.npc_goods = Some(ShopGoods {
            list: vec![ShopEntry {
                unique_id: 99,
                item_index: 1,
                name: "金创药".to_owned(),
                image: 5,
                price: 50,
                count: 1,
                current_dura: 0,
                max_dura: 0,
            }],
            rate: 1.0,
            version: 1,
        });
        snapshot
    }

    #[test]
    fn clicking_an_inventory_slot_selects_it_without_network_action() {
        let snapshot = snapshot();
        let mut state = UiState::new(Language::ZhCn);
        state.show_inventory = true;
        state.sync(&snapshot);
        let frame = Rect::screen(800, 600);
        let panel = inventory::inventory_panel_rect(frame);
        let slot = inventory::inventory_slot_rect(panel, 0).unwrap();
        state.mouse = slot.center();
        let action = state.on_click(&snapshot, &text(), frame, None);
        assert!(action.is_none(), "选格子不应发网络包");
        assert_eq!(state.selected_slot, Some(0));
        assert_eq!(
            state
                .selected_inventory_item(&snapshot)
                .map(|item| item.unique_id),
            Some(11)
        );
    }

    #[test]
    fn clicking_a_dialog_button_returns_its_script_key() {
        let snapshot = with_npc(with_dialog(snapshot()));
        let mut state = UiState::new(Language::ZhCn);
        state.sync(&snapshot);
        assert!(state.dialog.is_some());
        let frame = Rect::screen(800, 600);
        let page = state.dialog.as_ref().unwrap();
        let layout = dialog::dialog_layout(frame, page, |label| text().text_width(label, 14));
        state.mouse = layout.buttons[0].center();
        let action = state
            .on_click(&snapshot, &text(), frame, None)
            .expect("按钮应产生动作");
        assert_eq!(
            action,
            UiAction::CallNpc {
                object_id: 42,
                key: "@BUY".to_owned()
            }
        );
    }

    #[test]
    fn clicking_a_shop_row_buys_one_item() {
        let snapshot = with_npc(with_shop(snapshot()));
        let mut state = UiState::new(Language::ZhCn);
        state.sync(&snapshot);
        assert!(state.shop_open);
        let frame = Rect::screen(800, 600);
        let layout = shop::shop_layout(frame, 1);
        state.mouse = layout.rows[0].center();
        let action = state
            .on_click(&snapshot, &text(), frame, None)
            .expect("点货架应产生动作");
        assert_eq!(
            action,
            UiAction::BuyItem { unique_id: 99 },
            "买的是商店物品唯一编号"
        );
        assert_eq!(state.selected_goods, Some(0));
    }

    #[test]
    fn clicking_an_npc_in_the_world_opens_the_dialog() {
        let snapshot = with_npc(snapshot());
        let mut state = UiState::new(Language::ZhCn);
        state.sync(&snapshot);
        let action = state
            .on_click(
                &snapshot,
                &text(),
                Rect::screen(800, 600),
                Some(Point::new(11, 11)),
            )
            .expect("点 NPC 应发起对话");
        assert_eq!(
            action,
            UiAction::CallNpc {
                object_id: 42,
                key: "@main".to_owned()
            }
        );
        assert_eq!(state.npc_object_id, Some(42));
    }

    #[test]
    fn closing_npc_hides_dialog_and_shop() {
        let snapshot = with_npc(with_shop(with_dialog(snapshot())));
        let mut state = UiState::new(Language::ZhCn);
        state.sync(&snapshot);
        assert!(state.dialog.is_some());
        assert!(state.shop_open);
        state.close_npc();
        assert!(state.dialog.is_none());
        assert!(!state.shop_open);
    }

    #[test]
    fn empty_dialog_page_closes_everything() {
        let mut snapshot = with_shop(with_dialog(snapshot()));
        snapshot.npc_dialog_version = 2;
        snapshot.npc_dialog = Some(NpcDialog {
            lines: Vec::new(),
            object_id: Some(42),
            version: 2,
        });
        let mut state = UiState::new(Language::ZhCn);
        state.sync(&snapshot);
        assert!(state.dialog.is_none());
        assert!(!state.shop_open);
        assert_eq!(state.dialog_version, 2);
    }

    #[test]
    fn pressing_a_number_key_casts_and_binds_unbound_magic() {
        let base = snapshot();
        let mut state = UiState::new(Language::ZhCn);
        let actions = state.press_magic_key(&base, 1);
        assert_eq!(actions.len(), 1, "已绑定键不产生绑定动作");
        assert!(matches!(actions[0], UiAction::CastMagic { .. }));

        let mut unbound = base;
        unbound.magics[0].key = 0;
        let actions = state.press_magic_key(&unbound, 1);
        assert_eq!(actions.len(), 2);
        assert!(matches!(actions[0], UiAction::BindMagic { key: 1, .. }));
        assert!(matches!(actions[1], UiAction::CastMagic { .. }));
    }

    #[test]
    fn selected_slot_is_cleared_when_the_inventory_shrinks() {
        let mut snapshot = snapshot();
        let mut state = UiState::new(Language::ZhCn);
        state.selected_slot = Some(1);
        state.sync(&snapshot);
        snapshot.inventory.clear();
        state.sync(&snapshot);
        assert_eq!(state.selected_slot, None);
    }

    #[test]
    fn clicking_outside_panels_does_nothing() {
        let snapshot = with_dialog(with_npc(snapshot()));
        let mut state = UiState::new(Language::ZhCn);
        state.sync(&snapshot);
        state.mouse = Point::new(400, 20);
        assert!(
            state
                .on_click(&snapshot, &text(), Rect::screen(800, 600), None)
                .is_none()
        );
    }
}
