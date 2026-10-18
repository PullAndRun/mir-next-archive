//! 技能面板（F3）：已学技能列表与 1..9 快捷键施法。
//!
//! 数据来自 `S.UserInformation.Magics`，并随 `S.NewMagic` / `S.MagicLeveled` /
//! `S.RemoveMagic` 更新。目标选择保持简单：
//!
//! * 沿当前朝向前方 1..=3 格找最近的活怪物 → 作为 `target_id`；
//! * 没有目标时，治疗类技能（治愈术/群体治愈术/治愈之光）以自己为目标；
//! * 其它技能把落点放在前方一格、`target_id = 0`（范围法术可用，单体法术会被服务端忽略）。
//!
//! 键位绑定：技能自带 `key`（1..=9）优先；没绑定时按「第 N 个技能」临时绑定并返回
//! [`MagicKeyChange`]，由调用方发 `C.MagicKey`。

use mir_core::Language;
use mir_core::Point;
use mir_core::enums::{MirDirection, Spell};
use mir_render::{EntityKind, Frame};

use crate::geom::Rect;
use crate::net::{MagicView, WorldSnapshot};
use crate::ui::PanelContext;
use crate::ui::hud::{BODY_COLOR, GOLD_COLOR, MUTED_COLOR, SIZE_BODY, SIZE_SMALL, TITLE_COLOR};
use crate::ui::labels::{self, Label};

/// 技能面板宽度。
pub const SKILL_WIDTH: u32 = 240;
/// 每行高度。
pub const SKILL_ROW_HEIGHT: u32 = 24;
/// 标题高度。
pub const SKILL_TITLE_HEIGHT: u32 = 24;
/// 底部提示高度。
pub const SKILL_FOOTER_HEIGHT: u32 = 22;
/// 内边距。
pub const SKILL_PADDING: u32 = 8;
/// 快捷栏键数。
pub const SKILL_KEYS: u8 = 9;
/// 施法搜索距离（格）。
pub const CAST_RANGE: i32 = 3;
/// 最多显示多少行（再多的技能等以后做滚动）。
pub const MAX_SKILL_ROWS: usize = 6;
/// 面板顶部位置（避开商店/状态栏，底部避开聊天面板）。
pub const SKILL_TOP: i32 = 214;

/// 技能面板布局。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkillLayout {
    /// 面板矩形。
    pub panel: Rect,
    /// 每行技能的矩形。
    pub rows: Vec<Rect>,
    /// 底部提示区域。
    pub footer: Rect,
}

impl SkillLayout {
    /// 命中测试：点在哪个技能行上。
    #[must_use]
    pub fn row_at(&self, point: Point) -> Option<usize> {
        self.rows.iter().position(|rect| rect.contains(point))
    }
}

/// 计算技能面板布局（顶部居中，不压住左右两侧的背包/商店）。
#[must_use]
pub fn skill_layout(frame: Rect, row_count: usize) -> SkillLayout {
    let visible = row_count.clamp(1, MAX_SKILL_ROWS);
    let width = SKILL_WIDTH.min(frame.width.saturating_sub(16)).max(160);
    let height = SKILL_TITLE_HEIGHT
        + u32::try_from(visible)
            .unwrap_or(0)
            .saturating_mul(SKILL_ROW_HEIGHT)
        + SKILL_FOOTER_HEIGHT
        + SKILL_PADDING
        + 4;
    let panel = Rect::new(
        (frame.width.saturating_sub(width) / 2) as i32,
        SKILL_TOP
            .min(frame.bottom() - i32::try_from(height).unwrap_or(0) - 8)
            .max(60),
        width,
        height,
    );
    let mut rows = Vec::with_capacity(visible);
    for index in 0..visible {
        rows.push(Rect::new(
            panel.x + i32::try_from(SKILL_PADDING).unwrap_or(0),
            panel.y
                + i32::try_from(SKILL_TITLE_HEIGHT).unwrap_or(0)
                + i32::try_from(index).unwrap_or(0) * i32::try_from(SKILL_ROW_HEIGHT).unwrap_or(0),
            panel.width.saturating_sub(SKILL_PADDING * 2),
            SKILL_ROW_HEIGHT,
        ));
    }
    let footer = Rect::new(
        panel.x + i32::try_from(SKILL_PADDING).unwrap_or(0),
        panel.bottom() - i32::try_from(SKILL_FOOTER_HEIGHT + 4).unwrap_or(0),
        panel.width.saturating_sub(SKILL_PADDING * 2),
        SKILL_FOOTER_HEIGHT,
    );
    SkillLayout {
        panel,
        rows,
        footer,
    }
}

