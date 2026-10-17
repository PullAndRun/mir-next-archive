//! 真实样本的端到端测试。
//!
//! 样本来自 `resource/community/`（该目录被 gitignore，CI 上可能不存在），
//! 所以每个测试都在文件缺失时**优雅跳过**并打印原因。
//!
//! 覆盖范围：
//!
//! * 5 个 `clgforever910-mir2database` 样本（`Version` 83 / 88）——旧格式分支的真实数据，
//!   其中 `Daneo1989` 是 `<= 84` 的固定属性块编码；
//! * 1 个 `suprcode-crystal-database/cjlaaa` 样本（`Version` 105，452,106 字节），
//!   `docs/store/mirdb-format.md` 已逐字段手工验证过，这里断言它记录的具体事实与段偏移；
//! * 逐字节往返：对 `MapInfo` 与 `MonsterInfo` 两段做 `encode(decode(bytes)) == bytes`。

use std::path::{Path, PathBuf};

use mir_core::enums::Stat;
use mir_db::{DbWriter, MirDb};

/// 一个样本的元信息。
struct Sample {
    /// 目录名（用于断言消息）。
    name: &'static str,
    /// 相对 `resource/` 的目录。
    relative: &'static str,
    /// 权威版本号（直接读文件头 `int32`）。
    version: i32,
}

/// 全部样本。
const SAMPLES: &[Sample] = &[
    Sample {
        name: "clgforever910-mir2database/Daneo1989",
        relative: "community/clgforever910-mir2database/Daneo1989",
        version: 83,
    },
    Sample {
        name: "clgforever910-mir2database/Jev",
        relative: "community/clgforever910-mir2database/Jev",
        version: 88,
    },
    Sample {
        name: "clgforever910-mir2database/Jev Without Archers",
        relative: "community/clgforever910-mir2database/Jev Without Archers",
        version: 88,
    },
    Sample {
        name: "clgforever910-mir2database/Jev Without Archers&Sins",
        relative: "community/clgforever910-mir2database/Jev Without Archers&Sins",
        version: 88,
    },
    Sample {
        name: "clgforever910-mir2database/Jev Without Sins",
        relative: "community/clgforever910-mir2database/Jev Without Sins",
        version: 88,
    },
    Sample {
        name: "suprcode-crystal-database/cjlaaa",
        relative: "community/suprcode-crystal-database/cjlaaa",
        version: 105,
    },
];

/// `resource/` 根目录：`crates/mir-db` → 仓库根 → `resource`。
fn resource_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("resource")
}

/// 某样本的 `Server.MirDB` 路径。
fn database_path(relative: &str) -> PathBuf {
    resource_dir().join(relative).join("Server.MirDB")
}

/// 读样本；文件不存在时返回 `None`（调用方应当跳过）。
fn read_sample(sample: &Sample) -> Option<Vec<u8>> {
    let path = database_path(sample.relative);
    match std::fs::read(&path) {
        Ok(bytes) => Some(bytes),
        Err(error) => {
            eprintln!(
                "跳过样本 {}：无法读取 {}（{error}）；resource/ 通常被 gitignore",
                sample.name,
                path.display()
            );
            None
        }
    }
}

/// 解析一个样本，断言 `leftover == 0` 并返回数据库。
fn parse_sample(sample: &Sample) -> Option<(Vec<u8>, MirDb)> {
    let bytes = read_sample(sample)?;
    let (db, leftover) = match MirDb::parse_with_leftover(&bytes) {
        Ok(parsed) => parsed,
        Err(error) => panic!("样本 {} 解析失败：{error}", sample.name),
    };
    assert_eq!(
        leftover, 0,
        "样本 {} 应当正好消费全部字节，实际剩余 {leftover} 字节",
        sample.name
    );
    assert_eq!(db.header.version, sample.version, "样本 {}", sample.name);
    Some((bytes, db))
}

