//! R47 A1/A3：战士空挥选招边界、生产本地动作链与 600/800ms 生命周期。

use mir_client::attack::{
    AttackContext, AttackRng, seed_from_epoch_and_process, select_attack_variant,
    select_attack_variant_with_rng, variant_for_roll,
};
use mir_client::camera;
use mir_client::net::{NetCommand, PlayerLooks, SelfPlayer, WorldSnapshot};
use mir_core::Point;
use mir_core::enums::{MirClass, MirDirection, MirGender, Spell};
use mir_protocol::ServerPacket;
use mir_protocol::packets::server::{
    Death, MapChanged, ObjectRemove, ObjectStruck, ObjectTurn, UserLocation,
};
use mir_render::animation::AttackVariant;
use mir_render::{ActorAction, EntityView};

const T0: u64 = 1_000_000;

fn context(
    class: MirClass,
    shift: bool,
    has_target: bool,
    spell: Spell,
    riding: bool,
) -> AttackContext {
    AttackContext {
        class,
        shift,
        has_target,
        spell,
        riding,
    }
}

#[allow(clippy::cast_precision_loss)]
fn percentile(sorted: &[u128], pct: f64) -> u128 {
    let index = ((sorted.len() as f64 - 1.0) * pct).round() as usize;
    sorted[index.min(sorted.len() - 1)]
}

#[derive(Debug)]
struct PerfStats {
    batches_ns: Vec<u128>,
    total_ns: u128,
    mean_ns_per_action: f64,
    median_ns_per_action: f64,
    p95_ns_per_action: f64,
}

#[allow(clippy::cast_precision_loss)]
fn summarize_batches(batches_ns: Vec<u128>, ops_per_batch: usize) -> PerfStats {
    let mut sorted = batches_ns.clone();
    sorted.sort_unstable();
    let total_ns = batches_ns.iter().sum::<u128>();
    let actions = u128::try_from(batches_ns.len() * ops_per_batch).expect("actions");
    let ops = ops_per_batch as f64;
    PerfStats {
        mean_ns_per_action: total_ns as f64 / actions as f64,
        median_ns_per_action: percentile(&sorted, 0.50) as f64 / ops,
        p95_ns_per_action: percentile(&sorted, 0.95) as f64 / ops,
        total_ns,
        batches_ns,
    }
}

fn record_batch(batches: &mut Vec<u128>, batch_started: std::time::Instant) {
    batches.push(batch_started.elapsed().as_nanos());
}

fn self_snapshot(class: MirClass) -> WorldSnapshot {
    WorldSnapshot {
        self_player: Some(SelfPlayer {
            object_id: 1,
            real_id: 1,
            name: "R47Hero".to_owned(),
            position: Point::new(100, 100),
            direction: MirDirection::Right,
            hp: 100,
            hp_max: 100,
            mp: 0,
            mp_max: 0,
            level: 1,
            class,
            gender: MirGender::Male,
            hair: 0,
            gold: 0,
            credit: 0,
            dead: false,
            action: ActorAction::Standing,
            action_started_ms: 0,
            attack_variant: None,
            looks: PlayerLooks::default(),
        }),
        ..WorldSnapshot::default()
    }
}

fn apply_attack(snapshot: &mut WorldSnapshot, variant: AttackVariant) {
    let command = NetCommand::Attack {
        direction: MirDirection::Right,
        variant: Some(variant),
    };
    let (action, direction, selected) = command.local_animation().expect("攻击本地动作");
    assert_eq!(selected, Some(variant));
    snapshot.apply_local_action_with_variant(action, direction, selected, T0);
}

// ---------------------------------------------------------------------------
// A1
// ---------------------------------------------------------------------------

#[test]
fn a1_roll_boundaries_0_19_20_99_and_illegal_100() {
    assert_eq!(variant_for_roll(0), Some(AttackVariant::Attack3));
    assert_eq!(variant_for_roll(19), Some(AttackVariant::Attack3));
    assert_eq!(variant_for_roll(20), Some(AttackVariant::Attack1));
    assert_eq!(variant_for_roll(99), Some(AttackVariant::Attack1));
    assert_eq!(
        variant_for_roll(100),
        None,
        "100 是非法抽签值，不偷进 Attack1"
    );
    assert_eq!(variant_for_roll(255), None);
}

