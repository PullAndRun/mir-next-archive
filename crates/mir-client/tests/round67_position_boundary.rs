//! Round 67 B：真实 71/125 链上的位置缓存安全边界（G1.3/G1.4）。
//!
//! 看板子单 `JJC-20261027-004-sub-bd2cae45`（R67-B）。本文件只调用 A 的测试私有参考模型
//! （`#[path = "support/round67_positions.rs"] mod round67_positions;`，**不修改**），把它的
//! 淘汰提案结果当作**历史预热筛选**，再跑真实 `ServerPacket::encode_frame` →
//! `decode_server_packet` → `WorldSnapshot::apply_packet_at` → `UiState::sync` 的现有消费链；
//! 不注册缓存、不接生产、不改 wire：`known_positions` 仍是无界 `HashMap`（生产限容继续
//! PARTIAL，见 `docs/process/round67-plan.md:29-32,89-96`）。
//!
//! ## 夹具代号（M1）
//!
//! 两支测试都用 `WorldSnapshot::default()`（`session_generation == 0` 的手工同代夹具）而不是
//! `new_session()`：`new_session()` 发的代号与本 UI 初始 0 不同，首批 125 会先走
//! [`mir_client::ui::projectile`] 的 `reset_for_new_generation` 清掉刚预热的 `known_positions`，
//! 根本验证不了「历史预热 → 缓存命中」。本文件显式断言 UI 与快照代号都为 0；71 实体与 125
//! 事件仍全部走真实编码/解码入口（非 hand-built 结构体）。
//!
//! ## 非 wire 披露（不称全链闭环）
//!
//! * 「实体从视野消失」是非 wire 夹具：G1.3 的事件流快照只种入源 7（目标 1..=4 不在快照里、
//!   只在预热阶段的历史里）；G1.4 直接 `snapshot.entities.remove` 把目标移出快照。真实 125
//!   没有落点字段（`Shared/ServerPackets.cs:3564-3578`），这里只验证「快照不可见后回退到
//!   现有 `known_positions`」的既有 `consume` 行为，不声称 socket 的 `ObjectRemove` 包已闭环。
//! * G1.3 的「候选历史预热」是**测试模拟**：按参考提案的淘汰结果只预热保留项，不是生产运行时
//!   逐帧 LRU，也不删真实快照事件；预算/ticks 是显式测试输入，非 Crystal 原作行为。
//!
//! ## 断言分工（M2/M3）
//!
//! 参考模型的返回值**不直接断言**（避免「只败在 helper」）：只用它筛选预热历史，失败必须落在
//! 真实 UI 的身份/坐标/cue/游标断言上。隔离副本红证据只改实现、本文件红绿字节不变：
//! m1 去掉换代 `known_positions.clear`、m2 去掉可见缓存刷新（`insert` 不再覆盖旧值）、m3 在
//! support 中忽略保护 ID 1（测试字面量不动）→ G1.3 候选真实 UI 身份 `[4] != [1]`。

#[path = "support/round67_positions.rs"]
mod round67_positions;

use mir_client::net::{ProjectileEvent, WorldSnapshot};
use mir_client::ui::UiState;
use mir_core::Language;
use mir_core::Point;
use mir_core::enums::{MirDirection, Monster, Spell};
use mir_protocol::ServerPacket;
use mir_protocol::packets::server::{ObjectMonster, ObjectProjectile, decode_server_packet};
use round67_positions::plan_evictions;

/// 施法者 object id（125 的未知 source 会被接收路径丢弃，所以必须可见）。
const SOURCE_ID: u32 = 7;
/// 测试统一使用的冻结墙钟毫秒；UI 命中/飞行时钟全程不推进（同刻重复 sync 幂等）。
const FROZEN_MS: i64 = 1_000;
/// 出生音（`GameScene.cs:4697`，`Spell::FireBounce` 的出生 cue）。
const SPAWN_CUE: &str = "M34-1.wav";
/// 真实 71 目标：目标 1..=4 的历史位置（G1.3 的预热坐标 / G1.4 的初始坐标）。
const TARGETS: [(u32, Point); 4] = [
    (1, Point::new(11, 10)),
    (2, Point::new(12, 10)),
    (3, Point::new(13, 10)),
    (4, Point::new(14, 10)),
];

