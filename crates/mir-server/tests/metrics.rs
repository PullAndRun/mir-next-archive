//! `/metrics` 端点的端到端测试：真起 listener、发一次 HTTP GET、断言字段。
//!
//! 指标结构本身的单测在 `src/metrics.rs` 的 `#[cfg(test)]` 里（计数/环形窗口/分位）。

use std::sync::Arc;
use std::time::Duration;

use mir_core::Point;
use mir_core::enums::{AttackMode, MirClass, MirDirection, MirGender};
use mir_server::combat::{self, CombatStats};
use mir_server::items;
use mir_server::world::Player;
use mir_server::{MagicState, NpcState, TradeState};

use mir_server::metrics::{Metrics, Ring, bind_metrics, metrics};
use mir_server::world::World;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::sync::Mutex;

/// 往世界里放一个确定性的玩家（只用到 `world.enter`，不进网络）。
fn enter_player(world: &mut World, object_id: u32, map: &str) {
    let base: CombatStats = combat::player_base_stats(10, MirClass::Warrior);
    let (sender, _receiver) = tokio::sync::mpsc::unbounded_channel();
    world.enter(Player {
        object_id,
        session_id: u64::from(object_id),
        character_id: i64::from(object_id),
        name: format!("player{object_id}"),
        map: map.to_owned(),
        position: Point::new(10, 10),
        direction: MirDirection::Down,
        level: 10,
        experience: 0,
        max_experience: 0,
        class: MirClass::Warrior,
        gender: MirGender::Male,
        hair: 1,
        hp: base.hp,
        mp: base.mp,
        stats: base,
        action_time: 0,
        attack_time: 0,
        struck_time: 0,
        regen_time: i64::MAX,
        death_time: 0,
        revive_time: 0,
        cell_time: 0,
        step_counter: 0,
        run_counter: 0,
        run_time: 0,
        dead: false,
        attack_mode: AttackMode::Peace,
        pk_points: 0,
        brown_time: 0,
        pk_point_time: 0,
        last_hitter: None,
        last_hit_time: 0,
        name_colour: mir_core::Color(-1),
        visible_monsters: std::collections::BTreeSet::new(),
        visible_npcs: std::collections::BTreeSet::new(),
        visible_items: std::collections::BTreeSet::new(),
        movement_time: 0,
        inventory: items::new_inventory(),
        equipment: items::new_equipment(),
        storage: mir_server::storage::new_storage(),
        magics: Vec::new(),
        magic: MagicState::default(),
        gold: 0,
        credit: 0,
        quests: Vec::new(),
        completed_quests: std::collections::BTreeSet::new(),
        mail: mir_server::MailState::default(),
        buffs: Vec::new(),
        pot_health_amount: 0,
        pot_mana_amount: 0,
        pot_time: 0,
        unlock_curse: false,
        npc: NpcState::default(),
        group_id: None,
        group_invitation: None,
        group_invite_time: 0,
        allow_group: true,
        guild: None,
        trade: TradeState::default(),
        trade_invitation: None,
        next_trade_time: 0,
        allow_trade: true,
        bind_map_index: 0,
        bind_point: mir_core::Point::default(),
        sender,
    });
}

/// 发一次最简 HTTP GET，返回 `(状态行, body)`。
async fn http_get(address: std::net::SocketAddr, path: &str) -> (String, String) {
    let mut stream = TcpStream::connect(address).await.expect("连 metrics 端口");
    let request = format!("GET {path} HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n");
    stream.write_all(request.as_bytes()).await.expect("发请求");
    let mut buffer = Vec::new();
    let read = tokio::time::timeout(Duration::from_secs(5), stream.read_to_end(&mut buffer))
        .await
        .expect("读响应超时")
        .expect("读响应");
    assert!(read > 0, "端点没有响应");
    let text = String::from_utf8_lossy(&buffer).into_owned();
    let (status, body) = text.split_once("\r\n\r\n").unwrap_or((text.as_str(), ""));
    let status_line = status.lines().next().unwrap_or_default().to_owned();
    (status_line, body.to_owned())
}

