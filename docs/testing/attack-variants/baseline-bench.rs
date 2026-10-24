//! 55bd709 基线性能：1000 次「旧 Attack1/50ms 动画建立 + 帧决策」。
//! 编译前复制到 55bd709 worktree 的 `crates/mir-client/tests/attack_perf_baseline.rs`。
//! 时钟必须用 `start_ms + offset`；`start=0/now≈1e6` 会让所有帧在 300ms 后结束。
use std::hint::black_box;
use std::time::Instant;

use mir_core::enums::MirDirection;
use mir_render::animation::{ActorAction, ActorAnimation, BodyKind};

const ITERATIONS: usize = 1000;
const BATCH_SIZE: usize = 100;
const BATCHES: usize = ITERATIONS / BATCH_SIZE;
const START_MS: u64 = 1_792_790_000_000;
const SAMPLE_OFFSETS_MS: [u64; 11] = [0, 99, 100, 299, 300, 499, 500, 599, 600, 799, 800];

#[allow(clippy::cast_precision_loss)]
fn percentile(sorted: &[u128], pct: f64) -> u128 {
    let index = ((sorted.len() as f64 - 1.0) * pct).round() as usize;
    sorted[index.min(sorted.len() - 1)]
}

#[test]
#[ignore = "R47 baseline perf"]
#[allow(clippy::cast_precision_loss)]
fn attack_perf_baseline() {
    let mut batches_ns = Vec::with_capacity(BATCHES);
    for _ in 0..BATCHES {
        let batch_started = Instant::now();
        for _ in 0..BATCH_SIZE {
            // black_box 输入阻止编译器把常量动作/时刻整体常量折叠或循环外提。
            let animation = black_box(ActorAnimation::new(
                ActorAction::Attacking,
                MirDirection::Down,
                BodyKind::Human,
                START_MS,
            ));
            for offset in SAMPLE_OFFSETS_MS {
                let now = black_box(START_MS + offset);
                let choice = animation.frame_at(now);
                black_box((choice.index, animation.duration_ms(), choice.finished));
            }
        }
        batches_ns.push(batch_started.elapsed().as_nanos());
    }
    let mut sorted = batches_ns.clone();
    sorted.sort_unstable();
    let total_ns = batches_ns.iter().sum::<u128>();
    let actions = u128::try_from(BATCHES * BATCH_SIZE).expect("actions");
    let ops = BATCH_SIZE as f64;
    let mean_ns_per_action = total_ns as f64 / actions as f64;
    let median_ns_per_action = percentile(&sorted, 0.50) as f64 / ops;
    let p95_ns_per_action = percentile(&sorted, 0.95) as f64 / ops;
    println!(
        "baseline_55bd709 batches={BATCHES} total={total_ns}ns mean={mean_ns_per_action:.1}ns/action median={median_ns_per_action:.1}ns p95={p95_ns_per_action:.1}ns"
    );
    let out = std::env::var("R47_PERF_OUT")
        .unwrap_or_else(|_| "target/attack-variant-baseline.json".to_owned());
    std::fs::write(
        &out,
        serde_json::to_string_pretty(&serde_json::json!({
            "schema": 2,
            "baseline_commit": "55bd709",
            "baseline_commit_full": "55bd709",
            "command": "cargo test --release -p mir-client --test attack_perf_baseline -- --ignored --nocapture",
            "clock": {
                "start_ms": START_MS,
                "sample_offsets_ms": SAMPLE_OFFSETS_MS,
                "sample_note": "now = start_ms + offset；覆盖 0..duration 的活动帧与终点",
            },
            "iterations": ITERATIONS,
            "batch_size": BATCH_SIZE,
            "batches": BATCHES,
            "scenario": "1000 × 旧 ActorAnimation::new(Attacking) + 11 帧决策（55bd709 旧 Attack1/50ms 路径）",
            "runs": [{
                "batches_ns": batches_ns,
                "total_ns": total_ns,
                "mean_ns_per_action": mean_ns_per_action,
                "median_ns_per_action": median_ns_per_action,
                "p95_ns_per_action": p95_ns_per_action,
            }],
        }))
        .expect("baseline JSON"),
    )
    .expect("写 baseline JSON");
}