/// 计数器是「最后分配的 ID」，语义并不严格（`docs/store/mirdb-format.md` §1 / §8.4）：
///
/// * 记录被删除后，条数会明显少于计数器（`Jev Without Archers`：1349 个物品 ID、1254 条记录）；
/// * 反过来也有记录数超过计数器的样本（`Jev`：`MapIndex = 387` 却有 389 张地图；
///   `cjlaaa`：`ItemIndex = 1348` 却有 1349 条物品）。
///
/// 所以这里只做**数量级**校验：既抓住「版本门读错、条数离谱」这类 bug，
/// 又不把真实数据里对不齐的计数器当成错误。
fn assert_count_plausible(label: &str, count: usize, counter: Option<i32>) {
    assert!(count > 0, "{label} 段不应为空");
    if let Some(counter) = counter.filter(|&counter| counter > 0) {
        let counter = counter as usize;
        assert!(
            count <= counter * 2 + 2,
            "{label} 段 {count} 条远超头部计数器 {counter}，说明版本门可能读错"
        );
        assert!(
            count * 2 + 2 >= counter,
            "{label} 段只有 {count} 条，却声明计数器 {counter}，说明记录边界可能读错"
        );
    }
}

#[test]
fn every_sample_parses_with_zero_leftover() {
    let mut parsed = 0;
    for sample in SAMPLES {
        let Some((_bytes, db)) = parse_sample(sample) else {
            continue;
        };
        parsed += 1;

        assert!(!db.maps.is_empty(), "{} 的地图段为空", sample.name);
        assert!(!db.items.is_empty(), "{} 的物品段为空", sample.name);
        assert!(!db.monsters.is_empty(), "{} 的怪物段为空", sample.name);
        assert!(!db.npcs.is_empty(), "{} 的 NPC 段为空", sample.name);
        assert!(!db.quests.is_empty(), "{} 的任务段为空", sample.name);

        assert_count_plausible("maps", db.maps.len(), Some(db.header.map_index));
        assert_count_plausible("items", db.items.len(), Some(db.header.item_index));
        assert_count_plausible("monsters", db.monsters.len(), Some(db.header.monster_index));
        assert_count_plausible("npcs", db.npcs.len(), Some(db.header.npc_index));
        assert_count_plausible("quests", db.quests.len(), Some(db.header.quest_index));

        // `> 67` 的样本一定有刷怪计时器段。
        assert!(db.respawn.is_some(), "{} 缺刷怪计时器", sample.name);
        // 105 / 88 / 83 都 < 111，不可能有 GT 尾段。
        assert!(db.gt_maps.is_none(), "{} 不应有 GT 尾段", sample.name);

        // 段偏移必须首尾相接、覆盖到文件尾（`sections` 是解析器自己的账本）。
        // 起点用第一段的实际偏移（文件头长度随版本门变化：`>= 63` / `>= 66` / `>= 68` 各多一个 `i32`）。
        let mut cursor = db
            .sections
            .first()
            .unwrap_or_else(|| panic!("{} 应当至少解析出一段", sample.name))
            .range
            .start;
        for span in &db.sections {
            assert_eq!(
                span.range.start, cursor,
                "{} 的 {} 段起点",
                sample.name, span.name
            );
            assert!(span.range.end >= span.range.start);
            cursor = span.range.end;
        }
        let file_len = usize::try_from(
            std::fs::metadata(database_path(sample.relative))
                .expect("样本刚读过，元数据应当可取")
                .len(),
        )
        .expect("文件大小放得进 usize");
        assert_eq!(cursor, file_len, "{} 的段应当覆盖到文件尾", sample.name);
    }

    if parsed == 0 {
        eprintln!("跳过：resource/community 下没有 Server.MirDB 样本（CI 环境通常如此）");
    }
}

