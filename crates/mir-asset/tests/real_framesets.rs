//! R49 A1 真实 `.Lib` 帧表回归；需要显式 `--ignored` 执行。
//!
//! 只读仓库内已有的 Alliance 源库，不写任何文件、不导入 game-assets。

#[cfg(feature = "legacy")]
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use mir_asset::{Frame, FrameSet, action_name};

fn source(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../resource/community/mirfiles-alliance/Alliance/Data/Data(Mount-Npc...)/Monster")
        .join(name)
}

/// R49 之前的旧布局规则，仅用于本次改动的对照证据（不是运行时语义）。
fn legacy_layout_error(frame: &Frame) -> Option<&'static str> {
    if frame.start < 0 || frame.count < 0 || frame.interval < 0 {
        return Some("负的 Start/Count/Interval");
    }
    if frame.count > 1 {
        let stride = i64::from(frame.count) + i64::from(frame.skip);
        if stride < 0 {
            return Some("Count>1 时方向步长为负");
        }
        let end = i64::from(frame.start) + 7 * stride + i64::from(frame.count);
        if end > i64::from(i32::MAX) {
            return Some("帧下标溢出 i32");
        }
    } else if frame.count == 1 && frame.skip < -1 {
        return Some("Count=1 时 Skip 最小为 -1");
    }
    if frame.effect_count < 0 || frame.effect_start < 0 || frame.effect_interval < 0 {
        return Some("负的特效 Start/Count/Interval");
    }
    None
}

#[test]
#[ignore = "真实素材：cargo test -p mir-asset --test real_framesets -- --ignored"]
fn real_005_022_000_framesets_match_crystal_records() {
    let set = FrameSet::from_file(&source("005.Lib"))
        .expect("005 头合法")
        .expect("005 有真 v3 帧表");
    assert_eq!(set.frames.len(), 7, "005 的 frameCount 应为 7");
    assert_eq!(
        set.frames.iter().map(|f| f.action).collect::<Vec<_>>(),
        vec![0, 1, 9, 18, 21, 22, 28]
    );
    assert_eq!(
        set.known_actions(),
        vec![
            "Standing", "Walking", "Attack1", "Struck", "Die", "Dead", "Revive"
        ]
    );
    let standing = set.get(0).expect("Standing");
    assert_eq!((standing.start, standing.count, standing.skip), (0, 4, 0));
    assert_eq!((standing.interval, standing.effect_count), (500, 0));
    let die = set.get(21).expect("Die");
    assert_eq!((die.start, die.count, die.interval), (144, 10, 100));
    assert!(set.get(28).expect("Revive").reverse);

    let set = FrameSet::from_file(&source("022.Lib"))
        .expect("022 头合法")
        .expect("022 有真 v3 帧表");
    assert_eq!(set.frames.len(), 7);
    assert_eq!(set.get(9).expect("Attack1").start, 80);

    let set = FrameSet::from_file(&source("000.Lib"))
        .expect("000 头合法")
        .expect("000 有真 v3 帧表");
    assert_eq!(set.frames.len(), 3);
    assert_eq!(set.known_actions(), vec!["Standing", "Walking", "Attack1"]);
    assert_eq!(set.get(1).expect("Walking").count, 6);
}

