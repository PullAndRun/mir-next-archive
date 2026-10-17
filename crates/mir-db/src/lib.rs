//! 《传奇-Next》存档层：Crystal `Server.MirDB` 数据库读取器。
//!
//! `Server.MirDB` 是**无 magic、无压缩、无校验和、无索引**的裸 `BinaryWriter` 顺序流：
//! 10 个小端 `int32` 头部计数，随后 10 个顺序记录段（详见 `docs/store/mirdb-format.md`）。
//! 本 crate 把它解析成强类型结构，供服务端后续复用。
//!
//! ```no_run
//! let db = mir_db::MirDb::load("resource/community/suprcode-crystal-database/cjlaaa/Server.MirDB")?;
//! println!("{} 张地图，{} 个怪物模板", db.maps.len(), db.monsters.len());
//! # Ok::<(), mir_db::MirDbError>(())
//! ```
//!
//! # 用法要点
//!
//! * [`MirDb::from_bytes`] / [`MirDb::load`] 只要求「能解析」，**不**因为文件尾多出几个字节
//!   而失败（Crystal 侧遇到多余字节也是静默忽略）；
//! * 想自己检查越界字节就用 [`MirDb::parse_with_leftover`] / [`MirDb::load_with_leftover`]，
//!   它们返回 `(MirDb, leftover)`。非 0 的 leftover 说明文件与本解析器版本不符，是 bug 信号
//!   （`docs/store/mirdb-format.md` §8.2 用 leftover == 0 验证过 105 版样本）；
//! * 版本必须落在 `60..=117`（Crystal `Envir.MinVersion` / `Envir.Version`，`Envir.cs:56-57`），
//!   否则直接 [`MirDbError::UnsupportedVersion`]——Crystal 在同样情况下只是打一条日志、
//!   带着空世界继续跑（`Envir.cs:2836-2845` 与 `:3321` 的裸调用），这个坑不要复刻。
//!
//! # 版本门
//!
//! 所有 `if (Envir.LoadVersion ...)` 分支都按源码逐条实现，并打上 `File.cs:line` 出处。
//! `version <= 84` 的旧格式（固定属性块、`u32` 的 `StackSize` / `Count` 等）完整支持，
//! 且被 `resource/community/clgforever910-mir2database/Daneo1989`（`Version = 83`）这个真实样本覆盖。
//!
//! # 复用
//!
//! 物品模板与商城商品直接复用 `mir-protocol` 的 [`mir_protocol::data::ItemInfo`] /
//! [`mir_protocol::data::GameShopItem`]（现代布局与包路径逐字节相同），只在本 crate 里补
//! 旧版分支，理由见 [`item`] 模块文档。

pub mod conquest;
pub mod dragon;
pub mod drops;
pub mod item;
pub mod magic;
pub mod map;
pub mod monster;
pub mod npc;
pub mod quest;
pub mod reader;
pub mod report;
pub mod respawn;

use std::ops::Range;
use std::path::{Path, PathBuf};

use mir_protocol::ProtocolError;

/// 商城商品：直接复用 `mir-protocol` 的类型（`decode_db` / `encode_db` 已在那边实现）。
pub use mir_protocol::data::GameShopItem;
/// 物品模板：**直接复用** `mir-protocol` 的类型——DB 的现代布局与包路径逐字节相同，
/// 旧版（`<= 84`）只是解码方式不同，解出来的还是同一个类型（见 [`item`] 模块文档）。
pub use mir_protocol::data::ItemInfo;
/// 属性表：`mir-protocol` 的 `Stats`（`BTreeMap` 语义与 C# `SortedDictionary` 一致）。
pub use mir_protocol::data::Stats;

