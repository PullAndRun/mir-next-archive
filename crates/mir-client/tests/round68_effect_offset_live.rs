//! Round 68 B：真实 71（`ObjectMonster`）/ 125（`ObjectProjectile`）全链下，飞行帧与命中帧
//! 的 `ImageInfo` 有符号偏移经公开绘制入口 `Projectiles::draw` 消费。
//!
//! 链路（门下 C2，参照 `tests/round62_projectile_wire.rs` 范式）：
//! `encode_frame` → `decode_server_packet` → `WorldSnapshot::apply_packet_at` →
//! `UiState::sync` → `Projectiles::draw`。素材用进程唯一临时目录合成的 `magic.wzl/wzx`
//! （450..455 飞、570..579 命中；2×2 非黑不透明 RGB565，offset 逐帧不同）。
//!
//! 覆盖：
//! 1. `r68_g3_real_125_flight_frames_apply_per_frame_offsets`：五格飞行（250 ms / 8 帧 /
//!    31 ms 边界）严格 `NextFrame` 逐 tick 推进，450/451… 逐帧 offset 生效；换 center
//!    平移一致、重复 draw 不累积。
//! 2. `r68_g4_real_570_hit_per_tick_advance_consumes_offsets`：命中 570 起逐 tick 严格
//!    推进到 579（1309 等号不进、1310 才进 571；577..579 各自 offset 生效），越过最后一个
//!    边界才 Remove。
//!
//! 运行：必须带 `--features mir-render/legacy`（合成素材是原始 WZL/WZX，legacy 扫描/
//! 打开路径才编译）：
//!
//! ```text
//! cargo test -p mir-client --features mir-render/legacy --test round68_effect_offset_live
//! ```
//!
//! 缺 feature 时 `effect_size` 为 `None`，第一条断言以诊断文本失败（不是静默 SKIP）。
//! 本文件不断言中文 UI 文案；中文只作为失败诊断。

use std::fs;
use std::path::PathBuf;

use mir_client::net::WorldSnapshot;
use mir_client::ui::UiState;
use mir_client::ui::projectile::{
    HIT_EFFECT_BASE_INDEX, HIT_EFFECT_DURATION_MS, HIT_EFFECT_FRAME_COUNT, HIT_EFFECT_FRAME_MS,
    duration_ms, frame_count,
};
use mir_core::enums::{MirDirection, Monster, Spell};
use mir_core::{Language, Point};
use mir_protocol::ServerPacket;
use mir_protocol::packets::server::{ObjectMonster, ObjectProjectile, decode_server_packet};
use mir_render::{
    CELL_HEIGHT, CELL_WIDTH, EntityLibraries, Frame, FxKind, MapScene, TileLibraries, Viewport,
};

/// 出生墙钟毫秒（固定 tick）。
const START_MS: i64 = 1_000;
/// 源对象 id（真实 71 包写入）。
const SOURCE_ID: u32 = 7;
/// 目标对象 id（真实 71 包写入）。
const DEST_ID: u32 = 8;
/// 摄像中心格（`Projectiles::draw` 的 center）。
fn center() -> Point {
    Point::new(10, 10)
}
/// 源格。
fn source_point() -> Point {
    Point::new(10, 10)
}
/// 目标格（同一行向右 5 格 → Direction16 = 4，飞行帧 450 起）。
fn target_point() -> Point {
    Point::new(15, 10)
}

/// 合成帧边长（2×2）。
const FRAME_SIDE: u16 = 2;