/// R49 X1-01 回归：用 A0 冻结的 used 清单（123 条记录 / 122 个不同 `.Lib`）
/// 重放本次放宽。118 个「表有效」库必须全部仍能解析、帧字段与 A0 元数据一致；
/// 同时用旧规则对照，记录放宽后新接受的帧。证据写到
/// `target/r49/frames-negative-stride-scan.json`。
#[test]
#[ignore = "真实素材：cargo test -p mir-asset --test real_framesets -- --ignored --nocapture"]
fn real_used_monster_tables_survive_negative_stride_relaxation() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let used_path = root.join("target/r49/used-monster-images.json");
    let text = std::fs::read_to_string(&used_path)
        .unwrap_or_else(|error| panic!("读 A0 used 清单 {} 失败：{error}", used_path.display()));
    let doc: serde_json::Value = serde_json::from_str(&text).expect("A0 used 清单是合法 JSON");
    let items = doc["items"].as_array().expect("A0 used 清单有 items 数组");

    let mut valid = 0usize;
    let mut negative_stride_frames = 0usize;
    let mut single_frame_negative_skip = 0usize;
    let mut newly_accepted = Vec::new();
    let mut non_v3 = Vec::new();
    for item in items {
        let source = &item["source"];
        let Some(path) = source["path"].as_str() else {
            non_v3.push(format!(
                "appearance {} 无源文件（file_missing）",
                item["appearance"]
            ));
            continue;
        };
        let layout = source["layout"].as_str().unwrap_or("");
        let parsed = FrameSet::from_file(&root.join(path))
            .unwrap_or_else(|error| panic!("{path} 解析报错：{error}"));
        if layout != "v3_frames" {
            assert!(parsed.is_none(), "{path} 标为 {layout} 却读出了帧表");
            non_v3.push(format!(
                "appearance {} {path} layout={layout} → 无帧表",
                item["appearance"]
            ));
            continue;
        }
        let set = parsed.unwrap_or_else(|| panic!("{path} 是 v3_frames 但没读到帧表"));
        let metadata = source["actions"].as_object().expect("A0 actions 对象");
        assert_eq!(
            set.frames.len(),
            source["frame_count"].as_u64().expect("A0 frame_count") as usize,
            "{path} 帧数应与 A0 元数据一致"
        );
        assert_eq!(
            set.frames.len(),
            metadata.len(),
            "{path} 帧数应与 A0 actions 条数一致"
        );
        for frame in &set.frames {
            let key = frame.action.to_string();
            let meta = metadata
                .get(&key)
                .unwrap_or_else(|| panic!("{path} action {} 不在 A0 元数据里", frame.action));
            assert_eq!(
                frame.start,
                meta["start"].as_i64().expect("start") as i32,
                "{path} action {key} start"
            );
            assert_eq!(
                frame.count,
                meta["count"].as_i64().expect("count") as i32,
                "{path} action {key} count"
            );
            assert_eq!(
                frame.skip,
                meta["skip"].as_i64().expect("skip") as i32,
                "{path} action {key} skip"
            );
            assert_eq!(
                frame.interval,
                meta["interval"].as_i64().expect("interval") as i32,
                "{path} action {key} interval"
            );
            // 新规则必须继续接受 A0 已认定合法的库。
            assert_eq!(
                frame.layout_error(),
                None,
                "{path} action {key} 被新规则拒绝"
            );
            for direction in 0..8usize {
                assert!(
                    frame.frame_indices(direction).is_some(),
                    "{path} action {key} 方向 {direction} 无有效区间"
                );
            }
            if frame.count > 1 && frame.skip < -frame.count {
                negative_stride_frames += 1;
            }
            if frame.count == 1 && frame.skip < -1 {
                single_frame_negative_skip += 1;
            }
            if legacy_layout_error(frame).is_some() {
                newly_accepted.push(serde_json::json!({
                    "file": path,
                    "action": frame.action,
                    "start": frame.start,
                    "count": frame.count,
                    "skip": frame.skip,
                }));
            }
        }
        valid += 1;
    }
    assert_eq!(valid, 118, "A0 的 118 个表有效 used 库必须全部仍可解析");
    assert!(
        newly_accepted.is_empty(),
        "放宽不应改变 118 个已导入库的接受结果，实际新接受：{newly_accepted:?}"
    );
    let report = serde_json::json!({
        "schema": "mir-next/r49-frames-negative-stride-scan/v1",
        "source": "target/r49/used-monster-images.json",
        "items": items.len(),
        "valid_v3_frames": valid,
        "negative_stride_frames_in_used_libs": negative_stride_frames,
        "single_frame_negative_skip_frames_in_used_libs": single_frame_negative_skip,
        "newly_accepted_frames": newly_accepted,
        "non_v3_items": non_v3,
    });
    let out = root.join("target/r49/frames-negative-stride-scan.json");
    std::fs::write(
        &out,
        serde_json::to_string_pretty(&report).expect("序列化扫描结果"),
    )
    .expect("写扫描结果");
    println!(
        "used 库负 stride 扫描：{valid} 个 v3 库全部通过，结果 {}",
        out.display()
    );
    for entry in &non_v3 {
        println!("非 v3：{entry}");
    }
}

#[test]
#[ignore = "真实素材：cargo test -p mir-asset --test real_framesets -- --ignored"]
fn real_v3_without_frame_seek_is_none() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join(
        "../../resource/community/lomcn-artificialmir-client-data/ArtificialMir2/Data/Monster/000.Lib",
    );
    let set = FrameSet::from_file(&path).expect("无 frameSeek 的 v3 合法");
    assert!(set.is_none(), "无 frameSeek 必须返回 None，不得猜表");
}

#[test]
#[ignore = "真实素材：cargo test -p mir-asset --test real_framesets -- --ignored"]
fn real_v4_is_rejected_not_parsed_as_v3() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../resource/community/mirfiles-mercifulmir/MercifulMir/Data/Monster");
    let mut found = None;
    for entry in std::fs::read_dir(root).expect("MercifulMir 源存在") {
        let path = entry.expect("目录项").path();
        if path.extension().and_then(|e| e.to_str()) != Some("Lib") {
            continue;
        }
        let Ok(bytes) = std::fs::read(&path) else {
            continue;
        };
        if bytes.len() >= 4 && i32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]) == 4 {
            found = Some(path);
            break;
        }
    }
    let path = found.expect("MercifulMir 至少有一个 v4 库");
    let error = FrameSet::from_file(&path).expect_err("v4 必须拒绝");
    assert!(
        matches!(error, mir_asset::AssetError::BadMagic { .. }),
        "实际错误：{error}"
    );
}

