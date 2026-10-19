//! 攻城（沙巴克）客户端表现：器械、血条、状态 HUD 与结算横幅。
//!
//! 数据来自服务端 t67 的包（`S.ConquestInfo` 300 / `S.ConquestObject` 301 /
//! `S.ConquestResult` 302），客户端只读 `WorldSnapshot.conquest`。
//!
//! # 器械素材与回退
//!
//! 扁平语料没有「城门/城墙/箭塔」的确定帧号映射；这里先尝试
//! `scene.libraries().image(2, 3 + kind)`（`objects.wzl`，门/墙/塔各一张），
//! 取不到（空帧/缺库）时回退为**半透明色块 + 类型文字**，不 panic。
//! 血条复用怪物头顶样式：底色深灰、前景按归属行会 ID 派生颜色。
//! 摧毁后 1 秒内画一次碎裂粒子，然后从画面移除。

use std::time::{SystemTime, UNIX_EPOCH};

use mir_core::Language;
use mir_core::Point;
use mir_protocol::packets::server::ConquestObjectType;
use mir_render::{CELL_HEIGHT, CELL_WIDTH, Frame, MapScene};

use crate::geom::Rect;

use crate::net::{ConquestView, WorldSnapshot};
use crate::ui::UiText;
use crate::ui::labels::{self, Label};

/// 结算横幅显示时长（5 秒）。
pub const RESULT_BANNER_MS: u64 = 5_000;
/// 摧毁碎裂效果时长。
pub const SHATTER_MS: u64 = 1_000;
/// 控制点满分（Crystal `MAX_CONTROL_POINTS = 6`）。
pub const MAX_CONTROL_POINTS: i32 = 6;

/// 本地当前时间（Unix 毫秒）。
#[must_use]
pub fn now_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| {
            u64::try_from(duration.as_millis()).unwrap_or(u64::MAX)
        })
}

/// 剩余秒数（`end_time` 与服务端时钟同源；已结束返回 0）。
#[must_use]
pub fn remaining_seconds(conquest: &ConquestView, now_ms: u64) -> i64 {
    let now = i64::try_from(now_ms).unwrap_or(i64::MAX);
    ((conquest.end_time - now) / 1000).max(0)
}

/// `mm:ss` 格式。
#[must_use]
pub fn format_remaining(seconds: i64) -> String {
    let seconds = seconds.max(0);
    format!("{:02}:{:02}", seconds / 60, seconds % 60)
}

/// 归属行会颜色：0 为灰色，其余按 ID 派生稳定色。
#[must_use]
pub fn owner_color(owner: i32) -> [u8; 4] {
    if owner == 0 {
        return [150, 150, 150, 255];
    }
    let hash = u32::from_ne_bytes(i32::to_ne_bytes(owner));
    let red = 80 + u8::try_from((hash >> 3) % 140).unwrap_or(0);
    let green = 80 + u8::try_from((hash >> 11) % 140).unwrap_or(0);
    let blue = 80 + u8::try_from((hash >> 19) % 140).unwrap_or(0);
    [red, green, blue, 255]
}

/// 器械类型对应的回退色块颜色。
#[must_use]
pub const fn object_color(kind: ConquestObjectType) -> [u8; 4] {
    match kind {
        ConquestObjectType::Gate => [120, 80, 40, 190],
        ConquestObjectType::Wall => [90, 90, 96, 200],
        ConquestObjectType::Archer => [120, 60, 140, 190],
    }
}

/// 器械屏幕矩形（格原点处，向上生长）。
#[must_use]
pub fn object_rect(scene: &MapScene, center: Point, location: Point) -> Rect {
    let (x, y) = scene.world_to_screen(center, location);
    let width = CELL_WIDTH;
    let height = CELL_HEIGHT;
    Rect::new(
        x + 4,
        y - height,
        u32::try_from(width - 8).unwrap_or(40),
        u32::try_from(height + height / 2).unwrap_or(48),
    )
}