#[test]
fn a1_matrix_only_eligible_warrior_shift_no_target_can_vary() {
    for class in MirClass::ALL {
        for shift in [false, true] {
            for has_target in [false, true] {
                for spell in [Spell::None, Spell::FireBall] {
                    for riding in [false, true] {
                        let ctx = context(*class, shift, has_target, spell, riding);
                        let eligible =
                            *class == MirClass::Warrior && !riding && spell == Spell::None;
                        let expected = if !eligible {
                            None
                        } else if shift && !has_target {
                            Some(AttackVariant::Attack3) // roll=0
                        } else {
                            Some(AttackVariant::Attack1)
                        };
                        assert_eq!(
                            select_attack_variant(&ctx, 0),
                            expected,
                            "class={class:?} shift={shift} target={has_target} spell={spell:?} riding={riding}"
                        );
                        if eligible && shift && !has_target {
                            assert_eq!(
                                select_attack_variant(&ctx, 19),
                                Some(AttackVariant::Attack3)
                            );
                            assert_eq!(
                                select_attack_variant(&ctx, 20),
                                Some(AttackVariant::Attack1)
                            );
                            assert_eq!(
                                select_attack_variant(&ctx, 99),
                                Some(AttackVariant::Attack1)
                            );
                        } else {
                            // 不随机分支对 roll 不敏感，仍固定 Attack1（或非入围 None）。
                            for roll in [0, 19, 20, 99] {
                                assert_eq!(
                                    select_attack_variant(&ctx, roll),
                                    expected,
                                    "非随机分支不得被 roll 改变"
                                );
                            }
                        }
                    }
                }
            }
        }
    }
}

#[test]
fn a1_seed_uses_full_u64_epoch_and_process_id() {
    const EPOCH: u64 = 1_792_787_753_316;
    let first = seed_from_epoch_and_process(EPOCH, 4242);
    let next = seed_from_epoch_and_process(EPOCH + 100, 4242);
    assert_ne!(first, 1, "生产种子不能退化成 1");
    assert_ne!(next, 1);
    assert_ne!(first, next, "相邻 epoch 必须得到不同种子");
    assert_ne!(
        AttackRng::new(first),
        AttackRng::new(next),
        "不同种子必须产生不同 RNG 初态"
    );

    // 两个进程即使在同一毫秒启动，也必须不同。
    let other_process = seed_from_epoch_and_process(EPOCH, 4243);
    assert_ne!(first, other_process);
    assert_ne!(AttackRng::new(first), AttackRng::new(other_process));

    // 高 32 位不同、低 32 位相同的 epoch 不得因 u32 截断而相撞。
    let low = EPOCH & 0xffff_ffff;
    let high_a = low | (1_u64 << 32);
    let high_b = low | (2_u64 << 32);
    assert_ne!(high_a, high_b);
    assert_eq!(
        u32::try_from(high_a & 0xffff_ffff).expect("low u32"),
        u32::try_from(high_b & 0xffff_ffff).expect("low u32")
    );
    assert_ne!(
        seed_from_epoch_and_process(high_a, 7),
        seed_from_epoch_and_process(high_b, 7)
    );
}

#[test]
fn a1_locked_or_invalid_target_is_not_none() {
    // 有目标（即使目标 ID 无效）必须算 has_target，不能靠回退进入随机分支。
    let ctx = context(MirClass::Warrior, true, true, Spell::None, false);
    assert_eq!(select_attack_variant(&ctx, 0), Some(AttackVariant::Attack1));
    assert_eq!(
        select_attack_variant(&ctx, 19),
        Some(AttackVariant::Attack1)
    );
}

#[test]
fn a1_rng_is_bounded_and_each_action_has_one_draw() {
    let ctx = context(MirClass::Warrior, true, false, Spell::None, false);
    let mut rng = AttackRng::new(0x1234_5678);
    assert_eq!(rng.draws(), 0);
    let first = select_attack_variant_with_rng(&ctx, &mut rng);
    assert!(first.is_some());
    assert_eq!(rng.draws(), 1, "每个动作建立只抽一次");
    let second = select_attack_variant_with_rng(&ctx, &mut rng);
    assert!(second.is_some());
    assert_eq!(rng.draws(), 2);

    // 变体写入快照后，多次 render/snapshot 不调用 RNG（draws 不变）。
    let mut snapshot = self_snapshot(MirClass::Warrior);
    apply_attack(&mut snapshot, AttackVariant::Attack3);
    let before = rng.draws();
    for now in [T0, T0 + 100, T0 + 300, T0 + 599] {
        let _ = snapshot.entity_views(now);
        let mut copy = snapshot.clone();
        let mut motion = None;
        let _ = camera::prepare_frame(&mut copy, &mut motion, Point::new(0, 0), now);
    }
    assert_eq!(rng.draws(), before, "render/snapshot 不得重抽");
}