/// 技能行文案：`火球术 等级 2  MP3`（快捷键由行首的小方块表示）。
#[must_use]
pub fn magic_row_text(magic: &MagicView, language: Language) -> String {
    format!(
        "{} {} {}  MP{}",
        magic.name,
        Label::Level.text(language),
        magic.level,
        magic.base_cost
    )
}

/// 按键对应的技能下标：优先已绑定该键的技能，否则按第 N 个技能。
#[must_use]
pub fn magic_index_for_key(magics: &[MagicView], key: u8) -> Option<usize> {
    if key == 0 || key > SKILL_KEYS {
        return None;
    }
    magics
        .iter()
        .position(|magic| magic.key == key)
        .or_else(|| {
            usize::from(key)
                .checked_sub(1)
                .filter(|index| *index < magics.len())
        })
}

/// 需要重新绑定快捷键时的变化（调用方发 `C.MagicKey`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MagicKeyChange {
    /// 技能。
    pub spell: Spell,
    /// 新按键。
    pub key: u8,
    /// 旧按键（0 表示原本未绑定）。
    pub old_key: u8,
}

/// 按键施法的结果：可选的绑定动作 + 施法请求。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MagicPress {
    /// 需要发给服务端的绑定变化。
    pub bind: Option<MagicKeyChange>,
    /// 施法请求。
    pub cast: CastRequest,
}

/// 施法请求（对应 `C.Magic` 字段）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CastRequest {
    /// 技能。
    pub spell: Spell,
    /// 朝向。
    pub direction: MirDirection,
    /// 目标对象 ID（0 表示无目标）。
    pub target_id: u32,
    /// 目标格。
    pub location: Point,
}

/// 某个按键被按下：决定放哪个技能、是否需要绑定。
#[must_use]
pub fn press_magic_key(snapshot: &WorldSnapshot, key: u8) -> Option<MagicPress> {
    let index = magic_index_for_key(&snapshot.magics, key)?;
    let magic = snapshot.magics.get(index)?;
    let bind = if magic.key == key {
        None
    } else {
        Some(MagicKeyChange {
            spell: magic.spell,
            key,
            old_key: magic.key,
        })
    };
    Some(MagicPress {
        bind,
        cast: cast_request(snapshot, magic.spell)?,
    })
}

/// 为技能算一个施法请求；没有自己时返回 `None`。
#[must_use]
pub fn cast_request(snapshot: &WorldSnapshot, spell: Spell) -> Option<CastRequest> {
    let me = snapshot.self_player.as_ref()?;
    let direction = me.direction;
    for distance in 1..=CAST_RANGE {
        let cell = me.position.moved(direction, distance);
        let target = snapshot.entities.values().find(|entity| {
            entity.kind == EntityKind::Monster && !entity.dead && entity.position == cell
        });
        if let Some(target) = target {
            return Some(CastRequest {
                spell,
                direction,
                target_id: target.object_id,
                location: cell,
            });
        }
    }
    if is_heal_spell(spell) {
        return Some(CastRequest {
            spell,
            direction,
            target_id: me.object_id,
            location: me.position,
        });
    }
    Some(CastRequest {
        spell,
        direction,
        target_id: 0,
        location: me.position.moved(direction, 1),
    })
}