/// 画所有攻城器械（地形/实体之后、HUD 之前）。
pub fn draw_objects(
    frame: &mut Frame,
    snapshot: &WorldSnapshot,
    scene: &MapScene,
    center: Point,
    text: &UiText,
    language: Language,
    now_ms: u64,
) {
    let Some(conquest) = &snapshot.conquest else {
        return;
    };
    let mut objects: Vec<_> = conquest.objects.values().collect();
    objects.sort_by_key(|object| (object.location.y, object.location.x));
    for object in objects {
        let rect = object_rect(scene, center, object.location);
        if object.destroyed {
            if now_ms.saturating_sub(object.destroyed_at_ms) < SHATTER_MS {
                draw_shatter(frame, rect, object.kind);
            }
            continue;
        }
        // 真实素材尝试：objects.wzl 的 3+kind；空帧/缺库时回退色块。
        let sprite = scene.libraries().image(2, 3 + u32::from(object.kind as u8));
        if let Some(image) = sprite {
            frame.blit(
                &image,
                rect.x + i32::from(image.info.offset_x),
                rect.y + i32::from(image.info.offset_y),
            );
        } else {
            frame.fill_rect_alpha(
                rect.x,
                rect.y,
                rect.width,
                rect.height,
                object_color(object.kind),
            );
            text.draw_center(
                frame,
                rect,
                labels::conquest_object_name(object.kind, language),
                12,
                [245, 245, 245, 255],
            );
        }
        draw_health_bar(frame, rect, object.hp, object.max_hp, object.owner);
        text.draw_clipped(
            frame,
            rect.x,
            rect.y - 14,
            labels::conquest_object_name(object.kind, language),
            11,
            [255, 236, 180, 255],
            Rect::new(rect.x, rect.y - 14, rect.width, 14),
        );
    }
}

/// 器械血条（怪物头顶样式的简化版）。
fn draw_health_bar(frame: &mut Frame, rect: Rect, hp: i32, max_hp: i32, owner: i32) {
    let ratio = if max_hp > 0 {
        (i64::from(hp.clamp(0, max_hp)) * 100 / i64::from(max_hp)) as u32
    } else {
        0
    };
    let width = rect.width;
    let bar = Rect::new(rect.x, rect.y - 5, width, 4);
    frame.fill_rect(bar.x, bar.y, bar.width, bar.height, [20, 20, 20, 220]);
    let filled = width * ratio / 100;
    frame.fill_rect(bar.x, bar.y, filled, bar.height, owner_color(owner));
}

/// 摧毁时的碎裂粒子（确定性像素，无素材也能看到反馈）。
fn draw_shatter(frame: &mut Frame, rect: Rect, kind: ConquestObjectType) {
    let color = object_color(kind);
    let seed = u32::try_from(rect.x).unwrap_or(0) ^ (u32::try_from(rect.y).unwrap_or(0) << 8);
    for index in 0..8u32 {
        let dx = i32::try_from((seed.wrapping_add(index * 37)) % rect.width.max(1)).unwrap_or(0);
        let dy = i32::try_from((seed.wrapping_add(index * 53)) % rect.height.max(1)).unwrap_or(0);
        let size = 2 + i32::try_from(index % 3).unwrap_or(0);
        frame.fill_rect_alpha(
            rect.x + dx,
            rect.y + dy,
            u32::try_from(size).unwrap_or(2),
            u32::try_from(size).unwrap_or(2),
            color,
        );
    }
}