/// 合成 `magic.wzl` 帧表：帧号 + 帧头有符号偏移 + 2×2 RGB565 四角原始值。
/// 450/451 与 570/571 的 offset 由门下 C1 钉死，其余帧互不相同以便逐帧区分。
const MAGIC_FRAMES: [MagicFrame; 16] = [
    MagicFrame::new(450, (-3, 5), [0xF800, 0xFFE0, 0x07E0, 0x001F]),
    MagicFrame::new(451, (4, -2), [0x001F, 0xF800, 0xFFFF, 0xFFE0]),
    MagicFrame::new(452, (-2, -3), [0x07E0, 0x001F, 0xF800, 0xFFFF]),
    MagicFrame::new(453, (3, 4), [0xFFFF, 0x07E0, 0xFFE0, 0xF800]),
    MagicFrame::new(454, (1, -1), [0xFFE0, 0x07FF, 0x8410, 0xF81F]),
    MagicFrame::new(455, (-4, 2), [0x8410, 0xF81F, 0x07FF, 0xFFE0]),
    MagicFrame::new(570, (-2, -3), [0x07E0, 0x001F, 0xF800, 0xFFFF]),
    MagicFrame::new(571, (3, 4), [0xFFFF, 0x07E0, 0xFFE0, 0xF800]),
    MagicFrame::new(572, (-4, 1), [0xF800, 0x07FF, 0x001F, 0xFFE0]),
    MagicFrame::new(573, (2, -5), [0x07FF, 0xFFE0, 0xF800, 0x001F]),
    MagicFrame::new(574, (-3, -2), [0xFFE0, 0xF81F, 0x07E0, 0xFFFF]),
    MagicFrame::new(575, (0, 0), [0x8410, 0x07E0, 0x001F, 0xF800]),
    MagicFrame::new(576, (1, 3), [0x07E0, 0x8410, 0xFFFF, 0xF800]),
    MagicFrame::new(577, (-5, 2), [0xF81F, 0xF800, 0x07E0, 0x001F]),
    MagicFrame::new(578, (4, -4), [0x001F, 0xFFE0, 0x8410, 0x07FF]),
    MagicFrame::new(579, (2, 2), [0xFFFF, 0x001F, 0x8410, 0x07E0]),
];

#[derive(Clone, Copy)]
struct MagicFrame {
    index: u32,
    offset: (i16, i16),
    quad: [u16; 4],
}

impl MagicFrame {
    const fn new(index: u32, offset: (i16, i16), quad: [u16; 4]) -> Self {
        Self {
            index,
            offset,
            quad,
        }
    }
}

/// 进程唯一临时目录（不读 `resource`/`game-assets`，也不与其它测试共享路径）。
///
/// 优先系统临时目录；Windows 上 `TEMP/TMP` 可能被环境策略拒绝写（`PermissionDenied`），
/// 此时回退到仓库内允许的 `target/round68/b/tmp/`（测试产物目录，不入库）。
fn unique_dir(tag: &str) -> PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |duration| duration.as_nanos());
    let name = format!("mir-next-r68-live-{tag}-{}-{nanos}", std::process::id());
    let candidates = [
        std::env::temp_dir().join(&name),
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/round68/b/tmp")
            .join(&name),
    ];
    for dir in candidates {
        if fs::create_dir_all(&dir).is_ok() {
            return dir;
        }
    }
    panic!("无法创建进程唯一临时目录（系统临时目录与 target/round68/b/tmp 均不可写）");
}

/// 写合成 `magic.wzl` + `magic.wzx` 并构造带该实体图库的地图场景。
fn scene_with_magic(tag: &str) -> (MapScene, PathBuf) {
    let dir = unique_dir(tag);
    let (wzl, wzx) = build_wzl(&MAGIC_FRAMES);
    fs::write(dir.join("magic.wzl"), wzl).expect("写合成 magic.wzl");
    fs::write(dir.join("magic.wzx"), wzx).expect("写合成 magic.wzx");
    let map = mir_world::Map {
        width: 32,
        height: 32,
        cells: vec![mir_world::MapCell::default(); 32 * 32],
        file_name: "r68-empty".to_owned(),
        version: mir_world::MapVersion::V0,
        doors: Vec::new(),
    };
    let scene = MapScene::from_parts_with_entities(
        map,
        TileLibraries::empty("."),
        Viewport::DEFAULT,
        EntityLibraries::open_dir(&dir),
    );
    (scene, dir)
}