pub use conquest::{ConquestArcherInfo, ConquestFlagInfo, ConquestInfo, ConquestStructure};
pub use dragon::{DRAGON_EXP_LEVELS, DragonInfo, MAX_DRAGON_LEVEL};
pub use drops::{DropInfo, DropKind, DropReward, DropTable, GroupDrop};
pub use magic::MagicInfo;
pub use map::{MapInfo, MineZone, MovementInfo, RespawnInfo, SafeZoneInfo};
pub use monster::MonsterInfo;
pub use npc::NpcInfo;
pub use quest::QuestInfo;
pub use reader::{DbReader, DbWriter};
pub use report::{SAMPLE_ROWS, render_report};
pub use respawn::{GtMap, RespawnTickOption, RespawnTimer};

use crate::respawn::MIN_GT_MAP_LEN;

/// Crystal 能读的最低版本（`Envir.cs:56`：`public const int MinVersion = 60;`）。
pub const MIN_VERSION: i32 = 60;

/// Crystal 当前写出的版本（`Envir.cs:57`：`public const int Version = 117;`）。
pub const MAX_VERSION: i32 = 117;

/// 数据库层的错误。
///
/// 所有错误都带**段名 + 绝对偏移 + 文件版本**：`Server.MirDB` 没有 magic 也没有校验和，
/// 因此「文件损坏」与「版本不符」只能在错位的第一现场（非法布尔值、离谱计数、读到文件尾）
/// 才能区分（`docs/store/mirdb-format.md` §6.2）。
#[derive(Debug, thiserror::Error)]
pub enum MirDbError {
    /// 文件版本超出 Crystal 支持范围（`Envir.cs:2836-2845`）。
    #[error(
        "文件版本 {version} 不在支持范围 {min}..={max} 内（Crystal `Envir.MinVersion` / `Envir.Version`）"
    )]
    UnsupportedVersion {
        /// 文件头里读到的版本。
        version: i32,
        /// 允许的最低版本。
        min: i32,
        /// 允许的最高版本。
        max: i32,
    },
    /// 读到文件尾（结构被截断，或版本门与文件实际布局不符，导致越读越多）。
    #[error("「{section}」偏移 {offset}（文件版本 {version}）处读取失败：{source}")]
    Truncated {
        /// 出错时所在的段。
        section: &'static str,
        /// 出错时的绝对偏移。
        offset: usize,
        /// 文件头里的版本。
        version: i32,
        /// 底层读取器的错误。
        #[source]
        source: ProtocolError,
    },
    /// 读到既不是 0 也不是 1 的「布尔」字节，几乎可以肯定是错位。
    #[error(
        "「{section}」偏移 {offset}（文件版本 {version}）读到非法布尔值 {value}，文件疑似错位或版本不符"
    )]
    InvalidBool {
        /// 出错时所在的段。
        section: &'static str,
        /// 出错时的绝对偏移。
        offset: usize,
        /// 文件头里的版本。
        version: i32,
        /// 读到的字节。
        value: u8,
    },
    /// 列表计数是负数或超出上限。
    #[error("「{section}」偏移 {offset}（文件版本 {version}）的计数 {value} 非法（上限 {limit}）")]
    CountOutOfRange {
        /// 出错时所在的段。
        section: &'static str,
        /// 计数字段的绝对偏移。
        offset: usize,
        /// 文件头里的版本。
        version: i32,
        /// 读到的计数。
        value: i64,
        /// 允许的上限。
        limit: usize,
    },
    /// 计数声明的记录数超出文件剩余字节能容纳的范围。
    #[error(
        "「{section}」偏移 {offset}（文件版本 {version}）声明 {count} 条记录，至少需要 {needed} 字节，但只剩 {remaining} 字节"
    )]
    ListOverrunsFile {
        /// 出错时所在的段。
        section: &'static str,
        /// 计数字段的绝对偏移。
        offset: usize,
        /// 文件头里的版本。
        version: i32,
        /// 声明的记录数。
        count: usize,
        /// 按最小记录体积估算所需字节数。
        needed: usize,
        /// 实际剩余字节数。
        remaining: usize,
    },
    /// 读取文件失败。
    #[error("读取数据库文件 {path} 失败：{source}")]
    Io {
        /// 文件路径。
        path: PathBuf,
        /// 底层 I/O 错误。
        #[source]
        source: std::io::Error,
    },
}