/// 新 UI 状态（中文是默认语言；本文件不断言本地化文本）。
fn ui_state() -> UiState {
    UiState::new(Language::ZhCn)
}

/// 把包编码成真实服务器帧，再走公开协议入口解码（帧布局：`[len u16][index i16][payload]`）。
fn through_wire(packet: &ServerPacket) -> ServerPacket {
    let frame = packet.encode_frame();
    assert!(
        frame.len() > 4,
        "encoded frame must carry a payload: {frame:?}"
    );
    let index = i16::from_le_bytes([frame[2], frame[3]]);
    decode_server_packet(index, &frame[4..]).expect("server frame must decode into ServerPacket")
}

/// 通过真实 `ObjectMonster`(71) wire 路径种入/更新快照里的可见实体（125 的 source 需可解析）。
fn seed_monster(snapshot: &mut WorldSnapshot, object_id: u32, location: Point) {
    let packet = through_wire(&ServerPacket::ObjectMonster(ObjectMonster {
        object_id,
        name: format!("monster-{object_id}"),
        location,
        image: Monster::Skeleton,
        direction: MirDirection::Down,
        ..ObjectMonster::default()
    }));
    let ServerPacket::ObjectMonster(decoded) = &packet else {
        panic!("71 must decode into ServerPacket::ObjectMonster");
    };
    assert_eq!(decoded.object_id, object_id);
    assert_eq!(decoded.location, location, "wire 必须保留实体坐标");
    assert!(!decoded.dead, "种子目标必须存活（否则命中表现被抑制）");
    snapshot.apply_packet_at(&packet, 0);
    assert_eq!(
        snapshot.object_position(object_id),
        Some(location),
        "seed failed: monster {object_id} is not a known snapshot object at {location:?}"
    );
}

/// 按真实接收路径追加一条 125 事件：真实 wire → `apply_packet_at`（表 append + 累计 +1）。
fn push_projectile(snapshot: &mut WorldSnapshot, destination: u32) {
    let packet = through_wire(&ServerPacket::ObjectProjectile(ObjectProjectile {
        spell: Spell::FireBounce,
        source: SOURCE_ID,
        destination,
    }));
    let ServerPacket::ObjectProjectile(decoded) = &packet else {
        panic!("125 must decode into ServerPacket::ObjectProjectile");
    };
    assert_eq!(decoded.spell, Spell::FireBounce);
    assert_eq!(decoded.source, SOURCE_ID);
    assert_eq!(
        decoded.destination, destination,
        "wire 必须保留事件身份（目标编号）"
    );
    let expected = ProjectileEvent {
        spell: Spell::FireBounce,
        source: SOURCE_ID,
        destination,
    };
    let total_before = snapshot.projectile_events_total;
    let len_before = snapshot.projectile_events.len();
    snapshot.apply_packet_at(&packet, 0);
    assert_eq!(
        snapshot.projectile_events.last().copied(),
        Some(expected),
        "接收路径只追加一条事件，不改写已有前缀"
    );
    assert_eq!(
        snapshot.projectile_events.len(),
        len_before + 1,
        "接收路径只 append 一条"
    );
    assert_eq!(
        snapshot.projectile_events_total,
        total_before + 1,
        "累计收到数 +1（本次夹具无回收，非饱和回绕场景）"
    );
}

/// 推进真实快照路径到冻结时钟 `FROZEN_MS`（UI 时钟喂给 `sync`）。
fn sync_at(ui: &mut UiState, snapshot: &WorldSnapshot) {
    ui.set_time(FROZEN_MS);
    ui.sync(snapshot);
}

/// 在飞视图的目标身份顺序（真实 UI 生成顺序 = 事件到达顺序）。
fn view_ids(ui: &UiState) -> Vec<u32> {
    ui.projectiles()
        .views()
        .iter()
        .map(|view| view.destination_id)
        .collect()
}

/// 在飞视图的落点坐标（缓存回退 / 当前位置优先的最终落点）。
fn view_positions(ui: &UiState) -> Vec<Point> {
    ui.projectiles()
        .views()
        .iter()
        .map(|view| view.destination)
        .collect()
}