#[cfg(feature = "legacy")]
#[test]
#[ignore = "真实素材：cargo test -p mir-asset --features legacy --test real_framesets -- --ignored --nocapture"]
fn real_stride_zero_monsters_share_frames_across_eight_directions() {
    use mir_asset::{ImageLibrary, LibraryKind};

    for stem in ["010", "041", "043", "062"] {
        let path = source(&format!("{stem}.Lib"));
        let lib = ImageLibrary::open(&path).expect("打开 stride=0 怪物库");
        assert_eq!(lib.kind(), LibraryKind::Lib);
        let set = lib.frames().expect("读帧表").expect("有 v3 帧表");
        let count = lib.len();
        let mut checked = 0usize;
        let mut stride_zero = 0usize;
        for frame in &set.frames {
            if frame.count <= 0 {
                continue;
            }
            let first = frame.frame_indices(0);
            assert!(
                first.is_some(),
                "{stem} action {} 无 0 方向帧",
                frame.action
            );
            if frame.count > 1 && frame.skip == -frame.count {
                stride_zero += 1;
                for direction in 0..8usize {
                    assert_eq!(
                        frame.frame_indices(direction),
                        first,
                        "{stem} action {} 的 8 方向帧区间应相同",
                        frame.action
                    );
                }
            }
            let range = first.expect("已检查");
            assert_eq!(range.len(), usize::try_from(frame.count).unwrap());
            for index in range {
                let index = usize::try_from(index).expect("非负下标");
                assert!(index < count, "{stem} index {index} >= {count}");
                let image = lib
                    .image(index)
                    .unwrap_or_else(|error| panic!("{stem} idx {index}: {error}"));
                assert!(!image.pixels.is_empty(), "{stem} idx {index} 像素为空");
                checked += 1;
            }
        }
        assert!(checked > 0, "{stem} 没有可解码帧");
        assert!(stride_zero > 0, "{stem} 没有 stride=0 动作，测试样本失效");
    }
}

#[test]
fn action_names_match_mir_action_numbers() {
    assert_eq!(action_name(0), Some("Standing"));
    assert_eq!(action_name(1), Some("Walking"));
    assert_eq!(action_name(2), Some("Running"));
    assert_eq!(action_name(9), Some("Attack1"));
    assert_eq!(action_name(10), Some("Attack2"));
    assert_eq!(action_name(11), Some("Attack3"));
    assert_eq!(action_name(12), Some("Attack4"));
    assert_eq!(action_name(18), Some("Struck"));
    assert_eq!(action_name(21), Some("Die"));
    assert_eq!(action_name(22), Some("Dead"));
    assert_eq!(action_name(28), Some("Revive"));
    assert_eq!(action_name(17), None);
}

#[cfg(feature = "legacy")]
#[test]
#[ignore = "真实素材：cargo test -p mir-asset --features legacy --test real_framesets -- --ignored --nocapture"]
fn real_core_monster_frames_decode_nonempty() {
    use mir_asset::{ImageLibrary, LibraryKind};

    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join(
        "../../resource/community/mirfiles-alliance/Alliance/Data/Data(Mount-Npc...)/Monster",
    );
    let mut report = String::from("{\n");
    let mut first = true;
    for stem in ["005", "022", "039"] {
        let path = root.join(format!("{stem}.Lib"));
        let lib = ImageLibrary::open(&path).expect("打开核心怪物库");
        assert_eq!(lib.kind(), LibraryKind::Lib);
        let set = lib.frames().expect("读帧表").expect("核心库有帧表");
        let count = lib.len();
        let mut checked = 0usize;
        for frame in &set.frames {
            if frame.count <= 0 {
                continue;
            }
            for dir in 0..8usize {
                for j in 0..usize::try_from(frame.count).unwrap() {
                    let index = usize::try_from(frame.start).unwrap()
                        + dir * usize::try_from(frame.count).unwrap()
                        + j;
                    assert!(
                        index < count,
                        "{stem} action {} dir {dir} index {index} >= {count}",
                        frame.action
                    );
                    let image = lib
                        .image(index)
                        .unwrap_or_else(|error| panic!("{stem} idx {index}: {error}"));
                    assert!(
                        !image.pixels.is_empty(),
                        "{stem} action {} dir {dir} idx {index} 像素为空",
                        frame.action
                    );
                    checked += 1;
                }
            }
        }
        assert!(checked > 0);
        if !first {
            report.push_str(",\n");
        }
        first = false;
        let _ = write!(
            report,
            "  \"{stem}\": {{\"images\":{count},\"frames_checked\":{checked},\"actions\":{}}}",
            set.frames.len()
        );
    }
    report.push_str("\n}\n");
    let out = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/r49/core-pixel-audit.json");
    std::fs::write(&out, report).expect("写像素审计");
    println!("core pixel audit: {}", out.display());
}