/// 文件头（固定 40 字节 = 10 × `int32`，见 `docs/store/mirdb-format.md` §1）。
///
/// 字段名与 `Envir.cs:2457-2466`（写出）/ `:2847-2865`（读入）一一对应。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MirDbHeader {
    /// 写出时的 `Envir.Version`（`Envir.cs:2457`）。语义见 [`MIN_VERSION`] / [`MAX_VERSION`]。
    pub version: i32,
    /// 写出时的 `Envir.CustomVersion`（`Envir.cs:2458`）。参考实现里没有任何分支使用它。
    pub custom_version: i32,
    /// 地图 ID 计数器（`Envir.cs:2459`）。
    pub map_index: i32,
    /// 物品模板 ID 计数器（`:2460`）。
    pub item_index: i32,
    /// 怪物 ID 计数器（`:2461`）。
    pub monster_index: i32,
    /// NPC ID 计数器（`:2462`）。
    pub npc_index: i32,
    /// 任务 ID 计数器（`:2463`）。
    pub quest_index: i32,
    /// 商城条目 ID 计数器（`>= 63`，`:2464` / `:2854-2857`）。
    pub gameshop_index: Option<i32>,
    /// 攻城区域 ID 计数器（`>= 66`，`:2465` / `:2859-2862`）。
    pub conquest_index: Option<i32>,
    /// 刷怪点 ID 计数器（`>= 68`，`:2466` / `:2864-2865`）。
    pub respawn_index: Option<i32>,
}

/// 一个段（含其 `i32` 计数前缀）在文件里的字节范围。
///
/// `docs/store/mirdb-format.md` §8.2 就是用这张表验证「逐段偏移与条数」的；
/// `mir-tools mirdb-report` 会把它们打印出来，测试也用它做逐字节往返比对。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SectionSpan {
    /// 段名（`maps` / `items` / `monsters` / `npcs` / `quests` / `dragon` / `magics` /
    /// `gameshop` / `conquests` / `respawn_timer` / `gt_maps`）。
    pub name: &'static str,
    /// 段首偏移（列表段指向 `i32` 计数）。
    pub range: Range<usize>,
    /// 条数（无计数的段恒为 1）。
    pub count: usize,
}

/// 一份完整的 `Server.MirDB`。
#[derive(Debug, Clone, Default, PartialEq)]
pub struct MirDb {
    /// 文件头。
    pub header: MirDbHeader,
    /// 地图段（`Envir.cs:2868-2871`）。
    pub maps: Vec<MapInfo>,
    /// 物品模板段（`Envir.cs:2873-2882`）。
    pub items: Vec<ItemInfo>,
    /// 怪物段（`Envir.cs:2883-2886`）。
    pub monsters: Vec<MonsterInfo>,
    /// NPC 段（`Envir.cs:2888-2891`）。
    pub npcs: Vec<NpcInfo>,
    /// 任务段（只有头部，`Envir.cs:2893-2896`）。
    pub quests: Vec<QuestInfo>,
    /// 龙系统（单条，无计数，`Envir.cs:2898`）。
    pub dragon: DragonInfo,
    /// 技能段（`Envir.cs:2899-2905`）。Crystal 会丢掉 `Spell` 重复的条目（`:2903`），
    /// 这里保留文件里的原始顺序与条数——数据清洗交给上层。
    pub magics: Vec<MagicInfo>,
    /// 商城段（`>= 63`，`Envir.cs:2911-2923`）。
    pub gameshop: Vec<GameShopItem>,
    /// 攻城段（`>= 66`，`Envir.cs:2925-2933`）。
    pub conquests: Vec<ConquestInfo>,
    /// 刷怪计时器（单条，无计数，`> 67`，`Envir.cs:2935-2936`）。
    pub respawn: Option<RespawnTimer>,
    /// GT 城尾段（`GTMap`）。Crystal **写但不读**（`Envir.cs:2503-2505`），
    /// 这里在文件尾还有 >= 4 字节时尝试解析，能完整吃下才收下（见 [`Self::parse_with_leftover`]）。
    pub gt_maps: Option<Vec<GtMap>>,
    /// 各段的字节范围，按文件顺序。
    pub sections: Vec<SectionSpan>,
}

