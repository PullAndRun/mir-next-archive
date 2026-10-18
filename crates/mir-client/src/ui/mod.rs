//! 客户端 HUD / 面板状态与绘制入口。
//!
//! 这个模块把「服务器快照 + 鼠标键盘」翻译成画面和 [`UiAction`]：
//!
//! * [`hud`]：顶部状态栏、左下聊天面板（含输入行）、增益栏数据来自 [`buffs`]；
//! * [`inventory`]：46 格背包、金币、选中/悬停、拖拽幽灵与右键菜单；
//! * [`equipment`]：`E` 打开的纸娃娃面板（14 个装备槽 + 属性汇总）；
//! * [`tooltip`]：鼠标悬停物品的提示框（跟随鼠标并在屏幕内收边）；
//! * [`worldtext`]：实体头顶名字（描边）与伤害飘字；
//! * [`shop`]：NPC 商店货架与价格（支持滚轮/上下键滚动）；
//! * [`dialog`]：NPC 对话页与 `<文本/@跳转键>` 按钮；
//! * [`skills`]：F3 技能列表与 1..9 施法目标选择；
//! * [`labels`]：中文/英文标签表。
//!
//! UI 不直接依赖网络层：点击产生的 [`UiAction`] 由 `app.rs` 映射成
//! [`crate::net::NetCommand`]。布局函数与命中测试共用同一份矩形，所以单元测试可以在
//! 不开窗口的情况下验证按钮位置。
//!
//! # 可注入时钟
//!
//! 伤害飘字与增益倒计时都用 [`UiState::time_millis`]（Unix 毫秒）。窗口模式每帧用
//! [`UiState::set_time`] 写入真实时间；无头测试直接写任意时刻，于是可以在不 sleep 的
//! 情况下断言「500 ms 内存在、2 s 后消失」。

pub mod buffs;
pub mod dialog;
pub mod equipment;
pub mod hud;
pub mod inventory;
pub mod labels;
pub mod shop;
pub mod skills;
pub mod text;
pub mod tooltip;
pub mod worldtext;

use std::collections::HashMap;

use mir_core::Point;
use mir_core::enums::{MirDirection, MirGridType, Spell};
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
    /// 使用背包物品（`C.UseItem`；双击或右键菜单「使用」）。
    UseItem {
        /// 物品唯一编号。
        unique_id: u64,
    },
    /// 丢弃背包物品（`C.DropItem`）。
    DropItem {
        /// 物品唯一编号。
        unique_id: u64,
        /// 数量。
        count: u16,
    },
    /// 拖拽物品（`C.MoveItem`）。
    ///
    /// `grid` 是拖拽**起点**网格、`to_grid` 是落点网格：
    /// * 背包内交换：`Inventory → Inventory`；
    /// * 背包 → 装备槽（穿戴）：`Inventory → Equipment`；
    /// * 装备槽 → 背包（脱下）：`Equipment → Inventory`。
    MoveItem {
        /// 起点网格。
        grid: MirGridType,
        /// 起点格。
        from: i32,
        /// 落点格。
        to: i32,
        /// 落点网格。
        to_grid: MirGridType,
    },
    /// 点地走路：`step = true` 走一步，`false` 只转身（见 [`walk_intent`]）。
    WorldMove {
        /// 方向。
        direction: MirDirection,
        /// 是否直接走（否则转身）。
        step: bool,
    },
}

impl UiAction {
    /// 纯数据动作 → 网络命令。
    ///
    /// `app.rs` 与无头测试共用这一份映射；NPC 对话、施法、绑定等需要额外本地状态
    /// （关闭面板、记录新键位、按当前朝向决定走/转）的动作返回 `None`，由 `app.rs` 处理。
    #[must_use]
    pub fn net_command(&self) -> Option<crate::net::NetCommand> {
        use crate::net::NetCommand;
        match self {
            Self::UseItem { unique_id } => Some(NetCommand::UseItem {
                unique_id: *unique_id,
            }),
            Self::DropItem { unique_id, count } => Some(NetCommand::DropItem {
                unique_id: *unique_id,
                count: *count,
            }),
            Self::MoveItem {
                grid,
                from,
                to,
                to_grid,
            } => Some(NetCommand::MoveItem {
                grid: *grid,
                from: *from,
                to: *to,
                to_grid: *to_grid,
            }),
            Self::BuyItem { unique_id } => Some(NetCommand::BuyItem {
                unique_id: *unique_id,
                count: 1,
            }),
            _ => None,
        }
    }
}