/// 按 `.wzl`/`.wzx` 二进制格式构造一对文件内容（与 `WzlReader` 的读入格式一致）。
fn build_wzl(frames: &[MagicFrame]) -> (Vec<u8>, Vec<u8>) {
    let count = frames
        .iter()
        .map(|frame| frame.index)
        .max()
        .expect("合成帧表不能为空")
        + 1;
    let mut wzl = vec![0u8; 48];
    let mut table = vec![0u32; usize::try_from(count).expect("帧数必须放得进 usize")];
    for spec in frames {
        let mut raw = Vec::with_capacity(usize::from(FRAME_SIDE) * usize::from(FRAME_SIDE) * 2);
        // 行序按 Crystal 自下而上：文件第 0 行 = 图像最下面一行。
        for value in [spec.quad[2], spec.quad[3], spec.quad[0], spec.quad[1]] {
            raw.extend_from_slice(&value.to_le_bytes());
        }
        let compressed = zlib_store(&raw);
        table[usize::try_from(spec.index).expect("帧号必须放得进 usize")] =
            u32::try_from(wzl.len()).expect(".wzl 偏移必须放得进 u32");
        wzl.push(5); // 格式 5 = RGB565
        wzl.extend_from_slice(&[0, 0, 0]); // Crystal 跳过的 3 个保留字节
        wzl.extend_from_slice(&FRAME_SIDE.to_le_bytes());
        wzl.extend_from_slice(&FRAME_SIDE.to_le_bytes());
        wzl.extend_from_slice(&spec.offset.0.to_le_bytes());
        wzl.extend_from_slice(&spec.offset.1.to_le_bytes());
        wzl.extend_from_slice(
            &u32::try_from(compressed.len())
                .expect("压缩长度必须放得进 u32")
                .to_le_bytes(),
        );
        wzl.extend_from_slice(&compressed);
    }
    let mut wzx = vec![0u8; 48];
    wzx[44..48].copy_from_slice(&count.to_le_bytes());
    for offset in table {
        wzx.extend_from_slice(&offset.to_le_bytes());
    }
    (wzl, wzx)
}

/// zlib 流：DEFLATE 存储块（BTYPE=0），不新增依赖；`zlib` 校验尾用 Adler-32。
fn zlib_store(payload: &[u8]) -> Vec<u8> {
    let mut out = vec![0x78, 0x01];
    let mut chunks = payload.chunks(usize::from(u16::MAX)).peekable();
    while let Some(chunk) = chunks.next() {
        let final_block = chunks.peek().is_none();
        out.push(u8::from(final_block));
        let len = u16::try_from(chunk.len()).expect("存储块长度 <= u16::MAX");
        out.extend_from_slice(&len.to_le_bytes());
        out.extend_from_slice(&(!len).to_le_bytes());
        out.extend_from_slice(chunk);
    }
    out.extend_from_slice(&adler32(payload).to_be_bytes());
    out
}

fn adler32(data: &[u8]) -> u32 {
    const MOD: u32 = 65_521;
    let (mut low, mut high) = (1u32, 0u32);
    for &byte in data {
        low = (low + u32::from(byte)) % MOD;
        high = (high + low) % MOD;
    }
    (high << 16) | low
}

/// 帧表查询（缺帧号 panic，防止 fixture 与断言漂移）。
fn magic_frame(index: u32) -> &'static MagicFrame {
    MAGIC_FRAMES
        .iter()
        .find(|frame| frame.index == index)
        .expect("合成 magic.wzl 帧表必须覆盖该帧号")
}

/// Crystal `convert16bitTo32bit` 的独立 oracle（只左移，不复制低位）。
fn expand565(value: u16) -> [u8; 4] {
    let red = ((value >> 11) & 0x1F) << 3;
    let green = ((value >> 5) & 0x3F) << 2;
    let blue = (value & 0x1F) << 3;
    [red as u8, green as u8, blue as u8, 255]
}

/// 2×2 四角放到 `top_left` 后的落屏像素列表（左上、右上、左下、右下）。
fn place_quad(top_left: (i32, i32), quad: [u16; 4]) -> Vec<(i32, i32, [u8; 4])> {
    let mut placed = Vec::with_capacity(quad.len());
    for (slot, value) in quad.iter().enumerate() {
        let column = i32::try_from(slot % 2).expect("列号");
        let row = i32::try_from(slot / 2).expect("行号");
        placed.push((top_left.0 + column, top_left.1 + row, expand565(*value)));
    }
    placed
}

/// 独立复算 Crystal `Effect.cs:206-212` 的飞行偏移：
/// `(Destination - Source) * (48,32) * frame / count`，C# 整数除法向零截断。
fn flight_motion(source: Point, destination: Point, frame: u32, count: u32) -> (i32, i32) {
    let count = count.max(1);
    let frame = frame.min(count);
    let dx = (i64::from(destination.x) - i64::from(source.x)) * i64::from(CELL_WIDTH);
    let dy = (i64::from(destination.y) - i64::from(source.y)) * i64::from(CELL_HEIGHT);
    let offset_x = dx * i64::from(frame) / i64::from(count);
    let offset_y = dy * i64::from(frame) / i64::from(count);
    (clamp_i32(offset_x), clamp_i32(offset_y))
}