#[tokio::test]
async fn metrics_endpoint_reports_players_connections_and_tick_fields() {
    // 先往全局指标里塞一点可预测的东西（端点读的就是全局实例）。
    let global = metrics();
    global.record_tick(Duration::from_micros(1_234));
    global.record_auction_scan(1);
    global.record_db_save(Duration::from_micros(456));
    global.record_db_flush(Duration::from_micros(789));
    global.set_mail_queue(2);

    let world = Arc::new(Mutex::new(World::new(7)));
    let address = bind_metrics(
        "127.0.0.1:0".parse().expect("addr"),
        Arc::clone(&world),
        Arc::new(|| None),
    )
    .await
    .expect("绑定 metrics 端口");

    let (status, body) = http_get(address, "/metrics").await;
    assert!(status.contains("200"), "状态行：{status}");
    for needle in [
        "mir2_players_online ",
        "mir2_connections_active ",
        "mir2_connections_rejected ",
        "mir2_ticks_total ",
        "mir2_tick_micros_last ",
        "mir2_tick_micros_p50 ",
        "mir2_tick_micros_p95 ",
        "mir2_tick_micros_p99 ",
        "mir2_tick_micros_max ",
        "mir2_auction_scans_total ",
        "mir2_auctions_sold_total ",
        "mir2_mail_queue ",
        "mir2_db_save_total ",
        "mir2_db_save_micros_p95 ",
        "mir2_db_flush_total ",
        "mir2_db_flush_micros_p95 ",
        "mir2_packets_dropped_total ",
    ] {
        assert!(body.contains(needle), "缺少指标 {needle}：\n{body}");
    }
    assert!(body.contains("# TYPE mir2_ticks_total counter"), "{body}");
    assert!(body.contains("mir2_mail_queue 2"), "邮件队列应回填：{body}");
}

#[tokio::test]
async fn metrics_endpoint_lists_players_per_map() {
    let world = Arc::new(Mutex::new(World::new(7)));
    {
        let mut world = world.lock().await;
        world.insert_map(
            "0",
            Arc::new(mir_world::Map {
                width: 8,
                height: 8,
                cells: vec![mir_world::MapCell::default(); 64],
                file_name: "0".to_owned(),
                version: mir_world::MapVersion::V0,
                doors: Vec::new(),
            }),
        );
    }
    let address = bind_metrics(
        "127.0.0.1:0".parse().expect("addr"),
        Arc::clone(&world),
        Arc::new(|| None),
    )
    .await
    .expect("绑定 metrics 端口");

    // 1) 空世界：只有 `mir2_players_online`，没有任何 `mir2_players_on_map{...}` 行。
    let (status, body) = http_get(address, "/metrics").await;
    assert!(status.contains("200"), "{status}");
    assert!(body.contains("mir2_players_online "), "{body}");
    assert!(
        !body.contains("mir2_players_on_map{"),
        "没有玩家时不应出现每图人数行：\n{body}"
    );

    // 2) 放一个玩家进 map 0：`mir2_players_on_map{map="0"} 1`。
    {
        let mut world = world.lock().await;
        enter_player(&mut world, 1, "0");
    }
    let (status, body) = http_get(address, "/metrics").await;
    assert!(status.contains("200"), "{status}");
    assert!(
        body.contains("mir2_players_on_map{map=\"0\"} 1"),
        "map 0 应恰好 1 人：\n{body}"
    );

    // 3) 玩家离开后回到 0（渲染时不再列出该图）。
    {
        let mut world = world.lock().await;
        world.leave(1);
    }
    let (status, body) = http_get(address, "/metrics").await;
    assert!(status.contains("200"), "{status}");
    assert!(
        !body.contains("mir2_players_on_map{map=\"0\"} 1"),
        "玩家离开后 map 0 不应再是 1：\n{body}"
    );
}