#[test]
fn a1_rng_draws_only_for_shift_empty_swing() {
    // Crystal 只在 `Shift && TargetObject == null` 分支调用 Next(100)；
    // 锁定目标/普通 Space/非入围状态不得消耗随机数，否则会改变后续空挥序列。
    for ctx in [
        context(MirClass::Warrior, true, true, Spell::None, false),
        context(MirClass::Warrior, false, false, Spell::None, false),
        context(MirClass::Wizard, true, false, Spell::None, false),
        context(MirClass::Warrior, true, false, Spell::FireBall, false),
        context(MirClass::Warrior, true, false, Spell::None, true),
    ] {
        let mut rng = AttackRng::new(0x1234_5678);
        let _ = select_attack_variant_with_rng(&ctx, &mut rng);
        assert_eq!(rng.draws(), 0, "{ctx:?} 不应抽签");
    }
    let mut rng = AttackRng::new(0x1234_5678);
    let _ = select_attack_variant_with_rng(
        &context(MirClass::Warrior, true, false, Spell::None, false),
        &mut rng,
    );
    assert_eq!(rng.draws(), 1, "Shift+无目标空挥恰好抽一次");
}

// ---------------------------------------------------------------------------
// A3
// ---------------------------------------------------------------------------

#[test]
fn a3_attack1_600ms_and_attack3_800ms_do_not_fall_to_standing_at_300ms() {
    for (variant, duration) in [
        (AttackVariant::Attack1, 600_u64),
        (AttackVariant::Attack3, 800_u64),
    ] {
        let mut snapshot = self_snapshot(MirClass::Warrior);
        apply_attack(&mut snapshot, variant);
        let me = snapshot.self_player.as_ref().expect("自己");
        assert_eq!(me.action, ActorAction::Attacking);
        assert_eq!(me.attack_variant, Some(variant));
        assert_eq!(me.action_started_ms, T0);

        // 同坐标 UserLocation 不应重启动作/清变体（R45 语义）。
        snapshot.apply_packet_at(
            &ServerPacket::UserLocation(UserLocation {
                location: Point::new(100, 100),
                direction: MirDirection::Right,
            }),
            T0 + 200,
        );
        assert_eq!(
            snapshot.self_player.as_ref().unwrap().attack_variant,
            Some(variant)
        );

        for elapsed in [0_u64, 99, 100, 300, 599, duration - 1] {
            let mut ticked = snapshot.clone();
            ticked.tick_animations(T0 + elapsed);
            let me = ticked.self_player.as_ref().unwrap();
            assert_eq!(
                me.action,
                ActorAction::Attacking,
                "{variant:?} {elapsed}ms 不得提前 Standing"
            );
            assert_eq!(me.attack_variant, Some(variant));
            let views = ticked.entity_views(T0 + elapsed);
            let self_view = &views[0];
            assert_eq!(self_view.action, ActorAction::Attacking);
            assert_eq!(self_view.attack_variant, Some(variant));
        }

        let mut done = snapshot.clone();
        done.tick_animations(T0 + duration);
        let me = done.self_player.as_ref().unwrap();
        assert_eq!(
            me.action,
            ActorAction::Standing,
            "{variant:?} {duration}ms 应结束"
        );
        assert_eq!(me.attack_variant, None);
        let views = done.entity_views(T0 + duration);
        assert_eq!(views[0].action, ActorAction::Standing);
        assert_eq!(views[0].attack_variant, None);
    }
}