fn clamp_i32(value: i64) -> i32 {
    i32::try_from(value).unwrap_or(if value < 0 { i32::MIN } else { i32::MAX })
}

/// 期望落屏：锚点 + 飞行偏移（一次）+ 该帧 `ImageInfo` 偏移。
fn expected_effect(origin: (i32, i32), motion: (i32, i32), index: u32) -> Vec<(i32, i32, [u8; 4])> {
    let spec = magic_frame(index);
    let top_left = (
        origin.0 + motion.0 + i32::from(spec.offset.0),
        origin.1 + motion.1 + i32::from(spec.offset.1),
    );
    place_quad(top_left, spec.quad)
}

/// 断言帧上恰好只有期望的像素（位置 + RGBA 全等；数量相等保证没有多画/漏画）。
fn assert_frame_equals(frame: &Frame, placed: &[(i32, i32, [u8; 4])]) {
    let mut visible = 0usize;
    for &(x, y, color) in placed {
        if x < 0 || y < 0 {
            continue;
        }
        let (Ok(xu), Ok(yu)) = (u32::try_from(x), u32::try_from(y)) else {
            continue;
        };
        if xu >= frame.width || yu >= frame.height {
            continue;
        }
        visible += 1;
        assert_eq!(
            frame.pixel(xu, yu),
            Some(color),
            "帧像素不一致 @({x},{y})（投影/运动/帧头偏移必须各叠加一次）"
        );
    }
    let lit = frame
        .pixels
        .chunks_exact(4)
        .filter(|pixel| pixel[3] == 255)
        .count();
    assert_eq!(
        lit, visible,
        "落屏像素数必须恰为 {visible}（不得多画/漏画/残留）"
    );
}

/// 整帧期望缓冲（`Frame::new` 的其余像素保持透明黑 0）。
fn expected_frame_pixels(width: u32, height: u32, placed: &[(i32, i32, [u8; 4])]) -> Vec<u8> {
    let width_usize = usize::try_from(width).expect("宽");
    let height_usize = usize::try_from(height).expect("高");
    let mut pixels = vec![0u8; width_usize * height_usize * 4];
    for &(x, y, color) in placed {
        let (Ok(xu), Ok(yu)) = (u32::try_from(x), u32::try_from(y)) else {
            continue;
        };
        if xu >= width || yu >= height {
            continue;
        }
        let row = usize::try_from(yu).expect("行");
        let column = usize::try_from(xu).expect("列");
        let offset = (row * width_usize + column) * 4;
        pixels[offset..offset + 4].copy_from_slice(&color);
    }
    pixels
}