/// 取走本段新增的出生 cue，断言总数为 `expected` 且全部是出生音（冻结时钟不产生命中音）。
fn assert_spawn_cues(ui: &mut UiState, expected: usize) {
    let cues = ui.take_projectile_sound_cues();
    assert_eq!(
        cues.len(),
        expected,
        "出生 cue 总数（冻结时钟下不应有命中音）: {cues:?}"
    );
    assert!(
        cues.iter().all(|cue| *cue == SPAWN_CUE),
        "只允许出生 cue {SPAWN_CUE}: {cues:?}"
    );
}

/// G1.3：完整历史预热 vs 候选历史预热（按参考提案淘汰 3 个目标）在同一真实 125 流上差分。
///
/// 验证「淘汰历史会少生成」：完整预热四目标全部缓存命中（4 个在飞实体 / 坐标 / 4 cue）；
/// 候选预热只剩目标 1（1 个在飞实体 / (11,10) / 1 cue）；两者都消费 5 条事件（999 无坐标，
/// 不生成、不造坐标）、重复 sync 0 新增。
#[test]
fn r67_historical_preheat_candidate_vs_full() {
    // 模型输入：真实 71 五个对象的 (id, last_used_tick) —— 是显式测试输入，不是生产时钟。
    const ENTRIES: [(u32, u64); 5] = [(SOURCE_ID, 0), (1, 1), (2, 2), (3, 3), (4, 4)];
    /// 显式保护集（源 7 与目标 1）：测试假设，不是生产自动收集的引用集。
    const PROTECTED: [u32; 2] = [SOURCE_ID, 1];
    /// 预算 = 显式测试输入；保留 [7,1]（受保护条目允许超额）。
    const BUDGET: usize = 2;

    // ① 完整历史预热：同代 0 快照种入源 7 与目标 1..=4（真实 71 wire）。
    let mut full_history = WorldSnapshot::default();
    assert_eq!(
        full_history.session_generation, 0,
        "M1：必须用同代 0 夹具（WorldSnapshot::default），不用 new_session()"
    );
    seed_monster(&mut full_history, SOURCE_ID, Point::new(10, 10));
    for (id, position) in TARGETS {
        seed_monster(&mut full_history, id, position);
    }
    let mut full = ui_state();
    sync_at(&mut full, &full_history);
    assert_eq!(
        full.projectiles().watermark_generation(),
        0,
        "UI 初始代号必须与快照同代，否则预热会被换代清掉"
    );
    assert_eq!(full.projectiles().events_consumed(), 0);
    assert_eq!(full.projectiles().views().len(), 0, "预热不产生在飞实体");
    assert_spawn_cues(&mut full, 0);

    // ② 候选历史预热：参考模型只用于筛选保留项（不直接断言其 evict 返回值），
    // 被淘汰目标从未进入该 UI 的快照/缓存。
    let plan = plan_evictions(&ENTRIES, &PROTECTED, BUDGET).expect("唯一 id 输入必须给出提案");
    let kept: Vec<u32> = ENTRIES
        .iter()
        .map(|&(id, _)| id)
        .filter(|id| !plan.evict.contains(id))
        .collect();
    let mut candidate_history = WorldSnapshot::default();
    seed_monster(&mut candidate_history, SOURCE_ID, Point::new(10, 10));
    for (id, position) in TARGETS {
        if kept.contains(&id) {
            seed_monster(&mut candidate_history, id, position);
        }
    }
    let mut candidate = ui_state();
    sync_at(&mut candidate, &candidate_history);
    assert_eq!(
        candidate.projectiles().watermark_generation(),
        0,
        "候选 UI 也必须同代 0"
    );
    assert_eq!(candidate.projectiles().events_consumed(), 0);
    assert_eq!(candidate.projectiles().views().len(), 0);
    assert_spawn_cues(&mut candidate, 0);

    // ③ 真实 125 流：源 7 可见；目标 1..=4 已不可见（非 wire：事件流快照只种源，历史在缓存里）；
    // 999 从未见过。now=1000 冻结。
    let mut stream = WorldSnapshot::default();
    seed_monster(&mut stream, SOURCE_ID, Point::new(10, 10));
    for destination in [1, 2, 3, 4, 999] {
        push_projectile(&mut stream, destination);
    }
    assert_eq!(
        stream.session_generation, 0,
        "事件流与预热必须同代（M1 的 125 不触发换代清缓存）"
    );
    assert_eq!(stream.projectile_events.len(), 5, "五条真实 125 事件");

    // ④ 完整预热：四个目标全部缓存命中 → 四个在飞实体、四个出生 cue；999 无坐标不生成。
    sync_at(&mut full, &stream);
    assert_eq!(
        full.projectiles().views().len(),
        4,
        "完整历史预热：目标 1..=4 必须全部有缓存坐标"
    );
    assert_eq!(
        view_ids(&full),
        vec![1, 2, 3, 4],
        "完整分支真实 UI 身份顺序必须 e1→e4"
    );
    assert_eq!(
        view_positions(&full),
        TARGETS
            .iter()
            .map(|&(_, position)| position)
            .collect::<Vec<_>>(),
        "完整分支落点 = 预热缓存坐标"
    );
    assert!(
        !view_ids(&full).contains(&999),
        "999 从未见：不得造坐标、不得生成"
    );
    assert_eq!(
        full.projectiles().events_consumed(),
        5,
        "五条事件都消费（含 999 无落点不生成）"
    );
    assert_spawn_cues(&mut full, 4);

    // ⑤ 候选预热：淘汰 3 个历史后只有目标 1 能回退 → 1 个在飞实体 / (11,10) / 1 cue。
    sync_at(&mut candidate, &stream);
    assert_eq!(
        view_ids(&candidate),
        vec![1],
        "候选分支真实 UI 身份（m3 变异期望在此红：[4] != [1]）"
    );
    assert_eq!(
        view_positions(&candidate),
        vec![Point::new(11, 10)],
        "候选分支落点 = 保留目标 1 的预热坐标"
    );
    assert_eq!(
        candidate.projectiles().events_consumed(),
        5,
        "候选分支同样消费全部五条事件（无坐标的事件只计入游标）"
    );
    assert_spawn_cues(&mut candidate, 1);

    // ⑥ 重复 sync：两分支都不得新增在飞实体或 cue（同刻幂等）。
    let full_views = full.projectiles().views().len();
    let candidate_views = candidate.projectiles().views().len();
    sync_at(&mut full, &stream);
    sync_at(&mut candidate, &stream);
    assert_eq!(
        full.projectiles().views().len(),
        full_views,
        "重复 sync 不得新增在飞实体（完整分支）"
    );
    assert_eq!(
        candidate.projectiles().views().len(),
        candidate_views,
        "重复 sync 不得新增在飞实体（候选分支）"
    );
    assert_spawn_cues(&mut full, 0);
    assert_spawn_cues(&mut candidate, 0);
    assert_eq!(full.projectiles().events_consumed(), 5);
    assert_eq!(candidate.projectiles().events_consumed(), 5);
}