/// 治疗类技能（没有目标时对自己施放）。
#[must_use]
pub const fn is_heal_spell(spell: Spell) -> bool {
    matches!(
        spell,
        Spell::Healing | Spell::MassHealing | Spell::HealingCircle
    )
}

/// 画技能面板。
pub fn draw(
    frame: &mut Frame,
    layout: &SkillLayout,
    context: &PanelContext<'_>,
    selected: Option<usize>,
) {
    let text = context.text;
    let language = context.language;
    let snapshot = context.snapshot;
    crate::ui::draw_panel(frame, layout.panel);
    text.draw_clipped(
        frame,
        layout.panel.x + 10,
        layout.panel.y + 3,
        &format!("{} (F3)", Label::Skills.text(language)),
        SIZE_BODY,
        TITLE_COLOR,
        Rect::new(
            layout.panel.x,
            layout.panel.y,
            layout.panel.width,
            SKILL_TITLE_HEIGHT,
        ),
    );
    if snapshot.magics.is_empty() {
        text.draw_clipped(
            frame,
            layout.rows[0].x,
            layout.rows[0].y + 4,
            Label::NoSkills.text(language),
            SIZE_SMALL,
            MUTED_COLOR,
            layout.rows[0],
        );
    } else {
        for (index, row) in layout.rows.iter().enumerate() {
            let Some(magic) = snapshot.magics.get(index) else {
                break;
            };
            if selected == Some(index) {
                frame.fill_rect(row.x, row.y, row.width, row.height, [70, 62, 30, 220]);
                frame.fill_rect(row.x, row.y, 2, row.height, [255, 216, 96, 255]);
            } else if index % 2 == 1 {
                frame.fill_rect(row.x, row.y, row.width, row.height, [26, 30, 38, 150]);
            }
            // 快捷键小方块。
            if (1..=SKILL_KEYS).contains(&magic.key) {
                let badge = Rect::new(row.x + 2, row.y + 3, 16, 16);
                frame.fill_rect(
                    badge.x,
                    badge.y,
                    badge.width,
                    badge.height,
                    [52, 60, 76, 255],
                );
                text.draw_center(
                    frame,
                    badge,
                    &magic.key.to_string(),
                    SIZE_SMALL,
                    [255, 240, 180, 255],
                );
            }
            let name_area = Rect::new(row.x + 24, row.y, row.width.saturating_sub(28), row.height);
            text.draw_clipped(
                frame,
                name_area.x,
                row.y + 3,
                &magic_row_text(magic, language),
                SIZE_SMALL,
                if selected == Some(index) {
                    [255, 255, 220, 255]
                } else {
                    BODY_COLOR
                },
                name_area,
            );
            if magic.level >= 3 {
                text.draw_right(
                    frame,
                    row.right() - 4,
                    row.y + 3,
                    "MAX",
                    SIZE_SMALL,
                    GOLD_COLOR,
                );
            }
        }
    }
    text.draw_clipped(
        frame,
        layout.footer.x,
        layout.footer.y,
        &format!("{} 1..{SKILL_KEYS}", labels::Label::CastHint.text(language)),
        SIZE_SMALL,
        MUTED_COLOR,
        layout.footer,
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::net::{SelfPlayer, WorldEntity};
    use mir_core::enums::{MirClass, MirDirection, MirGender};
    use mir_render::EntityKind;

    fn magic(spell: Spell, key: u8, name: &str) -> MagicView {
        MagicView {
            name: name.to_owned(),
            spell,
            level: 1,
            key,
            base_cost: 5,
            icon: 0,
            range: 3,
        }
    }

    fn snapshot_with_self() -> WorldSnapshot {
        WorldSnapshot {
            self_player: Some(SelfPlayer {
                object_id: 7,
                name: "法师".to_owned(),
                position: Point::new(10, 10),
                direction: MirDirection::Right,
                hp: 10,
                mp: 10,
                hp_max: 10,
                mp_max: 10,
                level: 1,
                class: MirClass::Wizard,
                gender: MirGender::Male,
                hair: 0,
                gold: 0,
                dead: false,
                looks: crate::net::PlayerLooks::default(),
            }),
            ..WorldSnapshot::default()
        }
    }

    #[test]
    fn row_text_shows_name_level_and_cost() {
        let fireball = magic(Spell::FireBall, 2, "火球术");
        assert_eq!(
            magic_row_text(&fireball, Language::ZhCn),
            "火球术 等级 1  MP5"
        );
        let unbound = magic(Spell::Healing, 0, "治愈术");
        assert_eq!(magic_row_text(&unbound, Language::En), "治愈术 Lv 1  MP5");
    }

    #[test]
    fn magic_index_prefers_bound_keys_then_falls_back_by_position() {
        let magics = vec![
            magic(Spell::FireBall, 0, "火球术"),
            magic(Spell::Healing, 2, "治愈术"),
        ];
        assert_eq!(magic_index_for_key(&magics, 2), Some(1));
        assert_eq!(magic_index_for_key(&magics, 1), Some(0));
        assert_eq!(magic_index_for_key(&magics, 3), None);
        assert_eq!(magic_index_for_key(&magics, 0), None);
    }

    #[test]
    fn press_magic_key_reports_binding_for_unbound_skills() {
        let mut snapshot = snapshot_with_self();
        snapshot.magics = vec![magic(Spell::FireBall, 0, "火球术")];
        let press = press_magic_key(&snapshot, 1).expect("应能施法");
        assert_eq!(
            press.bind,
            Some(MagicKeyChange {
                spell: Spell::FireBall,
                key: 1,
                old_key: 0
            })
        );
        assert_eq!(press.cast.spell, Spell::FireBall);
        assert_eq!(press.cast.direction, MirDirection::Right);

        // 已绑定的技能不再产生绑定动作。
        snapshot.magics = vec![magic(Spell::FireBall, 1, "火球术")];
        let press = press_magic_key(&snapshot, 1).expect("应能施法");
        assert!(press.bind.is_none());
    }

    #[test]
    fn cast_targets_the_nearest_monster_in_front() {
        let mut snapshot = snapshot_with_self();
        for (object_id, x) in [(100u32, 12i32), (101, 11)] {
            snapshot.entities.insert(
                object_id,
                WorldEntity {
                    object_id,
                    kind: EntityKind::Monster,
                    name: "鸡".to_owned(),
                    position: Point::new(x, 10),
                    direction: MirDirection::Left,
                    image: 0,
                    looks: crate::net::PlayerLooks::default(),
                    dead: false,
                    hp_percent: Some(100),
                },
            );
        }
        let cast = cast_request(&snapshot, Spell::FireBall).expect("有自己");
        assert_eq!(cast.target_id, 101, "应选最近（11,10）的怪");
        assert_eq!(cast.location, Point::new(11, 10));
        assert_eq!(cast.direction, MirDirection::Right);

        // 没有怪物时治疗法术以自己为目标。
        snapshot.entities.clear();
        let heal = cast_request(&snapshot, Spell::Healing).expect("有自己");
        assert_eq!(heal.target_id, 7);
        assert_eq!(heal.location, Point::new(10, 10));

        // 非治疗法术没有目标时打到前方一格。
        let fire = cast_request(&snapshot, Spell::FireBall).expect("有自己");
        assert_eq!(fire.target_id, 0);
        assert_eq!(fire.location, Point::new(11, 10));
    }

    #[test]
    fn skill_rows_are_hit_testable() {
        let layout = skill_layout(Rect::screen(800, 600), 3);
        assert_eq!(layout.rows.len(), 3);
        for (index, row) in layout.rows.iter().enumerate() {
            assert!(layout.panel.contains(row.center()));
            assert_eq!(layout.row_at(row.center()), Some(index));
        }
        assert_eq!(layout.row_at(Point::new(0, 0)), None);
        let empty = skill_layout(Rect::screen(800, 600), 0);
        assert_eq!(empty.rows.len(), 1);
    }
}