impl MirDb {
    /// 从字节解析；**忽略**文件尾多出的字节（与 Crystal 的容忍度一致）。
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, MirDbError> {
        Self::parse_with_leftover(bytes).map(|(db, _)| db)
    }

    /// 从字节解析，并返回还有多少字节没被消费。
    ///
    /// 非 0 的 leftover 表示文件里有本解析器不认识的内容——要么是更新的版本，
    /// 要么是错位后恰好还能收尾的损坏文件。**不要**把它当成功信号。
    pub fn parse_with_leftover(bytes: &[u8]) -> Result<(Self, usize), MirDbError> {
        let mut r = DbReader::new(bytes, 0);
        r.set_section("文件头");

        let version = r.i32()?;
        let custom_version = r.i32()?;
        if !(MIN_VERSION..=MAX_VERSION).contains(&version) {
            return Err(MirDbError::UnsupportedVersion {
                version,
                min: MIN_VERSION,
                max: MAX_VERSION,
            });
        }
        r.set_version(version);

        let header = MirDbHeader {
            version,
            custom_version,
            map_index: r.i32()?,
            item_index: r.i32()?,
            monster_index: r.i32()?,
            npc_index: r.i32()?,
            quest_index: r.i32()?,
            gameshop_index: if version >= 63 { Some(r.i32()?) } else { None },
            conquest_index: if version >= 66 { Some(r.i32()?) } else { None },
            respawn_index: if version >= 68 { Some(r.i32()?) } else { None },
        };

        let mut sections = Vec::new();

        let (maps, span) = read_section(&mut r, "maps", map::MIN_MAP_RECORD_LEN, MapInfo::parse)?;
        sections.push(span);
        let (items, span) = read_section(&mut r, "items", item::MIN_ITEM_LEN, item::decode_item)?;
        sections.push(span);
        let (monsters, span) = read_section(
            &mut r,
            "monsters",
            monster::MIN_MONSTER_LEN,
            MonsterInfo::parse,
        )?;
        sections.push(span);
        let (npcs, span) = read_section(&mut r, "npcs", npc::MIN_NPC_LEN, NpcInfo::parse)?;
        sections.push(span);
        let (quests, span) =
            read_section(&mut r, "quests", quest::MIN_QUEST_LEN, QuestInfo::parse)?;
        sections.push(span);

        // 龙系统没有 `count`（`Envir.cs:2898`）。
        let start = r.position();
        r.set_section("dragon");
        let dragon = DragonInfo::parse(&mut r)?;
        sections.push(SectionSpan {
            name: "dragon",
            range: start..r.position(),
            count: 1,
        });

        let (magics, span) =
            read_section(&mut r, "magics", magic::MIN_MAGIC_LEN, MagicInfo::parse)?;
        sections.push(span);

        let mut gameshop = Vec::new();
        if version >= 63 {
            let (items, span) = read_section(
                &mut r,
                "gameshop",
                item::MIN_GAME_SHOP_LEN,
                item::decode_game_shop,
            )?;
            sections.push(span);
            gameshop = items;
        }

        let mut conquests = Vec::new();
        if version >= 66 {
            let (items, span) = read_section(
                &mut r,
                "conquests",
                conquest::MIN_CONQUEST_LEN,
                ConquestInfo::parse,
            )?;
            sections.push(span);
            conquests = items;
        }

        let respawn = if version > 67 {
            let start = r.position();
            r.set_section("respawn_timer");
            let timer = RespawnTimer::parse(&mut r)?;
            sections.push(SectionSpan {
                name: "respawn_timer",
                range: start..r.position(),
                count: 1,
            });
            Some(timer)
        } else {
            None
        };

        // 尾部的 GT 城段：Crystal 写进去但不读（`Envir.cs:2503-2505`），
        // 所以只有「恰好把剩余字节全部吃下」才认定它是 GT 段
        // （`docs/store/mirdb-format.md` §5 的建议）。
        let mut gt_maps = None;
        if r.remaining() >= 4 {
            let start = r.position();
            if let Some(maps) = try_parse_gt_maps(r.rest(), version) {
                let count = maps.len();
                r.consume_rest();
                sections.push(SectionSpan {
                    name: "gt_maps",
                    range: start..r.position(),
                    count,
                });
                gt_maps = Some(maps);
            }
        }

        let leftover = r.remaining();
        let db = Self {
            header,
            maps,
            items,
            monsters,
            npcs,
            quests,
            dragon,
            magics,
            gameshop,
            conquests,
            respawn,
            gt_maps,
            sections,
        };
        Ok((db, leftover))
    }