/// FNV-1a 像素摘要（失败信息只打印摘要，避免整帧缓冲刷屏）。
fn pixel_digest(frame: &Frame) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for &byte in &frame.pixels {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

fn assert_same_pixels(left: &Frame, right: &Frame, context: &str) {
    assert_eq!(
        (left.width, left.height),
        (right.width, right.height),
        "{context}：帧尺寸必须一致"
    );
    // 摘要只用于失败时给出短信息；真正的相等性必须比较 RGBA 字节切片本身。
    assert_eq!(
        pixel_digest(left),
        pixel_digest(right),
        "{context}：像素摘要不同"
    );
    assert_eq!(
        left.pixels, right.pixels,
        "{context}：像素尺寸与 RGBA 字节必须完全一致（重复绘制/平移不得累积或漂移）"
    );
}

/// 新 UI 状态（中文默认语言；本文件不断言本地化文本）。
fn ui_state() -> UiState {
    UiState::new(Language::ZhCn)
}

/// 真实 71：`encode_frame` → `decode_server_packet` → `apply_packet_at` 写入对象。
fn seed_monster_via_wire(snapshot: &mut WorldSnapshot, object_id: u32, location: Point) {
    let packet = through_wire(&ServerPacket::ObjectMonster(ObjectMonster {
        object_id,
        name: format!("monster-{object_id}"),
        location,
        image: Monster::Skeleton,
        direction: MirDirection::Down,
        dead: false,
        ..ObjectMonster::default()
    }));
    snapshot.apply_packet_at(&packet, 0);
    assert_eq!(
        snapshot.object_position(object_id),
        Some(location),
        "71 全链写入失败: monster {object_id} @ {location:?}"
    );
}

/// 真实 125：`encode_frame` → `decode_server_packet` → `apply_packet_at`。
fn push_projectile_via_wire(
    snapshot: &mut WorldSnapshot,
    source: u32,
    destination: u32,
    now_ms: u64,
) {
    let packet = through_wire(&ServerPacket::ObjectProjectile(ObjectProjectile {
        spell: Spell::FireBounce,
        source,
        destination,
    }));
    snapshot.apply_packet_at(&packet, now_ms);
}

/// 真实服务器帧（`[len u16][index i16][payload]`）→ 公开协议解码入口。
fn through_wire(packet: &ServerPacket) -> ServerPacket {
    let frame = packet.encode_frame();
    assert!(
        frame.len() > 4,
        "encoded frame must carry a payload: {frame:?}"
    );
    let index = i16::from_le_bytes([frame[2], frame[3]]);
    decode_server_packet(index, &frame[4..]).expect("server frame must decode into ServerPacket")
}

/// 推进真实快照路径到 `now_ms`（UI 时钟喂给 `projectiles.sync`）。
fn sync_at(ui: &mut UiState, snapshot: &WorldSnapshot, now_ms: i64) {
    ui.time_millis = now_ms;
    ui.sync(snapshot);
}

/// 五格飞行的严格边界 tick（duration 250 / count 8 / interval 31）。
const FIVE_CELL_TICKS: [i64; 8] = [32, 63, 94, 125, 156, 187, 218, 249];

/// 预检：合成图库确实被真实读取链看到（缺 legacy feature 时这里给出明确诊断）。
fn assert_fixture_readable(scene: &mut MapScene) {
    assert_eq!(
        scene
            .effect_size(FxKind::Magic, 450)
            .expect("合成 magic.wzl[450] 必须可读；请加 --features mir-render/legacy"),
        (u32::from(FRAME_SIDE), u32::from(FRAME_SIDE)),
        "合成帧必须是 2×2"
    );
    for frame in HIT_EFFECT_BASE_INDEX..HIT_EFFECT_BASE_INDEX + HIT_EFFECT_FRAME_COUNT {
        assert!(
            scene.effect_size(FxKind::Magic, frame).is_some(),
            "合成 magic.wzl[{frame}] 必须可读（命中逐帧 offset 用例依赖全部 570..=579）"
        );
    }
}

/// G3：真实 125 五格飞行的每一帧都把该帧 `ImageInfo` 偏移叠加一次（且只叠加一次）。
#[test]
fn r68_g3_real_125_flight_frames_apply_per_frame_offsets() {
    let (mut scene, dir) = scene_with_magic("g3");
    assert_fixture_readable(&mut scene);

    let source = source_point();
    let target = target_point();
    let mut snapshot = WorldSnapshot::new_session();
    seed_monster_via_wire(&mut snapshot, SOURCE_ID, source);
    seed_monster_via_wire(&mut snapshot, DEST_ID, target);
    push_projectile_via_wire(&mut snapshot, SOURCE_ID, DEST_ID, 0);

    // 独立 oracle 的飞行常量：五格 × 50 ms/格 = 250 ms；帧数 250/30 = 8；边界 250/8 = 31。
    let expected_duration = 5 * 50u64;
    assert_eq!(
        duration_ms(source, target),
        expected_duration,
        "G3：飞行时长"
    );
    let expected_count = 8u32;
    assert_eq!(
        frame_count(expected_duration),
        expected_count,
        "G3：飞行帧数"
    );
    let interval =
        i64::try_from(expected_duration / u64::from(expected_count)).expect("31 ms fits i64");
    assert_eq!(interval, 31, "G3：帧边界");

    let mut ui = ui_state();
    sync_at(&mut ui, &snapshot, START_MS);
    assert_eq!(ui.projectiles().events_consumed(), 1, "G3：125 出生恰一次");
    assert_eq!(ui.projectiles().len(), 1, "G3：真实 125 必须生成飞行视图");
    {
        let view = ui.projectiles().views()[0];
        assert_eq!(view.spell, Spell::FireBounce);
        assert_eq!(view.source, source, "G3：飞行起点 = 源对象位置");
        assert_eq!(view.destination, target, "G3：飞行终点 = 目标对象位置");
        assert_eq!(view.destination_id, DEST_ID);
        assert_eq!(view.started_ms, START_MS);
        assert_eq!(view.frame, 0);
        assert_eq!(view.frame_count, expected_count);
        assert_eq!(view.next_frame_ms, START_MS + interval);
        // 450 = Magic 起始帧 + 右向（Direction16 = 4）× 10。
        assert_eq!(view.sprite_index(), 450, "G3：首帧必须是 450");
    }

    let origin = scene.world_to_screen(center(), source);
    assert_eq!(
        ui.projectiles().views()[0].draw_location(origin),
        origin,
        "G3：frame 0 的运动偏移必须为 0"
    );

    // 严格 NextFrame：逐 tick 只过一帧边界（禁单次大跳伪到达），每帧整帧像素断言。
    for (tick_index, elapsed) in FIVE_CELL_TICKS.iter().enumerate() {
        let now = START_MS + elapsed;
        sync_at(&mut ui, &snapshot, now);
        if tick_index + 1 == FIVE_CELL_TICKS.len() {
            // 249 ms 跨过最后一个边界：飞行视图被删除、生成 570 命中残影。
            assert!(
                ui.projectiles().is_empty(),
                "G3：到达 tick 必须删除飞行视图（tick={elapsed}）"
            );
            let effects = ui.projectiles().hit_effects();
            assert_eq!(effects.len(), 1, "G3：到达必须生成一条命中残影");
            assert_eq!(effects[0].position, target, "G3：残影跟随目标格");
            assert_eq!(
                effects[0].sprite_index(now),
                HIT_EFFECT_BASE_INDEX,
                "G3：到达首帧必须是 570"
            );
            break;
        }
        let expected_frame = u32::try_from(tick_index + 1).expect("帧号");
        let view = ui.projectiles().views()[0];
        assert_eq!(
            view.frame, expected_frame,
            "G3：tick {elapsed} 必须恰进一帧"
        );
        assert_eq!(
            view.next_frame_ms,
            START_MS + interval * i64::from(expected_frame + 1),
            "G3：严格 NextFrame 边界"
        );
        assert_eq!(
            view.sprite_index(),
            450 + expected_frame % 6,
            "G3：450/451… 逐帧帧号（每个方向 6 帧后回绕）"
        );
        let mut canvas = Frame::new(Viewport::DEFAULT.width, Viewport::DEFAULT.height);
        ui.projectiles()
            .draw(&mut canvas, &mut scene, center(), now);
        let motion = flight_motion(source, target, expected_frame, expected_count);
        assert_frame_equals(
            &canvas,
            &expected_effect(origin, motion, view.sprite_index()),
        );
        // 450/451 的帧头 offset 不同：同运动偏移下像素必须不同（逐帧消费的直接证据）。
        if expected_frame == 1 {
            let swapped_top_left = (
                origin.0 + motion.0 + i32::from(magic_frame(450).offset.0),
                origin.1 + motion.1 + i32::from(magic_frame(450).offset.1),
            );
            let mut swapped = Frame::new(Viewport::DEFAULT.width, Viewport::DEFAULT.height);
            swapped.pixels = expected_frame_pixels(
                swapped.width,
                swapped.height,
                &place_quad(swapped_top_left, magic_frame(450).quad),
            );
            assert_ne!(
                pixel_digest(&canvas),
                pixel_digest(&swapped),
                "G3：451 与 450 的帧头 offset 不同，同样锚点/运动下像素必须不同"
            );

            // 飞行态（view 非空、运动偏移非零）同样必须满足：重复 draw 不累积、
            // 换 center 时屏幕位置整体平移（运动偏移/帧头 offset 各只叠加一次）。
            let mut twice = Frame::new(Viewport::DEFAULT.width, Viewport::DEFAULT.height);
            ui.projectiles().draw(&mut twice, &mut scene, center(), now);
            ui.projectiles().draw(&mut twice, &mut scene, center(), now);
            assert_same_pixels(&canvas, &twice, "G3：飞行帧（frame 1）重复 draw 不得累积");
            let flight_center_b = Point::new(15, 10);
            let flight_origin_b = scene.world_to_screen(flight_center_b, source);
            let mut flight_frame_b = Frame::new(Viewport::DEFAULT.width, Viewport::DEFAULT.height);
            ui.projectiles()
                .draw(&mut flight_frame_b, &mut scene, flight_center_b, now);
            assert_frame_equals(
                &flight_frame_b,
                &expected_effect(flight_origin_b, motion, view.sprite_index()),
            );
            assert_eq!(
                flight_origin_b.0 - origin.0,
                (center().x - flight_center_b.x) * CELL_WIDTH,
                "G3：飞行态换 center 的屏幕平移必须等于格差 × 48（符号与 geometry world_to_screen 一致）"
            );
        }
    }

    // 换 center：同一帧的屏幕位置必须整体平移（偏移只叠加一次，不随 center 重算）。
    let arrival = START_MS + 249;
    let mut frame_a = Frame::new(Viewport::DEFAULT.width, Viewport::DEFAULT.height);
    ui.projectiles()
        .draw(&mut frame_a, &mut scene, center(), arrival);
    let hit_origin = scene.world_to_screen(center(), target);
    assert_frame_equals(
        &frame_a,
        &expected_effect(hit_origin, (0, 0), HIT_EFFECT_BASE_INDEX),
    );

    let center_b = Point::new(15, 10);
    let origin_b = scene.world_to_screen(center_b, target);
    let mut frame_b = Frame::new(Viewport::DEFAULT.width, Viewport::DEFAULT.height);
    ui.projectiles()
        .draw(&mut frame_b, &mut scene, center_b, arrival);
    assert_frame_equals(
        &frame_b,
        &expected_effect(origin_b, (0, 0), HIT_EFFECT_BASE_INDEX),
    );
    assert_eq!(
        origin_b.0 - hit_origin.0,
        (center().x - center_b.x) * CELL_WIDTH,
        "G3：换 center 的屏幕平移必须等于格差 × 48（符号与 geometry world_to_screen 一致）"
    );

    // 重复绘制不累积：同一帧画两次，像素逐字节一致。
    let mut frame_twice = Frame::new(Viewport::DEFAULT.width, Viewport::DEFAULT.height);
    ui.projectiles()
        .draw(&mut frame_twice, &mut scene, center(), arrival);
    ui.projectiles()
        .draw(&mut frame_twice, &mut scene, center(), arrival);
    assert_same_pixels(&frame_a, &frame_twice, "G3：命中残影重复 draw 不得累积");

    let _ = fs::remove_dir_all(&dir);
}

/// G4：真实 570 命中残影逐 tick 严格推进，577..579 各自帧头 offset 生效。
#[test]
fn r68_g4_real_570_hit_per_tick_advance_consumes_offsets() {
    let (mut scene, dir) = scene_with_magic("g4");
    assert_fixture_readable(&mut scene);

    let source = source_point();
    let target = target_point();
    let mut snapshot = WorldSnapshot::new_session();
    seed_monster_via_wire(&mut snapshot, SOURCE_ID, source);
    seed_monster_via_wire(&mut snapshot, DEST_ID, target);
    push_projectile_via_wire(&mut snapshot, SOURCE_ID, DEST_ID, 0);

    let mut ui = ui_state();
    sync_at(&mut ui, &snapshot, START_MS);
    assert_eq!(
        ui.projectiles().len(),
        1,
        "G4：固定 tick {START_MS} 出生必须恰一条"
    );

    // 飞行段：逐 tick 严格推进（禁单次大跳伪到达），命中帧 450..455 已由 G3 钉死，
    // 这里只做到达断言，把 570 残影留在 249 ms 边界。
    for (tick_index, elapsed) in FIVE_CELL_TICKS.iter().enumerate() {
        let now = START_MS + elapsed;
        sync_at(&mut ui, &snapshot, now);
        if tick_index + 1 == FIVE_CELL_TICKS.len() {
            assert!(
                ui.projectiles().is_empty(),
                "G4：1249 必须到达（删除飞行视图）"
            );
            break;
        }
        let expected_frame = u32::try_from(tick_index + 1).expect("帧号");
        assert_eq!(
            ui.projectiles().views()[0].frame,
            expected_frame,
            "G4：到达前每个 tick 恰进一帧（tick={elapsed}）"
        );
        assert_eq!(
            ui.projectiles().views()[0].sprite_index(),
            450 + expected_frame % 6,
            "G4：450/451… 严格帧号"
        );
    }

    let arrival = START_MS + 249;
    let effects = ui.projectiles().hit_effects();
    assert_eq!(effects.len(), 1, "G4：1249 必须生成 570 残影");
    assert_eq!(effects[0].target_id, DEST_ID);
    assert_eq!(effects[0].position, target);
    assert_eq!(effects[0].started_ms, arrival);
    assert_eq!(
        effects[0].sprite_index(arrival),
        HIT_EFFECT_BASE_INDEX,
        "G4：命中首帧 570"
    );

    // 命中段：期望 = 目标格屏幕原点 + 当帧 ImageInfo 偏移（不动，无运动项）。
    let hit_origin = scene.world_to_screen(center(), target);
    let frame_ms = i64::try_from(HIT_EFFECT_FRAME_MS).expect("60 ms fits i64");
    let mut canvas = Frame::new(Viewport::DEFAULT.width, Viewport::DEFAULT.height);
    ui.projectiles()
        .draw(&mut canvas, &mut scene, center(), arrival);
    assert_frame_equals(
        &canvas,
        &expected_effect(hit_origin, (0, 0), HIT_EFFECT_BASE_INDEX),
    );

    // M3 精确边界：1249 + 60 = 1309 等号不进帧；1310 才进 571。
    let equality_tick = arrival + frame_ms;
    assert_eq!(equality_tick, 1_309, "G4：等号边界必须是 1309");
    sync_at(&mut ui, &snapshot, equality_tick);
    assert_eq!(
        ui.projectiles().hit_effects()[0].sprite_index(equality_tick),
        HIT_EFFECT_BASE_INDEX,
        "G4：1309 等号必须仍画 570（严格 NextFrame）"
    );
    let mut canvas_1309 = Frame::new(Viewport::DEFAULT.width, Viewport::DEFAULT.height);
    ui.projectiles()
        .draw(&mut canvas_1309, &mut scene, center(), equality_tick);
    assert_frame_equals(
        &canvas_1309,
        &expected_effect(hit_origin, (0, 0), HIT_EFFECT_BASE_INDEX),
    );

    let next_tick = equality_tick + 1;
    assert_eq!(next_tick, 1_310, "G4：1310 才进下一帧");
    sync_at(&mut ui, &snapshot, next_tick);
    assert_eq!(
        ui.projectiles().hit_effects()[0].sprite_index(next_tick),
        HIT_EFFECT_BASE_INDEX + 1,
        "G4：1310 必须进 571"
    );

    // 逐帧 tick：571..=579 每帧都必须把自己的帧头 offset 叠加一次。
    for frame in 1..HIT_EFFECT_FRAME_COUNT {
        let now = arrival + i64::from(frame) * frame_ms + 1;
        sync_at(&mut ui, &snapshot, now);
        let effects = ui.projectiles().hit_effects();
        assert_eq!(effects.len(), 1, "G4：残影必须持续到越过最后一个边界");
        let index = effects[0].sprite_index(now);
        assert_eq!(
            index,
            HIT_EFFECT_BASE_INDEX + frame,
            "G4：frame {frame} 必须是 {}",
            HIT_EFFECT_BASE_INDEX + frame
        );
        assert_eq!(
            effects[0].position, target,
            "G4：残影位置必须跟随目标（目标存活不动）"
        );
        let mut canvas = Frame::new(Viewport::DEFAULT.width, Viewport::DEFAULT.height);
        ui.projectiles()
            .draw(&mut canvas, &mut scene, center(), now);
        assert_frame_equals(&canvas, &expected_effect(hit_origin, (0, 0), index));
    }

    // 最后一个边界（1249 + 600 = 1849）等号仍画 579，越过才删除。
    let last_boundary = arrival + i64::try_from(HIT_EFFECT_DURATION_MS).expect("600 ms fits i64");
    assert_eq!(last_boundary, 1_849, "G4：最后一个边界必须是 1849");
    sync_at(&mut ui, &snapshot, last_boundary);
    assert_eq!(
        ui.projectiles().hit_effects().len(),
        1,
        "G4：1849 等号不得删除残影"
    );
    assert_eq!(
        ui.projectiles().hit_effects()[0].sprite_index(last_boundary),
        HIT_EFFECT_BASE_INDEX + HIT_EFFECT_FRAME_COUNT - 1,
        "G4：1849 仍画 579"
    );
    let mut canvas_last = Frame::new(Viewport::DEFAULT.width, Viewport::DEFAULT.height);
    ui.projectiles()
        .draw(&mut canvas_last, &mut scene, center(), last_boundary);
    assert_frame_equals(
        &canvas_last,
        &expected_effect(
            hit_origin,
            (0, 0),
            HIT_EFFECT_BASE_INDEX + HIT_EFFECT_FRAME_COUNT - 1,
        ),
    );
    sync_at(&mut ui, &snapshot, last_boundary + 1);
    assert!(
        ui.projectiles().hit_effects().is_empty(),
        "G4：越过 1849 才删除残影"
    );

    let _ = fs::remove_dir_all(&dir);
}