/// 拖拽起点（背包格或装备槽）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DragSource {
    /// 背包格。
    Inventory(usize),
    /// 装备槽。
    Equipment(usize),
}

impl DragSource {
    /// 起点网格。
    #[must_use]
    pub const fn grid(self) -> MirGridType {
        match self {
            Self::Inventory(_) => MirGridType::Inventory,
            Self::Equipment(_) => MirGridType::Equipment,
        }
    }

    /// 起点下标。
    #[must_use]
    pub const fn slot(self) -> usize {
        match self {
            Self::Inventory(slot) | Self::Equipment(slot) => slot,
        }
    }
}

/// 正在拖拽的物品（用于画半透明幽灵）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DragState {
    /// 拖拽起点。
    pub source: DragSource,
    /// 物品模板号（取图标）。
    pub item_index: i32,
    /// 物品唯一编号。
    pub unique_id: u64,
    /// 堆叠数量（幽灵右下角显示）。
    pub count: u16,
}

/// 右键菜单里的一项动作。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContextAction {
    /// 使用。
    Use,
    /// 穿到 `to` 号装备槽。
    Equip {
        /// 目标装备槽。
        to: usize,
    },
    /// 丢弃。
    Drop,
}

/// 右键菜单的一项（已本地化的文案）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContextEntry {
    /// 动作。
    pub action: ContextAction,
    /// 文案。
    pub label: String,
}

/// 右键菜单状态。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContextMenu {
    /// 物品所在位置。
    pub source: DragSource,
    /// 物品唯一编号。
    pub unique_id: u64,
    /// 物品模板号。
    pub item_index: i32,
    /// 堆叠数量。
    pub count: u16,
    /// 弹出位置（鼠标）。
    pub anchor: Point,
}

impl ContextMenu {
    /// 菜单项：使用 /（可装备时）装备 / 丢弃。
    #[must_use]
    pub fn entries(&self, snapshot: &WorldSnapshot, language: Language) -> Vec<ContextEntry> {
        let mut entries = vec![ContextEntry {
            action: ContextAction::Use,
            label: labels::Label::Use.text(language).to_owned(),
        }];
        if let Some(info) = snapshot.item_info(self.item_index) {
            if let Some(to) = equipment::equipment_slot_for_item(snapshot, info.item_type) {
                entries.push(ContextEntry {
                    action: ContextAction::Equip { to },
                    label: labels::Label::Equip.text(language).to_owned(),
                });
            }
        }
        entries.push(ContextEntry {
            action: ContextAction::Drop,
            label: labels::Label::Drop.text(language).to_owned(),
        });
        entries
    }
}

/// 点地走路的意图。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WalkIntent {
    /// 朝目标的方向。
    pub direction: MirDirection,
    /// `true` = 直接走一步；`false` = 只转身。
    pub step: bool,
}

/// 根据「自己 → 目标格」算出走路意图。
///
/// 规则（不寻路）：目标与自己在同一行/列且相邻 → 直接走一步；否则朝目标方向——
/// 已经面向该方向就再走一步，否则先转身。按住左键由 `app.rs` 连续调用。
#[must_use]
pub fn walk_intent(snapshot: &WorldSnapshot, target: Point) -> Option<WalkIntent> {
    let me = snapshot.self_player.as_ref()?;
    if me.position == target {
        return None;
    }
    let direction = me.position.direction_to(target);
    let dx = (target.x - me.position.x).abs();
    let dy = (target.y - me.position.y).abs();
    let adjacent = dx.max(dy) == 1;
    let aligned = dx == 0 || dy == 0;
    let step = (adjacent && aligned) || direction == me.direction;
    Some(WalkIntent { direction, step })
}

/// 界面状态：面板开关、选中项、鼠标位置、拖拽/右键菜单、飘字与物品图标缓存。
pub struct UiState {
    /// 界面语言。
    pub language: Language,
    /// 背包面板是否打开（`I`）。
    pub show_inventory: bool,
    /// 技能面板是否打开（`F3`）。
    pub show_skills: bool,
    /// 装备纸娃娃面板是否打开（`E`）。
    pub show_equipment: bool,
    /// 鼠标位置（物理像素，与帧坐标一致）。
    pub mouse: Point,
    /// 背包选中的格子。
    pub selected_slot: Option<usize>,
    /// 商店选中的货架下标。
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
    /// 商店滚动位置（第一条可见货架的下标）。
    pub shop_scroll: usize,
    /// 对话 NPC 的对象 ID。
    pub npc_object_id: Option<u32>,
    /// 正在拖拽的物品。
    pub drag: Option<DragState>,
    /// 右键菜单。
    pub context_menu: Option<ContextMenu>,
    /// 客户端本地时钟（Unix 毫秒；伤害飘字与增益倒计时用）。
    pub time_millis: i64,
    /// 正在飘的伤害数字。
    pub damage_numbers: Vec<worldtext::DamageNumber>,
    /// 正在播放的短光效（`S.ObjectEffect`）。
    pub effects: Vec<buffs::ActiveEffect>,
    /// 已消费到的伤害事件编号。
    pub last_damage_id: u64,
    /// 已消费到的光效事件编号。
    pub last_effect_id: u64,
    /// `items.wzl` 图标缓存（`None` 表示该图像号没有素材）。
    pub icons: HashMap<u16, Option<Image>>,
    /// 上一次点击（双击判定）。
    last_click: Option<(i64, DragSource)>,
}