/// 攻城状态 HUD + 结算横幅。
pub fn draw_hud(
    frame: &mut Frame,
    snapshot: &WorldSnapshot,
    text: &UiText,
    language: Language,
    now_ms: u64,
) {
    let Some(conquest) = &snapshot.conquest else {
        return;
    };
    let screen = Rect::screen(frame.width, frame.height);
    let width = 250.min(screen.width.saturating_sub(16)).max(180);
    let panel = Rect::new(
        screen.right() - i32::try_from(width).unwrap_or(0) - 8,
        screen.y + 54,
        width,
        74,
    );
    if conquest.war_is_on {
        frame.fill_rect_alpha(
            panel.x,
            panel.y,
            panel.width,
            panel.height,
            [12, 16, 24, 210],
        );
        frame.fill_rect(panel.x, panel.y, panel.width, 1, [180, 150, 70, 255]);
        let title = Label::ConquestWar.text(language);
        text.draw_clipped(
            frame,
            panel.x + 8,
            panel.y + 4,
            title,
            13,
            [255, 220, 140, 255],
            Rect::new(panel.x, panel.y, panel.width, 18),
        );
        let remaining = format_remaining(remaining_seconds(conquest, now_ms));
        text.draw_right(
            frame,
            panel.right() - 8,
            panel.y + 4,
            &format!("{} {remaining}", Label::ConquestRemaining.text(language)),
            12,
            [240, 240, 240, 255],
        );
        let owner = if conquest.owner == 0 {
            Label::ConquestUnclaimed.text(language).to_owned()
        } else {
            format!(
                "{} #{}",
                Label::ConquestOwner.text(language),
                conquest.owner
            )
        };
        text.draw_clipped(
            frame,
            panel.x + 8,
            panel.y + 24,
            &owner,
            11,
            owner_color(conquest.owner),
            Rect::new(panel.x, panel.y + 24, panel.width.saturating_sub(16), 16),
        );
        let points = conquest
            .control_points
            .iter()
            .map(|point| point.points)
            .max()
            .unwrap_or(0)
            .clamp(0, MAX_CONTROL_POINTS);
        let progress = format!(
            "{} {points}/{MAX_CONTROL_POINTS}",
            Label::ConquestControlPoints.text(language)
        );
        text.draw_clipped(
            frame,
            panel.x + 8,
            panel.y + 40,
            &progress,
            11,
            [220, 220, 220, 255],
            Rect::new(panel.x, panel.y + 40, panel.width.saturating_sub(16), 14),
        );
        let bar = Rect::new(panel.x + 8, panel.y + 58, panel.width.saturating_sub(16), 6);
        frame.fill_rect(bar.x, bar.y, bar.width, bar.height, [40, 40, 40, 220]);
        let filled = bar.width * u32::try_from(points).unwrap_or(0)
            / u32::try_from(MAX_CONTROL_POINTS).unwrap_or(1);
        frame.fill_rect(bar.x, bar.y, filled, bar.height, [200, 170, 70, 255]);
    }
    if let Some(result) = &conquest.result
        && now_ms.saturating_sub(result.received_ms) < RESULT_BANNER_MS
    {
        let banner_width = 420.min(screen.width.saturating_sub(16)).max(200);
        let banner = Rect::new(
            (screen.width.saturating_sub(banner_width) / 2) as i32,
            screen.y + i32::try_from(screen.height / 3).unwrap_or(0),
            banner_width,
            38,
        );
        frame.fill_rect_alpha(
            banner.x,
            banner.y,
            banner.width,
            banner.height,
            [30, 24, 12, 235],
        );
        frame.fill_rect(banner.x, banner.y, banner.width, 1, [220, 180, 80, 255]);
        let label = if result.winner == 0 {
            Label::ConquestResult.text(language).to_owned()
        } else {
            format!(
                "{} · {} #{}",
                Label::ConquestResult.text(language),
                Label::ConquestWinner.text(language),
                result.winner
            )
        };
        text.draw_center(frame, banner, &label, 15, [255, 230, 160, 255]);
    }
}

/// 兼容旧调用：快照里没有攻城数据时返回 `false`（HUD/器械都不画）。
#[must_use]
pub fn has_conquest(snapshot: &WorldSnapshot) -> bool {
    snapshot.conquest.is_some()
}