#[tokio::test]
async fn metrics_endpoint_returns_404_for_unknown_paths_and_healthz() {
    let world = Arc::new(Mutex::new(World::new(7)));
    let address = bind_metrics(
        "127.0.0.1:0".parse().expect("addr"),
        world,
        Arc::new(|| None),
    )
    .await
    .expect("绑定 metrics 端口");
    let (status, _body) = http_get(address, "/nope").await;
    assert!(status.contains("404"), "未知路径应回 404：{status}");
    let (status, body) = http_get(address, "/healthz").await;
    assert!(status.contains("200"), "健康检查应回 200：{status}");
    assert_eq!(body.trim(), "ok");
}

#[test]
fn ring_window_and_percentiles_are_bounded() {
    // 端到端之外再钉一遍窗口语义：容量 3，写 5 个数只留最后 3 个。
    let ring = Ring::with_capacity(3);
    for value in [5_u32, 1, 9, 7, 3] {
        ring.push(value);
    }
    assert_eq!(ring.snapshot(), vec![3, 7, 9]);
    assert_eq!(ring.quantile(0.5), Some(7));
    assert_eq!(ring.max(), Some(9));
    assert_eq!(Metrics::new().ticks_total(), 0);
}

/// `/metrics` 必须真的把连接池统计发出来（soak 的「DB 连接数不增长」判据只认这两条）。
#[tokio::test]
async fn metrics_exposes_db_pool_gauges_when_probe_present() {
    let world = Arc::new(Mutex::new(World::new(7)));
    let address = bind_metrics(
        "127.0.0.1:0".parse().expect("addr"),
        Arc::clone(&world),
        Arc::new(|| Some((7, 3))),
    )
    .await
    .expect("绑定 metrics 端口");

    let (status, body) = http_get(address, "/metrics").await;
    assert!(status.contains("200"), "状态行：{status}");
    assert!(
        body.contains("mir2_db_pool_connections 7"),
        "缺少连接池总量：\n{body}"
    );
    // 慢泄漏观测（第 24 轮）：内部集合长度必须以**固定 kind 集合**暴露
    for kind in mir_server::metrics::INTERNAL_COLLECTION_KINDS {
        assert!(
            body.contains(&format!("mir2_internal_records{{kind=\"{kind}\"}}")),
            "缺少内部集合 gauge：kind={kind}\n{body}"
        );
    }
    assert!(
        body.contains("mir2_db_pool_idle 3"),
        "缺少空闲连接数：\n{body}"
    );
}

/// **增长注入自证**：往 `DB_SAVE_RECORDS` 塞记录后，`/metrics` 的
/// `mir2_internal_records{kind="db_save"}` 必须跟着涨；塞到 cap（1024）后必须**不再涨**。
///
/// 目的：证明这条 gauge 真的能抓到「慢增长」，而不是一个恒为常量的装饰。
#[tokio::test]
async fn internal_records_gauge_tracks_growth_and_stops_at_cap() {
    let world = Arc::new(Mutex::new(World::new(7)));
    let address = bind_metrics(
        "127.0.0.1:0".parse().expect("addr"),
        Arc::clone(&world),
        Arc::new(|| None),
    )
    .await
    .expect("绑定 metrics 端口");

    let before = mir_server::pg_store::db_save_record_count();
    let injected = mir_server::pg_store::inject_db_save_record_for_test(42);
    assert_eq!(injected, before + 1, "注入一条后计数应 +1");

    let (status, body) = http_get(address, "/metrics").await;
    assert!(status.contains("200"), "{status}");
    let line = format!("mir2_internal_records{{kind=\"db_save\"}} {injected}");
    assert!(
        body.contains(&line),
        "gauge 未跟随集合增长（期望出现 `{line}`）：\n{body}"
    );

    // 塞到 cap 以上：有界环形应当**停在 1024**
    for character_id in 0..(1024 + 16) {
        mir_server::pg_store::inject_db_save_record_for_test(character_id);
    }
    let capped = mir_server::pg_store::db_save_record_count();
    assert_eq!(capped, 1024, "环形缓冲上限必须是 1024（无界=慢泄漏）");
    let (_, body2) = http_get(address, "/metrics").await;
    assert!(
        body2.contains("mir2_internal_records{kind=\"db_save\"} 1024"),
        "塞到上限后 gauge 应停在 1024：\n{body2}"
    );
}