    /// 从文件解析；**忽略**文件尾多出的字节。
    pub fn load(path: impl AsRef<Path>) -> Result<Self, MirDbError> {
        Self::load_with_leftover(path).map(|(db, _)| db)
    }

    /// 从文件解析，并返回未被消费的字节数。
    pub fn load_with_leftover(path: impl AsRef<Path>) -> Result<(Self, usize), MirDbError> {
        let path = path.as_ref();
        let bytes = std::fs::read(path).map_err(|source| MirDbError::Io {
            path: path.to_path_buf(),
            source,
        })?;
        Self::parse_with_leftover(&bytes)
    }

    /// 按段名查字节范围。
    #[must_use]
    pub fn section(&self, name: &str) -> Option<&SectionSpan> {
        self.sections.iter().find(|span| span.name == name)
    }
}

/// 读一个「`i32` 计数 + `count` 条记录」的段，并记录字节范围。
fn read_section<'a, T>(
    r: &mut DbReader<'a>,
    name: &'static str,
    min_record_size: usize,
    read: impl FnMut(&mut DbReader<'a>) -> Result<T, MirDbError>,
) -> Result<(Vec<T>, SectionSpan), MirDbError> {
    let start = r.position();
    r.set_section(name);
    let items = r.list(min_record_size, read)?;
    let span = SectionSpan {
        name,
        range: start..r.position(),
        count: items.len(),
    };
    Ok((items, span))
}