#[test]
fn a3_local_attack_clock_is_not_shifted_by_late_server_ack() {
    // 本地动作在发包时开始；200/600ms 后到的同坐标 UserLocation 只 bump，
    // 不得把 600/800ms 的动作顺延成“接收后再播 600/800ms”。
    for (variant, duration) in [
        (AttackVariant::Attack1, 600_u64),
        (AttackVariant::Attack3, 800_u64),
    ] {
        let snapshot = {
            let mut snapshot = self_snapshot(MirClass::Warrior);
            apply_attack(&mut snapshot, variant);
            snapshot
        };
        for delay in [200_u64, 600] {
            let mut delayed = snapshot.clone();
            delayed.apply_packet_at(
                &ServerPacket::UserLocation(UserLocation {
                    location: Point::new(100, 100),
                    direction: MirDirection::Right,
                }),
                T0 + delay,
            );
            let me = delayed.self_player.as_ref().expect("自己");
            assert_eq!(me.action_started_ms, T0, "迟到回包不得重启动动作");
            assert_eq!(me.attack_variant, Some(variant));

            let during = delayed.entity_views(T0 + delay.min(duration - 1));
            assert_eq!(during[0].action_started_ms, T0);
            assert_eq!(during[0].action, ActorAction::Attacking);
            assert_eq!(during[0].attack_variant, Some(variant));

            let mut ended = delayed.clone();
            ended.tick_animations(T0 + duration);
            let end = ended.entity_views(T0 + duration);
            assert_eq!(
                end[0].action,
                ActorAction::Standing,
                "{variant:?} 在发包后 {duration}ms 必须结束，不能按回包时刻顺延"
            );
            assert_eq!(end[0].attack_variant, None);
        }
    }
}

#[test]
fn a3_variant_is_established_once_and_survives_render_snapshot() {
    let mut snapshot = self_snapshot(MirClass::Warrior);
    apply_attack(&mut snapshot, AttackVariant::Attack3);
    let started = snapshot.self_player.as_ref().unwrap().action_started_ms;
    for elapsed in [0_u64, 100, 300, 599, 600, 799] {
        let mut copy = snapshot.clone();
        let mut motion = None;
        let prepared =
            camera::prepare_frame(&mut copy, &mut motion, Point::new(0, 0), T0 + elapsed);
        let self_view = prepared
            .entities
            .iter()
            .find(|view| view.name.as_deref() == Some("R47Hero"))
            .expect("自己");
        assert_eq!(self_view.attack_variant, Some(AttackVariant::Attack3));
        assert_eq!(self_view.action_started_ms, started);
    }
}

#[test]
fn a3_interruptions_new_attack_and_map_change_clear_variant() {
    let mut snapshot = self_snapshot(MirClass::Warrior);
    apply_attack(&mut snapshot, AttackVariant::Attack3);

    // 受击。
    snapshot.apply_packet_at(
        &ServerPacket::ObjectStruck(ObjectStruck {
            object_id: 1,
            attacker_id: 2,
            location: Point::new(101, 100),
            direction: MirDirection::Left,
        }),
        T0 + 100,
    );
    assert_eq!(snapshot.self_player.as_ref().unwrap().attack_variant, None);

    // 连续新攻击：后一个变体立即覆盖前一个。
    apply_attack(&mut snapshot, AttackVariant::Attack1);
    apply_attack(&mut snapshot, AttackVariant::Attack3);
    assert_eq!(
        snapshot.self_player.as_ref().unwrap().attack_variant,
        Some(AttackVariant::Attack3)
    );

    // 转身清旧动作变体。
    snapshot.apply_packet_at(
        &ServerPacket::ObjectTurn(ObjectTurn {
            object_id: 1,
            location: Point::new(100, 100),
            direction: MirDirection::Down,
        }),
        T0 + 200,
    );
    assert_eq!(snapshot.self_player.as_ref().unwrap().attack_variant, None);

    // 切图清实体/动作上下文。
    let mut map = self_snapshot(MirClass::Warrior);
    apply_attack(&mut map, AttackVariant::Attack3);
    map.apply_packet_at(
        &ServerPacket::MapChanged(MapChanged {
            map_index: 1,
            file_name: "1".to_owned(),
            ..MapChanged::default()
        }),
        T0 + 300,
    );
    assert!(map.self_player.as_ref().unwrap().attack_variant.is_none());

    // 删除/移动/死亡由现有动作打断路径清空（这里验证 Move 清变体）。
    let mut moving = self_snapshot(MirClass::Warrior);
    apply_attack(&mut moving, AttackVariant::Attack3);
    moving.apply_local_action_with_variant(
        ActorAction::Walking,
        MirDirection::Right,
        None,
        T0 + 400,
    );
    assert_eq!(moving.self_player.as_ref().unwrap().attack_variant, None);

    // 自己死亡：S.Death 走 set_action(Dying)，旧攻击变体必须消失。
    let mut died = self_snapshot(MirClass::Warrior);
    apply_attack(&mut died, AttackVariant::Attack3);
    died.apply_packet_at(
        &ServerPacket::Death(Death {
            location: Point::new(100, 100),
            direction: MirDirection::Right,
        }),
        T0 + 450,
    );
    assert_eq!(
        died.self_player.as_ref().unwrap().action,
        ActorAction::Dying
    );
    assert_eq!(died.self_player.as_ref().unwrap().attack_variant, None);

    // 断线/重连：ClientApp 的既有分支把自机切回 Standing（同一 set_action 方法），
    // 之后从新快照重新起算，不得把旧变体带进新连接。
    let mut reconnected = self_snapshot(MirClass::Warrior);
    apply_attack(&mut reconnected, AttackVariant::Attack3);
    reconnected
        .self_player
        .as_mut()
        .unwrap()
        .set_action(ActorAction::Standing, T0 + 500);
    assert_eq!(
        reconnected.self_player.as_ref().unwrap().attack_variant,
        None
    );

    // 删除自己实体不影响 self_player；用 ObjectRemove 同 ID 验证不 panic。
    let mut removed = self_snapshot(MirClass::Warrior);
    apply_attack(&mut removed, AttackVariant::Attack3);
    removed.apply_packet_at(
        &ServerPacket::ObjectRemove(ObjectRemove { object_id: 1 }),
        T0 + 500,
    );
    assert!(removed.remote_motions.is_empty());
}