#[test]
fn cjlaaa_matches_the_documented_layout() {
    let path = database_path("community/suprcode-crystal-database/cjlaaa");
    if !path.exists() {
        eprintln!("跳过：找不到 {}", path.display());
        return;
    }
    let bytes = std::fs::read(&path).expect("样本应当可读");
    let (db, leftover) = MirDb::parse_with_leftover(&bytes).expect("解析应当成功");
    assert_eq!(leftover, 0);
    assert_eq!(bytes.len(), 452_106, "样本大小与文档一致");

    // 头部（`docs/store/mirdb-format.md` §8.1）。
    assert_eq!(db.header.version, 105);
    assert_eq!(db.header.custom_version, 0);
    assert_eq!(db.header.map_index, 389);
    assert_eq!(db.header.item_index, 1_348);
    assert_eq!(db.header.monster_index, 551);
    assert_eq!(db.header.npc_index, 309);
    assert_eq!(db.header.quest_index, 157);
    assert_eq!(db.header.gameshop_index, Some(118));
    assert_eq!(db.header.conquest_index, Some(0));
    assert_eq!(db.header.respawn_index, Some(5_951));

    // 第 0 张地图的具体事实（§8.3）。
    let map = db.maps.first().expect("至少有一张地图");
    assert_eq!(map.index, 1);
    assert_eq!(map.file_name, "0");
    assert_eq!(map.title, "比奇省");
    assert_eq!(map.mini_map, 101);
    assert_eq!(map.light, 0);
    assert_eq!(map.big_map, 135);
    assert_eq!(map.safe_zones.len(), 5);
    assert_eq!(map.respawns.len(), 358);
    assert_eq!(map.movements.len(), 70);
    assert!(
        db.maps.iter().any(|map| !map.respawns.is_empty()),
        "至少有一张地图带刷怪点"
    );

    // 怪物与物品列表非空，且能从字典格式的属性里取到 HP（`version > 84` 分支）。
    let monster = db.monsters.first().expect("至少有一个怪物");
    assert_eq!(monster.name, "弓箭守卫");
    assert_eq!(monster.ai, 57);
    assert_eq!(monster.stat(Stat::HP), 9_999);
    let item = db.items.first().expect("至少有一个物品");
    assert_eq!(item.name, "祈祷之剑");
    assert_eq!(item.shape, 13);
    assert_eq!(item.price, 30_000);

    // 商城 `> 105` 关不生效：没有购买标志（§8.4）。
    assert!(!db.gameshop.is_empty());
    assert!(
        db.gameshop
            .iter()
            .all(|shop| !shop.can_buy_credit && !shop.can_buy_gold)
    );

    // 段偏移与条数（§8.2）。
    let expected = [
        ("maps", 40, 251_090, 389),
        ("items", 251_090, 355_442, 1_349),
        ("monsters", 355_442, 398_505, 544),
        ("npcs", 398_505, 429_110, 309),
        ("quests", 429_110, 443_035, 157),
        ("dragon", 443_035, 443_174, 1),
        ("magics", 443_174, 448_441, 107),
        ("gameshop", 448_441, 452_077, 82),
        ("conquests", 452_077, 452_081, 0),
        ("respawn_timer", 452_081, 452_106, 1),
    ];
    for (name, start, end, count) in expected {
        let span = db
            .section(name)
            .unwrap_or_else(|| panic!("应当有 {name} 段"));
        assert_eq!(span.range.start, start, "{name} 段起点");
        assert_eq!(span.range.end, end, "{name} 段终点");
        assert_eq!(span.count, count, "{name} 段条数");
    }

    // 报告文本里的关键行（CLI 输出稳定性的锚点）。
    let report = mir_db::render_report(&db, leftover);
    assert!(report.contains("version=105\n"), "报告应含版本行");
    assert!(report.contains("leftover=0\n"), "报告应含 leftover 行");
    assert!(
        report.contains(
            "map[0] index=1 file_name=\"0\" title=\"比奇省\" mini_map=101 light=0 big_map=135 safe_zones=5 respawns=358 movements=70"
        ),
        "报告里的地图样本行：\n{report}"
    );
    assert!(
        report.contains("monster[0] index=1 name=\"弓箭守卫\" ai=57 level=0 hp=9999"),
        "报告里的怪物样本行：\n{report}"
    );
}