/// 尝试把剩余字节整体解析成 `i32 count + GtMap[count]`（`Envir.cs:2503-2505`，
/// `GTMap.cs:27-51`）。必须正好吃光，否则返回 `None`（当作未知尾部数据）。
fn try_parse_gt_maps(bytes: &[u8], version: i32) -> Option<Vec<GtMap>> {
    let mut r = DbReader::new(bytes, version);
    r.set_section("gt_maps");
    let maps = r.list(MIN_GT_MAP_LEN, GtMap::parse).ok()?;
    if r.remaining() == 0 { Some(maps) } else { None }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 拼一个最小的 v60 文件：没有商城、攻城、刷怪计时器（`Envir.cs:263-266` 的版本门）。
    fn minimal_v60() -> Vec<u8> {
        let mut w = DbWriter::new();
        w.i32(60); // Version
        w.i32(0); // CustomVersion
        for _ in 0..5 {
            w.i32(0); // MapIndex / ItemIndex / MonsterIndex / NPCIndex / QuestIndex
        }
        for _ in 0..5 {
            w.i32(0); // maps / items / monsters / npcs / quests
        }
        DragonInfo::default().encode(&mut w);
        w.i32(0); // magics
        w.into_inner()
    }

    /// 拼一个最小的 v117 文件：所有段都在，GT 尾段由参数控制
    /// （`None` 表示连 `i32` 计数都不写，模拟「文件在计时器段就结束」）。
    fn minimal_v117(gt: Option<&[GtMap]>) -> Vec<u8> {
        let mut w = DbWriter::new();
        w.i32(117); // Version
        w.i32(0); // CustomVersion
        for _ in 0..8 {
            w.i32(0); // 8 个计数器（含 gameshop / conquest / respawn）
        }
        for _ in 0..5 {
            w.i32(0); // maps / items / monsters / npcs / quests
        }
        DragonInfo::default().encode(&mut w);
        w.i32(0); // magics
        w.i32(0); // gameshop
        w.i32(0); // conquests
        RespawnTimer::default().encode(&mut w);
        if let Some(gt) = gt {
            w.list(gt, |w, map| map.encode(w));
        }
        w.into_inner()
    }

    #[test]
    fn minimal_v60_parses_without_gated_sections() {
        let bytes = minimal_v60();
        let (db, leftover) = MirDb::parse_with_leftover(&bytes).expect("解析应当成功");
        assert_eq!(leftover, 0);
        assert_eq!(db.header.version, 60);
        assert_eq!(db.header.custom_version, 0);
        assert_eq!(db.header.gameshop_index, None, "< 63 没有 gameshop 计数器");
        assert_eq!(db.header.conquest_index, None, "< 66 没有 conquest 计数器");
        assert_eq!(db.header.respawn_index, None, "< 68 没有 respawn 计数器");
        assert!(db.gameshop.is_empty());
        assert!(db.conquests.is_empty());
        assert!(db.respawn.is_none(), "< 68 没有刷怪计时器段");
        assert!(db.gt_maps.is_none());
        assert_eq!(db.dragon, DragonInfo::default());
        assert_eq!(
            db.sections.iter().map(|span| span.name).collect::<Vec<_>>(),
            vec![
                "maps", "items", "monsters", "npcs", "quests", "dragon", "magics"
            ]
        );
        // v60 头部只有 8 + 5×4 = 28 字节（`>= 63` / `>= 66` / `>= 68` 的计数器不存在）。
        assert_eq!(db.section("maps").expect("必须有地图段").range.start, 28);
        assert_eq!(db.section("maps").expect("必须有地图段").range.end, 32);
    }

    #[test]
    fn minimal_v117_has_all_sections_and_gt_tail() {
        let gt = vec![GtMap {
            index: 1,
            key: 2,
            name: "GT".to_owned(),
            owner: "城主".to_owned(),
            leader: "会长".to_owned(),
            leader2: String::new(),
            price: 500,
            days: 7,
            begin: 1_700_000_000,
        }];
        let bytes = minimal_v117(Some(&gt));
        let (db, leftover) = MirDb::parse_with_leftover(&bytes).expect("解析应当成功");
        assert_eq!(leftover, 0);
        assert_eq!(db.header.gameshop_index, Some(0));
        assert_eq!(db.header.conquest_index, Some(0));
        assert_eq!(db.header.respawn_index, Some(0));
        assert_eq!(db.respawn, Some(RespawnTimer::default()));
        assert_eq!(db.gt_maps.as_deref(), Some(gt.as_slice()));
        assert_eq!(db.section("gt_maps").expect("应识别出 GT 尾段").count, 1);

        // 117 版 `SaveDB` 无条件写 `GTMapList.Count`（`Envir.cs:2503`），
        // 所以「计数为 0 的 GT 段」是合法且常见的：它存在但为空。
        let bytes = minimal_v117(Some(&[]));
        let (db, leftover) = MirDb::parse_with_leftover(&bytes).expect("解析应当成功");
        assert_eq!(leftover, 0);
        assert_eq!(db.gt_maps.as_deref(), Some([].as_slice()));
        assert_eq!(db.section("gt_maps").expect("空 GT 段也算一段").count, 0);

        // 文件在计时器段就结束（如 105 及更早的样本）时不该凭空造一个 GT 段。
        let bytes = minimal_v117(None);
        let (db, leftover) = MirDb::parse_with_leftover(&bytes).expect("解析应当成功");
        assert_eq!(leftover, 0);
        assert!(db.gt_maps.is_none());
        assert!(db.section("gt_maps").is_none());
    }

    #[test]
    fn trailing_bytes_are_reported_but_not_fatal() {
        // 3 个字节不足以构成 GT 段的 `i32 count`，只能算 leftover。
        let mut bytes = minimal_v60();
        bytes.extend_from_slice(&[0xDE, 0xAD, 0xBE]);
        let (db, leftover) = MirDb::parse_with_leftover(&bytes).expect("解析应当成功");
        assert_eq!(leftover, 3);
        assert!(db.gt_maps.is_none());
        // `from_bytes` / `load` 的语义是「容忍尾部多余字节」。
        assert!(MirDb::from_bytes(&bytes).is_ok());
    }

    #[test]
    fn bogus_versions_are_rejected() {
        for version in [59, 200] {
            let mut w = DbWriter::new();
            w.i32(version);
            w.i32(0);
            let bytes = w.into_inner();
            match MirDb::from_bytes(&bytes) {
                Err(MirDbError::UnsupportedVersion { version: seen, .. }) => {
                    assert_eq!(seen, version);
                }
                other => panic!("版本 {version} 应当被拒绝，实际 {other:?}"),
            }
        }
    }

    #[test]
    fn truncated_file_reports_offset_and_section() {
        let bytes = minimal_v60();
        // 去掉 5 个字节：龙系统段的最后一个 `i64` 少一个字节，随后魔法段的计数也读不到。
        let truncated = &bytes[..bytes.len() - 5];
        match MirDb::from_bytes(truncated) {
            Err(MirDbError::Truncated {
                section,
                offset,
                version,
                ..
            }) => {
                assert_eq!(section, "dragon", "截断发生在龙系统段的最后一个 i64 上");
                assert_eq!(version, 60);
                // 偏移指向失败字段的起点：最后一个 i64 从 179 开始，只留下 7 个字节。
                assert_eq!(offset, 179);
                assert_eq!(offset + 7, truncated.len());
            }
            other => panic!("截断文件应当报 Truncated，实际 {other:?}"),
        }

        // 连文件头都不完整时也一样报错。
        assert!(MirDb::from_bytes(&bytes[..5]).is_err());
    }

    #[test]
    fn non_bool_byte_means_misalignment() {
        // 先写一条合法的 v117 空地图，再把它的第一个布尔字段（`NoTeleport`）改成 2：
        // 记录内的偏移 = Index 4 + FileName 1 + Title 1 + MiniMap 2 + Light 1 + BigMap 2
        //                + SafeZones 4 + Respawns 4 + Movements 4 = 23。
        let mut writer = DbWriter::new();
        writer.i32(117);
        writer.i32(0);
        for _ in 0..8 {
            writer.i32(0);
        }
        writer.i32(1); // maps = 1
        let mut record = DbWriter::new();
        MapInfo::default().encode(&mut record, 117);
        let mut record = record.into_inner();
        assert_eq!(record[23], 0, "第 23 字节是 NoTeleport 的 false");
        record[23] = 2;
        writer.raw(&record);
        let bytes = writer.into_inner();

        match MirDb::from_bytes(&bytes) {
            Err(MirDbError::InvalidBool { value, section, .. }) => {
                assert_eq!(value, 2);
                assert_eq!(section, "maps");
            }
            other => panic!("非法布尔应当报 InvalidBool，实际 {other:?}"),
        }
    }

    #[test]
    fn absurd_count_is_rejected_before_allocating() {
        let mut w = DbWriter::new();
        w.i32(117);
        w.i32(0);
        for _ in 0..8 {
            w.i32(0);
        }
        w.i32(0x7FFF_FFFF); // maps = 2^31-1
        let bytes = w.into_inner();
        match MirDb::from_bytes(&bytes) {
            Err(MirDbError::CountOutOfRange { value, limit, .. }) => {
                assert_eq!(value, i64::from(i32::MAX));
                assert_eq!(limit, reader::MAX_LIST_LEN);
            }
            Err(MirDbError::ListOverrunsFile { count, .. }) => {
                assert_eq!(count, i32::MAX as usize);
            }
            other => panic!("离谱计数应当被拒绝，实际 {other:?}"),
        }
    }
}