#[test]
fn a3_non_eligible_classes_keep_legacy_attack1_50ms() {
    for class in [
        MirClass::Wizard,
        MirClass::Taoist,
        MirClass::Assassin,
        MirClass::Archer,
    ] {
        let mut snapshot = self_snapshot(class);
        snapshot.apply_local_action_with_variant(
            ActorAction::Attacking,
            MirDirection::Right,
            None,
            T0,
        );
        let me = snapshot.self_player.as_ref().unwrap();
        assert_eq!(me.attack_variant, None);
        snapshot.tick_animations(T0 + 300);
        assert_eq!(
            snapshot.self_player.as_ref().unwrap().action,
            ActorAction::Standing,
            "非入围职业保持旧 300ms 行为"
        );
    }
}

#[test]
#[ignore = "性能证据手动运行：先跑 55bd709 baseline，再 cargo test --release -p mir-client --test attack_variant -- --ignored attack_perf"]
#[allow(clippy::cast_precision_loss)]
fn attack_perf_1000_actions_baseline_vs_new() {
    use std::hint::black_box;
    use std::time::Instant;

    const ITERATIONS: usize = 1000;
    const BATCH_SIZE: usize = 100;
    const BATCHES: usize = ITERATIONS / BATCH_SIZE;
    // 真实模拟时钟：start 是动作开始基准，now = start + 偏移。
    const START_MS: u64 = 1_792_790_000_000;
    // 覆盖 Attack1 的 0..600ms 与 Attack3 的 0..800ms，各含终点后一帧。
    const SAMPLE_OFFSETS_MS: [u64; 11] = [0, 99, 100, 299, 300, 499, 500, 599, 600, 799, 800];
    const MAX_P95_RATIO: f64 = 1.40;
    const MAX_P95_SLACK_NS: f64 = 1_000.0; // 预算额外 1µs

    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let baseline_path = std::env::var("R47_BASELINE_PERF").map_or_else(
        |_| root.join("target/attack-variant-baseline.json"),
        std::path::PathBuf::from,
    );
    let baseline: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(&baseline_path).unwrap_or_else(|error| {
            panic!(
                "缺少 55bd709 baseline {}：{error}；先按 docs/testing/attack-variants/README.md 跑 baseline worktree",
                baseline_path.display()
            )
        }),
    )
    .expect("baseline JSON");
    let base_batches = baseline["runs"][0]["batches_ns"]
        .as_array()
        .expect("baseline batches_ns")
        .iter()
        .map(|value| u128::from(value.as_u64().expect("baseline batch")))
        .collect::<Vec<_>>();
    assert_eq!(base_batches.len(), BATCHES, "baseline 批次数量必须一致");
    let base = summarize_batches(base_batches, BATCH_SIZE);
    assert_eq!(
        baseline["runs"][0]["total_ns"].as_u64(),
        Some(u64::try_from(base.total_ns).expect("baseline total")),
        "baseline JSON 的 total_ns 必须等于 raw 批次之和"
    );

    // 1) 选招（1000 次）。
    let context = context(MirClass::Warrior, true, false, Spell::None, false);
    let rolls = (0..ITERATIONS)
        .map(|i| u8::try_from(i % 100).unwrap_or(0))
        .collect::<Vec<_>>();
    let mut selection_batches = Vec::with_capacity(BATCHES);
    for batch in 0..BATCHES {
        let batch_started = Instant::now();
        for roll in &rolls[batch * BATCH_SIZE..(batch + 1) * BATCH_SIZE] {
            let variant = select_attack_variant(&context, *roll);
            black_box(variant);
        }
        record_batch(&mut selection_batches, batch_started);
    }

    // 2) 动作建立（1000 次，使用已选出的变体）。
    let variants = rolls
        .iter()
        .map(|roll| select_attack_variant(&context, *roll))
        .collect::<Vec<_>>();
    let mut build_batches = Vec::with_capacity(BATCHES);
    for batch in 0..BATCHES {
        let batch_started = Instant::now();
        for variant in &variants[batch * BATCH_SIZE..(batch + 1) * BATCH_SIZE] {
            let animation = mir_render::animation::ActorAnimation::new_with_variant(
                ActorAction::Attacking,
                MirDirection::Down,
                mir_render::animation::BodyKind::Human,
                START_MS,
                black_box(*variant),
            );
            black_box(animation);
        }
        record_batch(&mut build_batches, batch_started);
    }

    // 3) 帧决策（1000 个动作 × 11 个模拟时刻，含活动帧与终点）。
    let animations = variants
        .iter()
        .map(|variant| {
            mir_render::animation::ActorAnimation::new_with_variant(
                ActorAction::Attacking,
                MirDirection::Down,
                mir_render::animation::BodyKind::Human,
                START_MS,
                *variant,
            )
        })
        .collect::<Vec<_>>();
    let mut frame_batches = Vec::with_capacity(BATCHES);
    for batch in 0..BATCHES {
        let batch_started = Instant::now();
        for animation in &animations[batch * BATCH_SIZE..(batch + 1) * BATCH_SIZE] {
            let animation = black_box(animation);
            for offset in SAMPLE_OFFSETS_MS {
                let now = black_box(START_MS + offset);
                let choice = animation.frame_at(now);
                black_box((
                    animation.attack_variant,
                    choice.index,
                    animation.duration_ms(),
                    choice.finished,
                ));
            }
        }
        record_batch(&mut frame_batches, batch_started);
    }

    // 4) 端到端（选招 + 建立 + 全部帧决策），与 baseline 同迭代数。
    let mut end_to_end_batches = Vec::with_capacity(BATCHES);
    for batch in 0..BATCHES {
        let batch_started = Instant::now();
        for &roll in &rolls[batch * BATCH_SIZE..(batch + 1) * BATCH_SIZE] {
            let variant = black_box(select_attack_variant(&context, roll));
            let animation = black_box(mir_render::animation::ActorAnimation::new_with_variant(
                ActorAction::Attacking,
                MirDirection::Down,
                mir_render::animation::BodyKind::Human,
                START_MS,
                variant,
            ));
            for offset in SAMPLE_OFFSETS_MS {
                let choice = animation.frame_at(black_box(START_MS + offset));
                black_box((
                    variant,
                    choice.index,
                    animation.duration_ms(),
                    choice.finished,
                ));
            }
        }
        record_batch(&mut end_to_end_batches, batch_started);
    }

    let selection = summarize_batches(selection_batches, BATCH_SIZE);
    let build = summarize_batches(build_batches, BATCH_SIZE);
    let frames = summarize_batches(frame_batches, BATCH_SIZE);
    let end_to_end = summarize_batches(end_to_end_batches, BATCH_SIZE);
    let threshold = base.p95_ns_per_action * MAX_P95_RATIO + MAX_P95_SLACK_NS;
    let ratio = end_to_end.p95_ns_per_action / base.p95_ns_per_action.max(1.0);
    assert!(
        frames.p95_ns_per_action > 0.0 && end_to_end.p95_ns_per_action > 0.0,
        "计时不得为 0"
    );
    assert!(
        end_to_end.p95_ns_per_action <= threshold,
        "新端到端 p95 {:.1}ns 超过固定预算 {:.1}ns（baseline {:.1}ns）",
        end_to_end.p95_ns_per_action,
        threshold,
        base.p95_ns_per_action
    );
    eprintln!(
        "baseline(55bd709) p95={:.1}ns | new p95 selection={:.1} build={:.1} frames={:.1} end_to_end={:.1}ns | ratio={ratio:.2} threshold={threshold:.1}ns",
        base.p95_ns_per_action,
        selection.p95_ns_per_action,
        build.p95_ns_per_action,
        frames.p95_ns_per_action,
        end_to_end.p95_ns_per_action
    );

    let path = root.join("target/attack-variant-perf.json");
    std::fs::write(
        &path,
        serde_json::to_string_pretty(&serde_json::json!({
            "schema": 2,
            "baseline_commit": "55bd709",
            // 仓库相对路径，避免把本机绝对路径写进可提交证据。
        "baseline_source": "target/attack-variant-baseline.json",
            "baseline_command": "cargo test --release -p mir-client --test attack_perf_baseline -- --ignored --nocapture（55bd709 独立 worktree）",
            "new_command": "cargo test --release -p mir-client --test attack_variant -- --ignored attack_perf --nocapture",
            "clock": {
                "start_ms": START_MS,
                "sample_offsets_ms": SAMPLE_OFFSETS_MS,
                "sample_note": "now = start_ms + offset；覆盖 0..duration 的活动帧与终点（Attack1 600ms, Attack3 800ms）",
            },
            "iterations": ITERATIONS,
            "batch_size": BATCH_SIZE,
            "batches": BATCHES,
            "scenario": "1000 次攻击动作；baseline=旧 Attack1/50ms 建立+11 帧决策；new=分相 select/new_with_variant/frame_at 与端到端",
            "threshold": {"p95_ratio": MAX_P95_RATIO, "p95_slack_ns": MAX_P95_SLACK_NS, "fixed_end_to_end_p95_ns": threshold},
            "baseline": {
                "batches_ns": base.batches_ns,
                "total_ns": base.total_ns,
                "mean_ns_per_action": base.mean_ns_per_action,
                "median_ns_per_action": base.median_ns_per_action,
                "p95_ns_per_action": base.p95_ns_per_action,
            },
            "new": {
                "selection": {
                    "batches_ns": selection.batches_ns,
                    "total_ns": selection.total_ns,
                    "mean_ns_per_action": selection.mean_ns_per_action,
                    "median_ns_per_action": selection.median_ns_per_action,
                    "p95_ns_per_action": selection.p95_ns_per_action,
                },
                "build": {
                    "batches_ns": build.batches_ns,
                    "total_ns": build.total_ns,
                    "mean_ns_per_action": build.mean_ns_per_action,
                    "median_ns_per_action": build.median_ns_per_action,
                    "p95_ns_per_action": build.p95_ns_per_action,
                },
                "frames": {
                    "batches_ns": frames.batches_ns,
                    "total_ns": frames.total_ns,
                    "mean_ns_per_action": frames.mean_ns_per_action,
                    "median_ns_per_action": frames.median_ns_per_action,
                    "p95_ns_per_action": frames.p95_ns_per_action,
                },
                "end_to_end": {
                    "batches_ns": end_to_end.batches_ns,
                    "total_ns": end_to_end.total_ns,
                    "mean_ns_per_action": end_to_end.mean_ns_per_action,
                    "median_ns_per_action": end_to_end.median_ns_per_action,
                    "p95_ns_per_action": end_to_end.p95_ns_per_action,
                },
                "measured_p95_ratio": ratio,
            },
        }))
        .expect("perf JSON"),
    )
    .expect("写 target/attack-variant-perf.json");
    eprintln!("perf report: {}", path.display());
}

#[test]
fn a3_entity_view_fields_are_propagated_to_renderer_contract() {
    let mut snapshot = self_snapshot(MirClass::Warrior);
    apply_attack(&mut snapshot, AttackVariant::Attack3);
    let views = snapshot.entity_views(T0 + 100);
    let self_view: &EntityView = &views[0];
    assert_eq!(self_view.attack_variant, Some(AttackVariant::Attack3));
    let animation = self_view.animation();
    assert_eq!(animation.attack_variant, Some(AttackVariant::Attack3));
    assert_eq!(animation.duration_ms(), 800);
}