#[test]
fn legacy_samples_hit_the_old_branches() {
    // 83 / 88 版的真实数据会把 `<= 84` 与若干早期版本门走到，
    // 断言这些门「确实生效」——读错任何一处都会先让 leftover != 0。
    let mut checked = 0;
    for sample in SAMPLES.iter().filter(|sample| sample.version <= 88) {
        let Some((_bytes, db)) = parse_sample(sample) else {
            continue;
        };
        checked += 1;

        // `ItemData.cs:88-95`：`<= 84` 的 `StackSize` 是 `u32`，截断后不可能是 0 以外的怪值；
        // 这里只断言全部模板都读到了合理的堆叠上限。
        assert!(
            db.items.iter().all(|item| item.stack_size >= 1),
            "{} 的物品 StackSize 全部为 0，旧版分支可能读错",
            sample.name
        );
        // `MonsterInfo.cs:135-137`：`< 89` 没有 `DropPath`。
        assert!(
            db.monsters
                .iter()
                .all(|monster| monster.drop_path.is_empty()),
            "{} 的怪物 DropPath 非空，`< 89` 的版本门可能没生效",
            sample.name
        );
        // `NPCInfo.cs:91-97`：`<= 95` 没有 `ShowOnBigMap` / `BigMapIcon`；
        // `<= 96` 没有 `CanTeleportTo`。
        assert!(
            db.npcs
                .iter()
                .all(|npc| !npc.show_on_big_map && npc.big_map_icon == 0 && !npc.can_teleport_to),
            "{} 的 NPC 读到了 > 95 才存在的字段",
            sample.name
        );
        // `NPCInfo.cs:99-102`：`< 107` 没有 `ConquestVisible`，保留 C# 字段初值 `true`。
        assert!(
            db.npcs.iter().all(|npc| npc.conquest_visible),
            "{} 的 ConquestVisible 应是字段初值 true",
            sample.name
        );
        // `QuestInfo.cs:104-107`：`<= 90` 没有时限。
        assert!(
            db.quests
                .iter()
                .all(|quest| quest.time_limit_in_seconds == 0),
            "{} 的任务读到了 > 90 才存在的时限字段",
            sample.name
        );
        // `> 84` 才是字典格式的属性表：旧版怪物照样有 HP。
        assert!(
            db.monsters.iter().any(|monster| monster.stat(Stat::HP) > 0),
            "{} 的怪物 HP 全为 0，属性表分支可疑",
            sample.name
        );
    }
    if checked == 0 {
        eprintln!("跳过：没有找到任何 83 / 88 版样本");
    }

    // `Daneo1989` 是本仓库唯一一个 `version <= 84` 的真实样本，条数固定写下来当回归锚点。
    let dan = SAMPLES
        .iter()
        .find(|sample| sample.version == 83)
        .expect("样本表里应当有 83 版样本");
    if let Some((_bytes, db)) = parse_sample(dan) {
        assert_eq!(db.maps.len(), 400);
        assert_eq!(db.items.len(), 1_348);
        assert_eq!(db.monsters.len(), 506);
        assert_eq!(db.npcs.len(), 293);
        assert_eq!(db.quests.len(), 157);
        assert_eq!(db.magics.len(), 105);
        assert_eq!(db.gameshop.len(), 106);
        assert!(db.conquests.is_empty());
        // `> 67`：83 版也有刷怪计时器段。
        assert!(db.respawn.is_some());
        // `ItemData.cs:183-186`：`if (version > 83) Slots = ...`，83 版没有这个字节。
        assert!(
            db.items.iter().all(|item| item.slots == 0),
            "v83 不应读到 Slots 字段"
        );
    }
}

#[test]
fn maps_and_monsters_roundtrip_byte_exactly() {
    let mut checked = 0;
    for sample in SAMPLES {
        let Some((bytes, db)) = parse_sample(sample) else {
            continue;
        };
        checked += 1;
        let version = db.header.version;

        roundtrip_section(&bytes, &db, "maps", &db.maps, |w, map| {
            map.encode(w, version);
        });
        roundtrip_section(&bytes, &db, "monsters", &db.monsters, |w, monster| {
            monster.encode(w, version);
        });
    }
    if checked == 0 {
        eprintln!("跳过：没有找到任何样本");
    }
}

/// 按 `sections` 记录的字节范围，把 `items` 重新编码后与原文件逐字节比对。
fn roundtrip_section<T>(
    bytes: &[u8],
    db: &MirDb,
    name: &str,
    items: &[T],
    mut encode: impl FnMut(&mut DbWriter, &T),
) {
    let span = db.section(name).expect("段必须存在");
    let mut writer = DbWriter::new();
    writer.count(items.len());
    for item in items {
        encode(&mut writer, item);
    }
    let encoded = writer.into_inner();
    assert_eq!(
        encoded.len(),
        span.range.len(),
        "{name} 段重新编码后长度不同"
    );
    assert!(
        encoded.as_slice() == &bytes[span.range.clone()],
        "{name} 段重新编码后与原文件不一致"
    );
    assert_eq!(span.count, items.len(), "{name} 段条数");
}

#[test]
fn truncated_real_sample_is_rejected() {
    let path = database_path("community/suprcode-crystal-database/cjlaaa");
    if !path.exists() {
        eprintln!("跳过：找不到 {}", path.display());
        return;
    }
    let bytes = std::fs::read(&path).expect("样本应当可读");
    for cut in [40, 1_000, 251_090, 400_000] {
        let result = MirDb::from_bytes(&bytes[..cut]);
        assert!(
            result.is_err(),
            "截断到 {cut} 字节后仍解析成功，说明缺少边界检查"
        );
    }
}