/// G1.4：可见更新刷新缓存、隐藏后缓存回退、新 generation 首批非空清旧 cache、本代当前位置优先。
///
/// ① 同代预热目标 1=(11,10)；② 真实 71 可见更新到 (19,10) 刷新缓存；③ 移出视野后真实 125
/// 打 1 → 落点回退 (19,10)（m2 变异期望在此红：(11,10)）；④ 新 generation 首批非空（源 7 +
/// 目标 1）→ 旧 cache 必须清掉：view/cue=0、游标=1（m1 变异期望在此红：旧 (19,10) 未清）；
/// ⑤ 本代目标 1 可见更新到 (13,10) + 真实 125 → 落点 (13,10)、cue 1；⑥ 重复 sync 0 新增。
#[test]
fn r67_cache_refresh_and_generation_clear_protection() {
    // ① 同代预热：目标 1 在 (11,10)。generation=0 手工夹具（M1）。
    let mut snapshot = WorldSnapshot::default();
    assert_eq!(
        snapshot.session_generation, 0,
        "M1：必须用同代 0 夹具（WorldSnapshot::default），不用 new_session()"
    );
    seed_monster(&mut snapshot, SOURCE_ID, Point::new(10, 10));
    seed_monster(&mut snapshot, 1, Point::new(11, 10));
    let mut ui = ui_state();
    sync_at(&mut ui, &snapshot);
    assert_eq!(ui.projectiles().watermark_generation(), 0);
    assert_eq!(ui.projectiles().events_consumed(), 0);
    assert_eq!(ui.projectiles().views().len(), 0, "预热不产生在飞实体");
    assert_spawn_cues(&mut ui, 0);

    // ② 可见更新：目标 1 经真实 71 wire 移到 (19,10) → 本帧缓存必须刷新到新位置（M2 可达负控）。
    seed_monster(&mut snapshot, 1, Point::new(19, 10));
    sync_at(&mut ui, &snapshot);
    assert_eq!(
        ui.projectiles().views().len(),
        0,
        "只有可见更新、没有 125：不生成"
    );
    assert_spawn_cues(&mut ui, 0);

    // ③ 隐藏：目标移出快照（非 wire 夹具，已在文件头披露）→ 真实 125 打 1 只能回退缓存。
    snapshot.entities.remove(&1);
    assert_eq!(
        snapshot.object_position(1),
        None,
        "目标已移出视野，仅缓存保留最后位置"
    );
    push_projectile(&mut snapshot, 1);
    sync_at(&mut ui, &snapshot);
    assert_eq!(view_ids(&ui), vec![1], "隐藏后退回缓存生成唯一在飞实体");
    assert_eq!(
        ui.projectiles().views()[0].destination,
        Point::new(19, 10),
        "可见更新必须刷新缓存（m2 变异期望在此红：(11,10) != (19,10)）"
    );
    assert_eq!(ui.projectiles().events_consumed(), 1);
    assert_spawn_cues(&mut ui, 1);

    // ④ 新 generation 首批非空：事件流只种源 7、125 打目标 1；旧代 cache 必须清掉。
    let mut next_gen = WorldSnapshot {
        session_generation: 1,
        ..WorldSnapshot::default()
    };
    seed_monster(&mut next_gen, SOURCE_ID, Point::new(10, 10));
    push_projectile(&mut next_gen, 1);
    sync_at(&mut ui, &next_gen);
    assert_eq!(
        ui.projectiles().watermark_generation(),
        1,
        "换代后 UI 水位代号必须跟随新代"
    );
    let stale: Vec<(u32, Point)> = ui
        .projectiles()
        .views()
        .iter()
        .map(|view| (view.destination_id, view.destination))
        .collect();
    assert!(
        stale.is_empty(),
        "旧 cache 必须已清：不得用旧代坐标 (19,10) 生成，实际在飞视图 {stale:?}（m1 变异期望在此红）"
    );
    assert_eq!(
        ui.projectiles().events_consumed(),
        1,
        "新代首批非空：游标 = 1"
    );
    assert_spawn_cues(&mut ui, 0);

    // ⑤ 本代随后：目标 1 可见更新到 (13,10) + 真实 125 → 当前位置优先，落点 (13,10)、cue 1。
    seed_monster(&mut next_gen, 1, Point::new(13, 10));
    push_projectile(&mut next_gen, 1);
    sync_at(&mut ui, &next_gen);
    assert_eq!(view_ids(&ui), vec![1], "本代只生成目标 1");
    assert_eq!(
        ui.projectiles().views()[0].destination,
        Point::new(13, 10),
        "本代可见位置优先于任何历史缓存"
    );
    assert_eq!(ui.projectiles().events_consumed(), 2, "本代已消费两条事件");
    assert_spawn_cues(&mut ui, 1);

    // ⑥ 重复 sync：同刻幂等，0 新增。
    let base = ui.projectiles().views().len();
    sync_at(&mut ui, &next_gen);
    assert_eq!(
        ui.projectiles().views().len(),
        base,
        "同刻重复 sync 不得新增在飞实体"
    );
    assert_eq!(ui.projectiles().events_consumed(), 2);
    assert_spawn_cues(&mut ui, 0);
}
