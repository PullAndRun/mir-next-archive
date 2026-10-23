//! 85bfc21 基线性能：1/50/200 远端 × 1000 帧 entity_views CPU。
use std::hint::black_box;
use std::time::Instant;

use mir_client::net::{PlayerLooks, SelfPlayer, WorldEntity, WorldSnapshot};
use mir_core::Point;
use mir_core::enums::{MirClass, MirDirection, MirGender};
use mir_render::EntityKind;

const BASE: u64 = 2_000_000;

fn snapshot_with_self(position: Point) -> WorldSnapshot {
    WorldSnapshot {
        self_player: Some(SelfPlayer {
            object_id: 1,
            real_id: 1,
            name: "SelfHero".to_owned(),
            position,
            direction: MirDirection::Right,
            hp: 100,
            hp_max: 100,
            mp: 0,
            mp_max: 0,
            level: 1,
            class: MirClass::Warrior,
            gender: MirGender::Male,
            hair: 0,
            gold: 0,
            credit: 0,
            dead: false,
            action: mir_render::ActorAction::Standing,
            action_started_ms: 0,
            looks: PlayerLooks::default(),
        }),
        ..WorldSnapshot::default()
    }
}

fn percentile(sorted: &[u128], pct: f64) -> u128 {
    if sorted.is_empty() {
        return 0;
    }
    #[allow(clippy::cast_precision_loss, clippy::cast_sign_loss)]
    let index = ((sorted.len() as f64 - 1.0) * pct).round() as usize;
    sorted[index.min(sorted.len() - 1)]
}

#[test]
#[ignore = "baseline perf"]
#[allow(clippy::cast_precision_loss)]
fn baseline_perf() {
    let mut runs = Vec::new();
    for remote_count in [1_u32, 50, 200] {
        let mut snapshot = snapshot_with_self(Point::new(100, 100));
        for id in 2..2 + remote_count {
            let x = 100 + i32::try_from(id % 80).unwrap();
            snapshot.entities.insert(
                id,
                WorldEntity::new(
                    id,
                    EntityKind::Player,
                    format!("Remote{id}"),
                    Point::new(x, 100),
                    MirDirection::Right,
                    0,
                ),
            );
        }
        let mut times = Vec::with_capacity(1000);
        for frame in 0..1000_u64 {
            let now = BASE + frame * 16;
            let started = Instant::now();
            let views = snapshot.entity_views(now);
            times.push(started.elapsed().as_nanos());
            black_box(views.len());
        }
        times.sort_unstable();
        let median = percentile(&times, 0.50);
        let p95 = percentile(&times, 0.95);
        println!(
            "baseline_85bfc21 remote_count={remote_count} median={median}ns p95={p95}ns"
        );
        runs.push(serde_json::json!({
            "remotes": remote_count,
            "frames": 1000,
            "median_ns": median,
            "p95_ns": p95,
            "times_ns": times,
        }));
    }
    let out = std::env::var("R46_PERF_OUT").unwrap_or_else(|_| {
        "target/remote-motion-baseline.json".to_owned()
    });
    std::fs::write(
        &out,
        serde_json::to_string_pretty(&serde_json::json!({
            "baseline_commit": "85bfc21",
            "baseline_commit_full": "85bfc21befae1725d73982afedabefeaca09f110",
            "command": "cargo test --release -p mir-client --test remote_perf_baseline -- --ignored --nocapture",
            "host_note": "85bfc21 独立 worktree release 构建；同机、同场景（1/50/200 远端 ×1000 帧），每帧 entity_views",
            "runs": runs,
        }))
        .expect("baseline perf JSON"),
    )
    .expect("写 baseline perf JSON");
}