impl UiState {
    /// 新建界面状态。
    #[must_use]
    pub fn new(language: Language) -> Self {
        Self {
            language,
            show_inventory: false,
            show_skills: false,
            show_equipment: false,
            mouse: Point::new(0, 0),
            selected_slot: None,
            selected_goods: None,
            selected_magic: None,
            dialog: None,
            dialog_version: 0,
            shop_open: false,
            shop_suppressed: false,
            goods_version: 0,
            shop_scroll: 0,
            npc_object_id: None,
            drag: None,
            context_menu: None,
            time_millis: 0,
            damage_numbers: Vec::new(),
            effects: Vec::new(),
            last_damage_id: 0,
            last_effect_id: 0,
            icons: HashMap::new(),
            last_click: None,
        }
    }

    /// 写入客户端时钟（窗口模式每帧调用；测试可直接写任意时刻）。
    pub fn set_time(&mut self, now_millis: i64) {
        self.time_millis = now_millis;
    }

    /// 跟随快照更新对话/商店/选中项，并消费新的伤害/光效事件。
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
                self.shop_scroll = 0;
            }
            Some(_) => {}
            None => {
                self.shop_open = false;
                self.selected_goods = None;
                self.shop_scroll = 0;
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
        if self.drag.as_ref().is_some_and(|drag| match drag.source {
            DragSource::Inventory(slot) => snapshot.inventory_item(slot).is_none(),
            DragSource::Equipment(slot) => snapshot.equipment_item(slot).is_none(),
        }) {
            self.drag = None;
        }
        self.collect_world_events(snapshot);
        self.prune_world_events();
        self.clamp_shop_scroll(snapshot);
    }

    /// 消费新增的伤害/光效事件（快照会整体克隆，所以用递增 ID 去重）。
    fn collect_world_events(&mut self, snapshot: &WorldSnapshot) {
        let now = self.time_millis;
        for event in &snapshot.damage_events {
            if event.id <= self.last_damage_id {
                continue;
            }
            self.last_damage_id = event.id;
            if let Some(number) = worldtext::damage_from_event(event, snapshot, self.language, now)
            {
                self.damage_numbers.push(number);
            }
        }
        if self.damage_numbers.len() > 32 {
            let excess = self.damage_numbers.len() - 32;
            self.damage_numbers.drain(0..excess);
        }
        for event in &snapshot.effects {
            if event.id <= self.last_effect_id {
                continue;
            }
            self.last_effect_id = event.id;
            let time = if event.time == 0 {
                u32::try_from(buffs::DEFAULT_EFFECT_MS).unwrap_or(600)
            } else {
                event.time.clamp(200, 2_000)
            };
            self.effects.push(buffs::ActiveEffect {
                object_id: event.object_id,
                started: now,
                until: now.saturating_add(i64::from(time)),
                effect: event.effect,
            });
        }
        if self.effects.len() > 16 {
            let excess = self.effects.len() - 16;
            self.effects.drain(0..excess);
        }
    }

    /// 清掉过期的飘字与光效。
    fn prune_world_events(&mut self) {
        let now = self.time_millis;
        self.damage_numbers.retain(|number| !number.expired(now));
        self.effects.retain(|effect| !effect.expired(now));
    }

    /// 滚动位置收边到当前货架长度。
    fn clamp_shop_scroll(&mut self, snapshot: &WorldSnapshot) {
        let max_scroll = snapshot.npc_goods.as_ref().map_or(0, |goods| {
            goods.list.len().saturating_sub(shop::MAX_SHOP_ROWS)
        });
        self.shop_scroll = self.shop_scroll.min(max_scroll);
    }

    /// 关掉 NPC 对话与商店（`Esc` 用；调用方负责先发 `@exit`）。
    pub fn close_npc(&mut self) {
        self.dialog = None;
        self.shop_open = false;
        self.shop_suppressed = true;
        self.selected_goods = None;
        self.shop_scroll = 0;
        self.npc_object_id = None;
    }

    /// 关掉所有可切换面板。
    pub fn close_panels(&mut self) {
        self.show_inventory = false;
        self.show_skills = false;
        self.show_equipment = false;
        self.drag = None;
        self.context_menu = None;
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

    /// 滚轮：商店打开时上下滚动货架（`delta > 0` 向下），返回是否发生了变化。
    pub fn on_wheel(&mut self, snapshot: &WorldSnapshot, delta: i32) -> bool {
        if !self.shop_open {
            return false;
        }
        self.scroll_shop(snapshot, delta.signum())
    }

    /// 上下键滚动商店（`delta` 为 +1 / -1）。
    pub fn scroll_shop(&mut self, snapshot: &WorldSnapshot, delta: i32) -> bool {
        let Some(goods) = &snapshot.npc_goods else {
            return false;
        };
        let max_scroll = goods.list.len().saturating_sub(shop::MAX_SHOP_ROWS);
        if max_scroll == 0 {
            return false;
        }
        let before = self.shop_scroll;
        let next = if delta < 0 {
            self.shop_scroll
                .saturating_sub(usize::try_from(-delta).unwrap_or(1))
        } else {
            self.shop_scroll
                .saturating_add(usize::try_from(delta).unwrap_or(1))
        };
        self.shop_scroll = next.min(max_scroll);
        self.shop_scroll != before
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

    /// 鼠标左键按下：按「右键菜单 > 对话 > 商店 > 装备 > 背包 > 技能 > 世界」的顺序派发。
    ///
    /// `world_cell` 是鼠标对应的地图格（由 `MapScene::screen_to_cell` 算好）。
    /// 在背包格按下会同时开始拖拽；同一格 `inventory::DOUBLE_CLICK_MS` 内再次按下
    /// 视为双击 → [`UiAction::UseItem`]。
    pub fn on_click(
        &mut self,
        snapshot: &WorldSnapshot,
        text: &UiText,
        frame: Rect,
        world_cell: Option<Point>,
    ) -> Option<UiAction> {
        self.sync(snapshot);
        if let Some(menu) = self.context_menu.clone() {
            let entries = menu.entries(snapshot, self.language);
            let panel = inventory::context_menu_rect(menu.anchor, entries.len(), frame);
            if let Some(index) = inventory::context_menu_entry_at(panel, entries.len(), self.mouse)
            {
                self.context_menu = None;
                let entry = entries.get(index)?;
                return Self::context_action(&menu, entry.action);
            }
            // 点在菜单外：关掉菜单并吃掉这次点击（与 Crystal 的右键菜单一致）。
            self.context_menu = None;
            return None;
        }
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
                let layout = shop::shop_layout(
                    frame,
                    goods.list.len(),
                    self.shop_scroll,
                    shop::shop_left(self.show_equipment),
                );
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
        if self.show_equipment {
            let layout = equipment::equipment_layout(frame);
            if layout.panel.contains(self.mouse) {
                if let Some(slot) = layout.slot_at(self.mouse) {
                    self.start_drag(snapshot, DragSource::Equipment(slot));
                }
                return None;
            }
        }
        if self.show_inventory {
            let panel = inventory::inventory_panel_rect(frame);
            if panel.contains(self.mouse) {
                if let Some(slot) = inventory::inventory_slot_at(panel, self.mouse) {
                    self.selected_slot = Some(slot);
                    if let Some(item) = snapshot.inventory_item(slot) {
                        let source = DragSource::Inventory(slot);
                        let now = self.time_millis;
                        if self.last_click.is_some_and(|(time, last)| {
                            last == source && now.saturating_sub(time) <= inventory::DOUBLE_CLICK_MS
                        }) {
                            self.last_click = None;
                            self.drag = None;
                            return Some(UiAction::UseItem {
                                unique_id: item.unique_id,
                            });
                        }
                        self.last_click = Some((now, source));
                        self.drag = Some(DragState {
                            source,
                            item_index: item.item_index,
                            unique_id: item.unique_id,
                            count: item.count,
                        });
                    }
                }
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
            return walk_intent(snapshot, cell).map(|intent| UiAction::WorldMove {
                direction: intent.direction,
                step: intent.step,
            });
        }
        None
    }

    /// 开始拖拽某个格子里的物品（空格不产生拖拽）。
    fn start_drag(&mut self, snapshot: &WorldSnapshot, source: DragSource) {
        let item = match source {
            DragSource::Inventory(slot) => snapshot.inventory_item(slot),
            DragSource::Equipment(slot) => snapshot.equipment_item(slot),
        };
        self.drag = item.map(|item| DragState {
            source,
            item_index: item.item_index,
            unique_id: item.unique_id,
            count: item.count,
        });
    }

    /// 鼠标左键释放：把拖拽落点翻译成 [`UiAction::MoveItem`]。
    pub fn on_mouse_release(&mut self, snapshot: &WorldSnapshot, frame: Rect) -> Option<UiAction> {
        self.sync(snapshot);
        let drag = self.drag.take()?;
        let equipment_slot = self
            .show_equipment
            .then(|| equipment::equipment_layout(frame).slot_at(self.mouse))
            .flatten();
        if let Some(to) = equipment_slot {
            return match drag.source {
                DragSource::Inventory(from) => Some(UiAction::MoveItem {
                    grid: MirGridType::Inventory,
                    from: i32::try_from(from).unwrap_or(0),
                    to: i32::try_from(to).unwrap_or(0),
                    to_grid: MirGridType::Equipment,
                }),
                DragSource::Equipment(slot) if slot == to => None,
                DragSource::Equipment(_) => None,
            };
        }
        let inventory_slot = self
            .show_inventory
            .then(|| {
                inventory::inventory_slot_at(inventory::inventory_panel_rect(frame), self.mouse)
            })
            .flatten();
        let to = inventory_slot?;
        match drag.source {
            DragSource::Inventory(from) if from == to => None,
            DragSource::Inventory(from) => Some(UiAction::MoveItem {
                grid: MirGridType::Inventory,
                from: i32::try_from(from).unwrap_or(0),
                to: i32::try_from(to).unwrap_or(0),
                to_grid: MirGridType::Inventory,
            }),
            DragSource::Equipment(from) => Some(UiAction::MoveItem {
                grid: MirGridType::Equipment,
                from: i32::try_from(from).unwrap_or(0),
                to: i32::try_from(to).unwrap_or(0),
                to_grid: MirGridType::Inventory,
            }),
        }
    }

    /// 鼠标右键：背包物品弹出「使用/装备/丢弃」菜单。
    pub fn on_right_click(&mut self, snapshot: &WorldSnapshot, frame: Rect) -> Option<UiAction> {
        self.sync(snapshot);
        self.context_menu = None;
        if !self.show_inventory {
            return None;
        }
        let panel = inventory::inventory_panel_rect(frame);
        let slot = inventory::inventory_slot_at(panel, self.mouse)?;
        let item = snapshot.inventory_item(slot)?;
        self.selected_slot = Some(slot);
        self.context_menu = Some(ContextMenu {
            source: DragSource::Inventory(slot),
            unique_id: item.unique_id,
            item_index: item.item_index,
            count: item.count,
            anchor: self.mouse,
        });
        None
    }

    /// 把右键菜单项变成动作。
    fn context_action(menu: &ContextMenu, action: ContextAction) -> Option<UiAction> {
        match action {
            ContextAction::Use => Some(UiAction::UseItem {
                unique_id: menu.unique_id,
            }),
            ContextAction::Equip { to } => {
                let from = menu.source.slot();
                // 只有背包里的物品可以「穿」；装备槽上右键没有装备项，防御性返回。
                if menu.source.grid() != MirGridType::Inventory {
                    return None;
                }
                Some(UiAction::MoveItem {
                    grid: MirGridType::Inventory,
                    from: i32::try_from(from).unwrap_or(0),
                    to: i32::try_from(to).unwrap_or(0),
                    to_grid: MirGridType::Equipment,
                })
            }
            ContextAction::Drop => Some(UiAction::DropItem {
                unique_id: menu.unique_id,
                count: 1,
            }),
        }
    }

    /// 鼠标悬停的背包格（tooltip 用）。
    #[must_use]
    pub fn hovered_inventory_slot(&self, frame: Rect) -> Option<usize> {
        if !self.show_inventory {
            return None;
        }
        inventory::inventory_slot_at(inventory::inventory_panel_rect(frame), self.mouse)
    }

    /// 鼠标悬停的装备槽（tooltip 用）。
    #[must_use]
    pub fn hovered_equipment_slot(&self, frame: Rect) -> Option<usize> {
        if !self.show_equipment {
            return None;
        }
        equipment::equipment_layout(frame).slot_at(self.mouse)
    }
}

/// 画所有面板与世界叠层。
///
/// 绘制顺序：世界文字（名字/伤害/光效）→ 状态栏 → 增益栏 → 聊天 → 装备 → 背包 →
/// 商店 → 技能 → 对话 → tooltip → 右键菜单 → 拖拽幽灵。
pub fn draw(
    state: &mut UiState,
    frame: &mut Frame,
    scene: &mut MapScene,
    snapshot: &WorldSnapshot,
    text: &UiText,
    chat_input: Option<&str>,
    center: Point,
) {
    state.sync(snapshot);
    let screen = Rect::screen(frame.width, frame.height);
    worldtext::draw_names(frame, snapshot, scene, center, text);
    worldtext::draw_damage_numbers(
        frame,
        &state.damage_numbers,
        scene,
        center,
        text,
        state.time_millis,
    );
    buffs::draw_effects(
        frame,
        &state.effects,
        snapshot,
        scene,
        center,
        state.time_millis,
    );
    let context = PanelContext {
        snapshot,
        text,
        language: state.language,
    };
    hud::draw_status(frame, snapshot, text, state.language);
    buffs::draw(
        frame,
        snapshot,
        text,
        state.language,
        state.time_millis,
        buffs::bar_left(state.show_equipment, state.shop_open),
    );
    hud::draw_chat(frame, snapshot, text, chat_input, state.language);

    if state.show_equipment {
        let layout = equipment::equipment_layout(screen);
        let hover = layout.slot_at(state.mouse);
        let UiState { icons, .. } = state;
        let mut icon_fn = |image: u16| -> Option<Image> { load_icon(scene, icons, image) };
        equipment::draw(frame, &layout, &context, hover, &mut icon_fn);
    }

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
            let layout = shop::shop_layout(
                screen,
                goods.list.len(),
                state.shop_scroll,
                shop::shop_left(state.show_equipment),
            );
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

    if state.context_menu.is_none() {
        if let Some(item) = hovered_item(state, snapshot, screen) {
            let content = tooltip::content_for_item(snapshot, item, state.language);
            let layout = tooltip::layout(&content, text, screen, state.mouse);
            tooltip::draw(frame, text, &layout);
        }
    }

    if let Some(menu) = &state.context_menu {
        let entries = menu.entries(snapshot, state.language);
        let panel = inventory::context_menu_rect(menu.anchor, entries.len(), screen);
        let hover = inventory::context_menu_entry_at(panel, entries.len(), state.mouse);
        inventory::draw_context_menu(frame, text, panel, &entries, hover);
    }

    if let Some(drag) = &state.drag {
        let image = snapshot.item_image(drag.item_index).and_then(|image| {
            let UiState { icons, .. } = state;
            load_icon(scene, icons, image)
        });
        inventory::draw_drag_ghost(frame, text, image.as_ref(), drag.count, state.mouse);
    }
}

/// 当前鼠标下的物品（背包优先，其次装备槽）。
fn hovered_item<'a>(
    state: &UiState,
    snapshot: &'a WorldSnapshot,
    screen: Rect,
) -> Option<&'a InventoryItem> {
    if let Some(slot) = state.hovered_inventory_slot(screen) {
        if let Some(item) = snapshot.inventory_item(slot) {
            return Some(item);
        }
    }
    let slot = state.hovered_equipment_slot(screen)?;
    snapshot.equipment_item(slot)
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
        ChatLine, DamageEvent, EffectEvent, InventoryItem, ItemInfoView, MagicView, MapSnapshot,
        NpcDialog, SelfPlayer, ShopEntry, ShopGoods, WorldEntity,
    };
    use mir_core::Language;
    use mir_core::enums::{ChatType, DamageType, MirClass, MirDirection, MirGender, Spell};
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
                looks: crate::net::PlayerLooks::default(),
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
                ..ItemInfoView::default()
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
                looks: crate::net::PlayerLooks::default(),
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
        assert!(state.drag.is_some(), "按下有物品的格子应开始拖拽");
    }

    #[test]
    fn double_clicking_an_inventory_item_uses_it() {
        let snapshot = snapshot();
        let mut state = UiState::new(Language::ZhCn);
        state.show_inventory = true;
        state.set_time(1_000);
        let frame = Rect::screen(800, 600);
        let panel = inventory::inventory_panel_rect(frame);
        state.mouse = inventory::inventory_slot_rect(panel, 0).unwrap().center();
        assert!(state.on_click(&snapshot, &text(), frame, None).is_none());
        state.set_time(1_200);
        let action = state
            .on_click(&snapshot, &text(), frame, None)
            .expect("第二次点击应是双击");
        assert_eq!(action, UiAction::UseItem { unique_id: 11 });
        assert!(state.drag.is_none(), "双击使用不应继续拖拽");
    }

    #[test]
    fn dragging_inventory_to_equipment_emits_move_item_with_inventory_source() {
        let mut snapshot = snapshot();
        let mut info = snapshot.item_infos[&1].clone();
        info.item_type = mir_core::enums::ItemType::Potion;
        snapshot.item_infos.insert(1, info);
        snapshot.equipment = vec![None; crate::net::EQUIPMENT_SLOTS];
        let mut state = UiState::new(Language::ZhCn);
        state.show_inventory = true;
        state.show_equipment = true;
        let frame = Rect::screen(800, 600);
        let inv = inventory::inventory_panel_rect(frame);
        let equip = equipment::equipment_layout(frame);
        state.mouse = inventory::inventory_slot_rect(inv, 0).unwrap().center();
        assert!(state.on_click(&snapshot, &text(), frame, None).is_none());
        assert!(state.drag.is_some());
        state.mouse = equip.slots[0].center();
        let action = state
            .on_mouse_release(&snapshot, frame)
            .expect("拖到装备槽应产生 MoveItem");
        assert_eq!(
            action,
            UiAction::MoveItem {
                grid: MirGridType::Inventory,
                from: 0,
                to: 0,
                to_grid: MirGridType::Equipment
            }
        );
    }

    #[test]
    fn dragging_between_inventory_slots_swaps_and_equipment_to_inventory_unequips() {
        let mut snapshot = snapshot();
        snapshot.inventory = vec![Some(item(11, 1, 2)), Some(item(12, 1, 1))];
        snapshot.equipment = vec![None; crate::net::EQUIPMENT_SLOTS];
        snapshot.equipment[3] = Some(item(13, 1, 1));
        let mut state = UiState::new(Language::ZhCn);
        state.show_inventory = true;
        state.show_equipment = true;
        let frame = Rect::screen(800, 600);
        let inv = inventory::inventory_panel_rect(frame);
        let equip = equipment::equipment_layout(frame);
        // 背包内交换。
        state.mouse = inventory::inventory_slot_rect(inv, 0).unwrap().center();
        let _ = state.on_click(&snapshot, &text(), frame, None);
        state.mouse = inventory::inventory_slot_rect(inv, 1).unwrap().center();
        assert_eq!(
            state.on_mouse_release(&snapshot, frame),
            Some(UiAction::MoveItem {
                grid: MirGridType::Inventory,
                from: 0,
                to: 1,
                to_grid: MirGridType::Inventory
            })
        );
        // 装备槽拖回背包。
        state.mouse = equip.slots[3].center();
        let _ = state.on_click(&snapshot, &text(), frame, None);
        state.mouse = inventory::inventory_slot_rect(inv, 1).unwrap().center();
        assert_eq!(
            state.on_mouse_release(&snapshot, frame),
            Some(UiAction::MoveItem {
                grid: MirGridType::Equipment,
                from: 3,
                to: 1,
                to_grid: MirGridType::Inventory
            })
        );
    }

    #[test]
    fn right_click_inventory_opens_a_context_menu_and_equip_moves_to_a_slot() {
        let mut snapshot = snapshot();
        let mut info = snapshot.item_infos[&1].clone();
        info.item_type = mir_core::enums::ItemType::Ring;
        snapshot.item_infos.insert(1, info);
        snapshot.equipment = vec![None; crate::net::EQUIPMENT_SLOTS];
        let mut state = UiState::new(Language::ZhCn);
        state.show_inventory = true;
        let frame = Rect::screen(800, 600);
        let inv = inventory::inventory_panel_rect(frame);
        state.mouse = inventory::inventory_slot_rect(inv, 0).unwrap().center();
        assert!(state.on_right_click(&snapshot, frame).is_none());
        let menu = state.context_menu.clone().expect("右键应打开菜单");
        let entries = menu.entries(&snapshot, Language::ZhCn);
        assert_eq!(entries.len(), 3, "使用/装备/丢弃");
        let panel = inventory::context_menu_rect(menu.anchor, entries.len(), frame);
        state.mouse = inventory::context_menu_entry_rect(panel, 1).center();
        let action = state
            .on_click(&snapshot, &text(), frame, None)
            .expect("点「装备」应产生动作");
        assert_eq!(
            action,
            UiAction::MoveItem {
                grid: MirGridType::Inventory,
                from: 0,
                to: mir_core::enums::EquipmentSlot::RingL as i32,
                to_grid: MirGridType::Equipment
            }
        );
        assert!(state.context_menu.is_none());
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
        let layout = shop::shop_layout(frame, 1, 0, shop::SHOP_LEFT);
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
    fn shop_scrolls_with_the_wheel_and_clamps_to_the_last_page() {
        let mut snapshot = with_npc(snapshot());
        let list: Vec<ShopEntry> = (0..30_u64)
            .map(|index| ShopEntry {
                unique_id: index,
                item_index: 1,
                name: format!("商品 {index}"),
                image: 5,
                price: 10,
                count: 1,
                current_dura: 0,
                max_dura: 0,
            })
            .collect();
        snapshot.npc_goods = Some(ShopGoods {
            list,
            rate: 1.0,
            version: 1,
        });
        snapshot.npc_goods_version = 1;
        let mut state = UiState::new(Language::ZhCn);
        state.sync(&snapshot);
        assert!(state.shop_open);
        assert!(state.on_wheel(&snapshot, 1), "滚轮向下应滚动");
        assert_eq!(state.shop_scroll, 1);
        assert!(state.scroll_shop(&snapshot, 100));
        assert_eq!(state.shop_scroll, 30 - shop::MAX_SHOP_ROWS);
        assert!(!state.scroll_shop(&snapshot, 1), "到底后不再变化");
        assert!(state.scroll_shop(&snapshot, -100));
        assert_eq!(state.shop_scroll, 0);
        // 货架换版本后回到顶部。
        state.shop_scroll = 5;
        snapshot.npc_goods_version = 2;
        snapshot.npc_goods.as_mut().unwrap().version = 2;
        state.sync(&snapshot);
        assert_eq!(state.shop_scroll, 0);
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
    fn clicking_the_ground_walks_or_turns_toward_the_target() {
        let snapshot = snapshot();
        // 同列相邻：直接走。
        assert_eq!(
            walk_intent(&snapshot, Point::new(10, 11)),
            Some(WalkIntent {
                direction: MirDirection::Down,
                step: true
            })
        );
        // 已经在朝向下：走一步。
        assert_eq!(
            walk_intent(&snapshot, Point::new(10, 13)),
            Some(WalkIntent {
                direction: MirDirection::Down,
                step: true
            })
        );
        // 方向不同：先转身。
        assert_eq!(
            walk_intent(&snapshot, Point::new(13, 10)),
            Some(WalkIntent {
                direction: MirDirection::Right,
                step: false
            })
        );
        // 原地点自己：没有动作。
        assert_eq!(walk_intent(&snapshot, Point::new(10, 10)), None);
        // 面板外点地：产生 WorldMove。
        let mut state = UiState::new(Language::ZhCn);
        let action = state
            .on_click(
                &snapshot,
                &text(),
                Rect::screen(800, 600),
                Some(Point::new(11, 11)),
            )
            .expect("点地应走路");
        assert_eq!(
            action,
            UiAction::WorldMove {
                direction: MirDirection::DownRight,
                step: false
            }
        );
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

    #[test]
    fn new_damage_and_effect_events_are_consumed_once_with_the_injected_clock() {
        let mut snapshot = snapshot();
        let mut state = UiState::new(Language::ZhCn);
        state.set_time(10_000);
        state.sync(&snapshot);
        assert!(state.damage_numbers.is_empty());
        snapshot.damage_events.push(DamageEvent {
            id: 1,
            object_id: 7,
            damage: Some(12),
            damage_type: Some(DamageType::Hit),
            location: None,
        });
        snapshot.effects.push(EffectEvent {
            id: 1,
            object_id: 7,
            effect: mir_core::enums::SpellEffect::MagicShieldUp,
            time: 600,
        });
        state.sync(&snapshot);
        assert_eq!(state.damage_numbers.len(), 1);
        assert_eq!(state.effects.len(), 1);
        assert_eq!(state.damage_numbers[0].text, "12");
        // 同一事件不会被重复消费。
        state.sync(&snapshot);
        assert_eq!(state.damage_numbers.len(), 1);
        assert_eq!(state.effects.len(), 1);
        // 500ms 时还在，2s 后消失。
        state.set_time(10_500);
        state.sync(&snapshot);
        assert_eq!(state.damage_numbers.len(), 1);
        assert_eq!(state.effects.len(), 1);
        state.set_time(12_100);
        state.sync(&snapshot);
        assert!(state.damage_numbers.is_empty(), "2s 后飘字应消失");
        assert!(state.effects.is_empty(), "光效应结束");
    }
}
