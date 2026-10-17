//! 地图文件读取器：逐行对齐 Crystal `Server/MirEnvir/Map.cs`（下称 `Map.cs`）。
//!
//! 地图文件的加载链路是 `MapInfo.CreateMap()` → `new Map(info)` → `Map.Load()`；
//! 真正碰文件的只有 `Load()`（`Map.cs:432-514`）：一次性读完整个文件，用 `FindType`
//! 判断版本（`Map.cs:73-110`），再分派到 `LoadMapCellsv0..v7` / `LoadMapCellsV100`。
//! 本模块实现的就是这一段，不包含安全区、刷新点、矿区等数据库侧的后处理。
//!
//! # 与 Crystal 的差异
//!
//! Crystal **不校验文件长度**（`Map.cs:432-514` 没有任何长度检查），文件被截断时会在逐格
//! 读取中抛异常，整个地图加载失败。这里把「数据不够」显式翻译成 [`MapError::BadSize`]；
//! **文件比布局更长时仍然接受**（多出的尾部字节被忽略），因为 Crystal 也是这样，而且
//! `resource/map` 下确实存在这种地图（例如 `EM001.map` 是 30×35，布局只占 12652 字节，
//! 文件却有 20692 字节）。
//!
//! # 版本分发（`FindType`，`Map.cs:73-110`）
//!
//! 按顺序短路匹配，全部不命中则返回 `0`。实测 `resource/map` 下 1113 个文件：
//! v0 848 个、v3 198 个、v2 65 个、v5 2 个（那 2 个其实是 zlib 压缩数据被误判）。
//!
//! | 版本 | `FindType` 返回值 | 首格偏移 | 每格字节 | 门字节 | 光照字节 | 布局来源 |
//! |---|---|---|---|---|---|---|
//! | [`MapVersion::V0`] | 0 | 52 | 12 | `o+8` | `o+11` | `Map.cs:112-152` |
//! | [`MapVersion::V1`] | 1 | 54 | 15 | `o+8` | `o+13` | `Map.cs:154-193` |
//! | [`MapVersion::V2`] | 2 | 52 | 14 | `o+6` | `o+11` | `Map.cs:195-234` |
//! | [`MapVersion::V3`] | 3 | 52 | 36 | `o+6` | `o+18` | `Map.cs:236-274` |
//! | [`MapVersion::V4`] | 4 | 64 | 12 | `o+6` | 无 | `Map.cs:276-307` |
//! | [`MapVersion::V5`] | 5 | `28 + 3·ceil(W/2)·(H/2)` | 14 | 无 | `o+13` | `Map.cs:309-335` |
//! | [`MapVersion::V6`] | 6 | 40 | 20 | 无 | 无 | `Map.cs:337-359` |
//! | [`MapVersion::V7`] | 7 | 54 | 15 | `o+8` | `o+12` | `Map.cs:361-394` |
//! | [`MapVersion::V100`] | 100 | 8 | 26 | `o+14` | `o+25` | `Map.cs:396-430` |
//!
//! # 单元格顺序：列优先（column-major）
//!
//! **文件里第 `i` 个格子对应 `(x, y)`，且 `i = x * Height + y`。** 依据：
//!
//! * 单元格存进二维数组 `Cells = new Cell[Width, Height]`（`Map.cs:118`、`Map.cs:201`、
//!   `Map.cs:243`……），C# 多维数组的最后一个下标变化最快，所以 `Cells[x, y]` 的线性地址
//!   就是 `x * Height + y`。
//! * 解析循环是 `for (int x = 0; x < Width; x++) for (int y = 0; y < Height; y++)`
//!   （`Map.cs:123-124`、`Map.cs:170-171`、`Map.cs:206-207` 等），而每个格子的读取只用
//!   一个单调递增的 `offSet` 游标（v0 见 `Map.cs:121`、`Map.cs:126-151`，每格 `offSet`
//!   前进 12 字节），也就是**文件顺序 = `Cells[x, y]` 的线性顺序**。
//! * 读坐标时也是 `GetCell(x, y) => Cells[x, y]`（`Map.cs:632-635`），`ValidPoint` 用
//!   `x < Width && y < Height` 判边界（`Map.cs:637-644`），与上面的顺序一致。
//!
//! 实测佐证：`resource/map/0100.map`（15×18）的 `(7, 6)` 用列优先取到
//! `00 00 00 00 3C 0E …`（前层图号 `0x0E3C`、可走），用行优先会取到另一格
//! `00 80 00 00 11 8E …`（高墙）；`resource/map/0.map` 的 `(265, 261)` 用列优先取到
//! `57 03 00 00 A4 0A 00 00 8A 00 00 05`，与 `docs/world/map-format.md` 的抽样完全一致。
//!
//! # v0 单元格 12 字节布局（`Map.cs:112-152`）
//!
//! | 格内偏移 | 大小 | 服务器行为 | 代码 |
//! |---|---|---|---|
//! | `o+0..o+1` | `u16` | bit15 → `HighWall`（注释 *Can Fire Over*） | `Map.cs:126-129` |
//! | `o+2..o+3` | `u16` | bit15 → `LowWall`（注释 *Can't Fire Over*） | `Map.cs:130-133` |
//! | `o+4..o+5` | `u16` | bit15 → `HighWall`（注释 *No Floor Tile*） | `Map.cs:135-136` |
//! | `o+6..o+7` | 2 字节 | 不读（`offSet += 4` 一并跳过） | `Map.cs:140` |
//! | `o+8` | `u8` | `> 0` → `AddDoor(值 & 0x7F, (x, y))` | `Map.cs:142-143` + `Map.cs:52` |
//! | `o+9..o+10` | 2 字节 | 不读（`offSet += 3` 一并跳过） | `Map.cs:145` |
//! | `o+11` | `u8` | `100..=119` → `FishingAttribute = 值 - 100` | `Map.cs:147-150` |
//!
//! 三次墙体判定是**后写覆盖**：`o+4` 的 bit15 一旦置位，结果一定是 `HighWall`。
//! 等价写法就是内部的 `WallRule::OverrideWords`：三个 bit15 全为 0 才是 `Walk`。
//!
//! 门字节的偏移有一处历史分歧：客户端 `MapCode.cs:248` 在 v0 读 `o+6`，服务器在 v0 读
//! `o+8`（v2/v3/v4 两边都读 `o+6`）。实测数据支持服务器的 `o+8`（见
//! `docs/world/map-format.md` §4.4），这里按服务器实现，并保留 `o+6`/`o+7`/`o+9`/`o+10`
//! 的原始字节（客户端语义），详见 [`MapCell::client_door_index`] 等字段。
//!
//! # 门与光照
//!
//! * 门：服务器只用门字节建 `List<Door>`（`Map.cs:50-59`），同一索引复用同一个 `Door`；
//!   **通行判定是另一件事**（`CheckDoorOpen`，`Map.cs:646-651`），本模块不做运行时门状态。
//! * 光照：地图文件里只有一个每格字节，服务器只拿它算钓鱼属性（`Map.cs:147-150`）；
//!   地图级光照在数据库的 `MapInfo.Light`（`MapInfo.cs:21`），不在 `.map` 文件里。

use std::fs;
use std::path::Path;

use mir_core::{CellAttribute, Point};
use thiserror::Error;
use tracing::{debug, warn};

/// `FindType` 至少要读到的字节数：最远的判定用到 `input[19]`（`Map.cs:88-96`）。
const MIN_HEADER_BYTES: usize = 20;

/// `AddDoor` 里的掩码：`DoorIndex &= 0x7F`（`Map.cs:52`）。
const DOOR_INDEX_MASK: u8 = 0x7F;

/// 客户端 v0/v2/v3/v4 单元格里附加字节的格内偏移（`MapCode.cs:248-253`）。
const AUX_MIR2: ClientAux = ClientAux {
    door_index: 6,
    door_offset: 7,
    front_animation_tick: 9,
    front_index: 10,
};

/// 客户端 v1/v7 单元格里附加字节的格内偏移（`MapCode.cs:297-302`、`MapCode.cs:600-605`）。
const AUX_LEGACY_15: ClientAux = ClientAux {
    door_index: 8,
    door_offset: 9,
    front_animation_tick: 11,
    front_index: 12,
};

/// `FindType`（`Map.cs:73-110`）识别出的地图版本。
///
/// 数值与 `FindType` 的返回值一致，可用 [`MapVersion::raw`] 取回、用
/// [`MapVersion::from_raw`] 转换。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum MapVersion {
    /// 老版 Mir2：52 字节头 + 每格 12 字节（`Map.cs:112-152`）。`resource/map` 下 848 个文件被
    /// 识别为 v0，其中 847 个能通过尺寸校验（`gongNkuang02.map` 的头部是垃圾数据）。
    V0,
    /// Wemade 2010（标题 `Map 2010 Ver 1.0`）：54 字节头 + 每格 15 字节（`Map.cs:154-193`）。
    V1,
    /// 老 Shanda：52 字节头 + 每格 14 字节（`Map.cs:195-234`）。
    V2,
    /// Shanda 2012：52 字节头 + 每格 36 字节（`Map.cs:236-274`）。
    V3,
    /// Wemade `AntiHack` 迷宫图（标题 `Mir2 AntiHack`）：64 字节头 + 每格 12 字节（`Map.cs:276-307`）。
    V4,
    /// Wemade Mir3：先跳过瓦片索引表，再每格 14 字节（`Map.cs:309-335`）。
    V5,
    /// Shanda Mir3（标题 `(C) SNDA, MIR3.`）：40 字节头 + 每格 20 字节（`Map.cs:337-359`）。
    V6,
    /// 3/4 Heroes（Myth/Lifcos）：54 字节头 + 每格 15 字节（`Map.cs:361-394`）。
    V7,
    /// Crystal 自定义格式：8 字节头 + 每格 26 字节（`Map.cs:396-430`）。
    V100,
}

impl MapVersion {
    /// 取回 `FindType` 的原始返回值（0..=7 或 100）。
    #[must_use]
    pub const fn raw(self) -> u16 {
        match self {
            Self::V0 => 0,
            Self::V1 => 1,
            Self::V2 => 2,
            Self::V3 => 3,
            Self::V4 => 4,
            Self::V5 => 5,
            Self::V6 => 6,
            Self::V7 => 7,
            Self::V100 => 100,
        }
    }

    /// 把 `FindType` 的原始返回值转成版本；未知值返回 `None`。
    #[must_use]
    pub const fn from_raw(raw: u16) -> Option<Self> {
        match raw {
            0 => Some(Self::V0),
            1 => Some(Self::V1),
            2 => Some(Self::V2),
            3 => Some(Self::V3),
            4 => Some(Self::V4),
            5 => Some(Self::V5),
            6 => Some(Self::V6),
            7 => Some(Self::V7),
            100 => Some(Self::V100),
            _ => None,
        }
    }
}

impl core::fmt::Display for MapVersion {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(formatter, "v{}", self.raw())
    }
}

/// 地图加载错误。
#[derive(Debug, Error)]
pub enum MapError {
    /// 读文件失败。
    #[error("读取地图文件失败：{0}")]
    Io(#[from] std::io::Error),

    /// 文件太短，连版本都判断不了（`FindType` 最多要读到 `input[19]`，`Map.cs:88-96`）。
    #[error("地图文件过短：识别版本至少需要 {needed} 字节，实际 {actual} 字节")]
    TooShort {
        /// 实际字节数。
        actual: usize,
        /// 识别版本所需的最小字节数。
        needed: usize,
    },

    /// `FindType` 返回了本实现不认识的版本号。
    ///
    /// Crystal 的 `FindType` 目前只返回 0..=7 与 100（`Map.cs:73-110`），这个变体留给
    /// 将来新增的格式。
    #[error("不支持的地图版本 {0}（Map.cs:73-110 的 FindType 返回值）")]
    UnsupportedVersion(u16),

    /// Crystal 自定义格式（`FindType` 返回 100）只支持 payload 版本 1.0。
    ///
    /// `LoadMapCellsV100` 在 `Bytes[0] != 1 || Bytes[1] != 0` 时直接 `return`（`Map.cs:399`），
    /// 随后 `GetWalkableCells` 会因 `Cells == null` 抛异常——也就是整张图加载失败。
    #[error("v100 地图只支持 payload 版本 1.0，实际为 {major}.{minor}（Map.cs:399）")]
    UnsupportedPayloadVersion {
        /// `Bytes[0]`。
        major: u8,
        /// `Bytes[1]`。
        minor: u8,
    },

    /// 头里读出的宽或高为 0。
    #[error("{version} 地图宽高非法：宽 {width}、高 {height}")]
    InvalidDimensions {
        /// 已识别出的版本。
        version: MapVersion,
        /// 从头部读到的宽度。
        width: u32,
        /// 从头部读到的高度。
        height: u32,
    },

    /// 按该版本布局计算，文件数据不足（Crystal 靠 `IndexOutOfRangeException` 兜底，`Map.cs:506-513`）。
    ///
    /// 注意：文件**比布局长**不会报错，多出的尾部字节按 Crystal 的行为忽略。
    #[error(
        "{version} 布局需要 {expected} 字节（{width}×{height}，每格 {stride} 字节），实际只有 {actual} 字节"
    )]
    BadSize {
        /// 布局所属版本。
        version: MapVersion,
        /// 头部声明的宽度。
        width: u32,
        /// 头部声明的高度。
        height: u32,
        /// 每格字节数。
        stride: usize,
        /// 布局需要的字节数。
        expected: usize,
        /// 文件实际字节数。
        actual: usize,
    },

    /// 头部字段越界（文件刚好够识别版本、却不够读宽高或瓦片表偏移）。
    #[error(
        "{version} 头部越界：在偏移 {offset} 读 {needed} 字节时超出文件末尾（共 {actual} 字节）"
    )]
    Truncated {
        /// 已识别出的版本。
        version: MapVersion,
        /// 越界的读取位置。
        offset: usize,
        /// 需要读取的字节数。
        needed: usize,
        /// 文件实际字节数。
        actual: usize,
    },
}

/// 单元格：服务器语义（`Cell`，`Map.cs:2475-2531`）+ 文件里保留的渲染字节。
///
/// 与 Crystal 一致，只有 [`attribute`](Self::attribute) 影响可走性（`Cell.Valid`，
/// `Map.cs:2480-2483`）；`HighWall` 与 `LowWall` 在服务器里没有任何行为差异
/// （全仓库只比较 `Attribute == Walk`：`Map.cs:524`）。
///
/// 字段按版本填充情况：
///
/// | 字段 | v0 | v1 | v2 | v3 | v4 | v5 | v6 | v7 | v100 |
/// |---|---|---|---|---|---|---|---|---|---|
/// | `attribute` | ✔ | ✔ | ✔ | ✔ | ✔ | ✔ | ✔ | ✔ | ✔ |
/// | `fishing_attribute` / `light` | ✔ | ✔ | ✔ | ✔ | — | ✔ | — | ✔ | ✔ |
/// | `door_raw` / `door_index` | ✔ | ✔ | ✔ | ✔ | ✔ | — | — | ✔ | ✔ |
/// | 三个图像号 | ✔ | — | ✔ | ✔ | ✔ | — | — | — | — |
/// | `client_door_index` 等 | ✔ | ✔ | ✔ | ✔ | ✔ | — | — | ✔ | — |
///
/// 图像号只对 v0/v2/v3（未异或）与 v4（异或头里的 `xor`）按客户端代码填充
/// （`MapCode.cs:242-246`、`MapCode.cs:337-341`、`MapCode.cs:379-383`、`MapCode.cs:432-436`）；
/// 其余版本的客户端把后层图号存成 `int`（`MapCode.cs:293`、`MapCode.cs:596`、`MapCode.cs:639`），
/// 单个 `u16` 放不下，因此留 0。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MapCell {
    /// 墙体属性（`Cell.Attribute`，`Map.cs:2486`）：`Walk` / `HighWall` / `LowWall`。
    pub attribute: CellAttribute,

    /// 钓鱼属性（`Cell.FishingAttribute`，`Map.cs:2487`）：默认 [`MapCell::FISHING_ATTRIBUTE_NONE`]；
    /// 服务器读到光照字节落在 `100..=119` 时写入 `光照 - 100`（`Map.cs:149-150`）。
    pub fishing_attribute: i8,

    /// 服务器读到的门字节原始值：v0 取 `o+8`（`Map.cs:142`）。
    ///
    /// 客户端 v0 把同一字节叫 `FrontAnimationFrame`（`MapCode.cs:250`），门的判定以服务器为准。
    pub door_raw: u8,

    /// 服务器门索引：只有 [`door_raw`](Self::door_raw) `> 0` 时才是
    /// `Some(door_raw & 0x7F)`（`Map.cs:142-143` 的 `> 0` 判断 + `Map.cs:52` 的掩码）。
    ///
    /// 原始字节 `0x80` 会得到索引 `0`（`0.map` 里有 2 个这样的格子，Crystal 会造出一个索引 0 的门）。
    pub door_index: Option<u8>,

    /// 服务器读取的光照字节：v0/v2 的 `o+11`、v3 的 `o+18`、v5 的 `o+13`、v7 的 `o+12`、
    /// v100 的 `o+25`；v4/v6 不读光照，恒为 0。服务器只拿它算钓鱼属性。
    pub light: u8,

    /// 后层图像号（v0 `o+0..o+1`，客户端叫 `BackImage`，`MapCode.cs:242`）。
    pub back_image: u16,

    /// 中层图像号（v0 `o+2..o+3`，`MapCode.cs:244`）。
    pub middle_image: u16,

    /// 前层图像号（v0 `o+4..o+5`，`MapCode.cs:246`）。
    pub front_image: u16,

    /// 客户端语义的门索引（v0/v2/v3/v4 的 `o+6 & 0x7F`，`MapCode.cs:248`）。
    ///
    /// 服务器 v0 不读这个字节（它读 `o+8`），只有 v2/v3/v4 的服务器实现与客户端一致。
    /// 详见 `docs/world/map-format.md` §4.4 与 [`MapCell::door_raw`]。
    pub client_door_index: u8,

    /// 客户端语义的门偏移（v0/v2/v3/v4 的 `o+7`，`MapCode.cs:249`）；服务器不读。
    pub door_offset: u8,

    /// 客户端语义的前层动画计时（v0/v2/v3/v4 的 `o+9`，`MapCode.cs:251`）；服务器不读。
    pub front_animation_tick: u8,

    /// 客户端语义的前层瓦片库原始字节（v0/v2/v3/v4 的 `o+10`）。
    ///
    /// 客户端会再加偏移才当成瓦片库序号（v0 是 `+2`，`MapCode.cs:252`；v2/v3 是 `+120`，
    /// `MapCode.cs:347`、`MapCode.cs:389`），这里保留原始字节。
    pub front_index: u8,
}

impl MapCell {
    /// `Cell.FishingAttribute` 的默认值（`Map.cs:2487`）。
    pub const FISHING_ATTRIBUTE_NONE: i8 = -1;

    /// 是否可走，等价于 `Cell.Valid`（`Map.cs:2480-2483`）。
    #[must_use]
    pub const fn is_walkable(&self) -> bool {
        matches!(self.attribute, CellAttribute::Walk)
    }

    /// 是否高墙（`CellAttribute::HighWall`）。
    #[must_use]
    pub const fn is_high_wall(&self) -> bool {
        matches!(self.attribute, CellAttribute::HighWall)
    }

    /// 是否低墙（`CellAttribute::LowWall`）。
    #[must_use]
    pub const fn is_low_wall(&self) -> bool {
        matches!(self.attribute, CellAttribute::LowWall)
    }
}

impl Default for MapCell {
    fn default() -> Self {
        Self {
            attribute: CellAttribute::Walk,
            fishing_attribute: Self::FISHING_ATTRIBUTE_NONE,
            door_raw: 0,
            door_index: None,
            light: 0,
            back_image: 0,
            middle_image: 0,
            front_image: 0,
            client_door_index: 0,
            door_offset: 0,
            front_animation_tick: 0,
            front_index: 0,
        }
    }
}

/// 门：对应 Crystal 的 `Door`（`Map.cs:50-71`），同一索引的所有格子共享一个门。
///
/// Crystal 的 `Door` 还有 `DoorState` / `LastTick` 等运行时状态（`Map.cs:61-71`、
/// `Map.cs:653-665`），这里只记录加载阶段得到的信息；开关门逻辑属于世界运行时的职责。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Door {
    /// 门索引：原始字节 `& 0x7F`（`Map.cs:52`）。
    pub index: u8,
    /// 该索引第一次出现的格子（`AddDoor` 只在新建时记录 `Location`，`Map.cs:56`）。
    pub location: Point,
}

/// 一张已解析的地图。
#[derive(Debug, Clone)]
pub struct Map {
    /// 宽（`Map.cs:115` 等，各版本头部字段见模块文档的版本表）。
    pub width: i32,
    /// 高。
    pub height: i32,
    /// 单元格，**按列优先存放**：`cells[x * height + y]`（推导见模块文档）。
    pub cells: Vec<MapCell>,
    /// 地图名，不含 `.map` 后缀（对应 `MapInfo.FileName`：文件名由
    /// `Path.Combine(Settings.MapPath, Info.FileName + ".map")` 拼出，`Map.cs:436`）。
    pub file_name: String,
    /// 版本。
    pub version: MapVersion,
    /// 加载阶段按 `AddDoor` 规则收集到的门表（同索引复用，`Map.cs:50-59`）。
    pub doors: Vec<Door>,
}

impl Map {
    /// 从磁盘读取并解析地图文件。
    ///
    /// 对应 `Map.Load()` 里 `File.ReadAllBytes` + `FindType` + `LoadMapCells*` 这一段
    /// （`Map.cs:439-469`）；安全区、刷新点等数据库相关后处理不在本函数内。
    pub fn load(path: &Path) -> Result<Self, MapError> {
        // io::Error 不携带路径，这里把路径塞进 message，同时保留 kind（NotFound 等）。
        let data = fs::read(path).map_err(|source| {
            MapError::Io(std::io::Error::new(
                source.kind(),
                format!("{}：{source}", path.display()),
            ))
        })?;
        // Crystal 的地图名不含后缀（`Info.FileName + ".map"`，`Map.cs:436`），所以取 file_stem。
        let file_name = path.file_stem().map_or_else(
            || path.display().to_string(),
            |stem| stem.to_string_lossy().into_owned(),
        );
        parse(&data, &file_name)
    }

    /// 直接解析内存中的地图数据（不读文件）。
    ///
    /// 文件名记为 `"<memory>"`；需要指定名字时用 [`Map::from_bytes_named`]。
    pub fn from_bytes(data: &[u8]) -> Result<Self, MapError> {
        parse(data, "<memory>")
    }

    /// 直接解析内存中的地图数据，并指定地图名。
    pub fn from_bytes_named(data: &[u8], file_name: &str) -> Result<Self, MapError> {
        parse(data, file_name)
    }

    /// 取某格的引用；越界返回 `None`（对应 `Map.cs:632-635` 的 `GetCell` + `Map.cs:637-644`
    /// 的边界判断，但这里不检查门）。
    #[must_use]
    pub fn cell(&self, x: i32, y: i32) -> Option<&MapCell> {
        if x < 0 || y < 0 || x >= self.width || y >= self.height {
            return None;
        }
        // 列优先：i = x * Height + y
        let height = self.height as usize;
        self.cells.get(x as usize * height + y as usize)
    }

    /// 是否可走：在边界内且格子不是墙（等价于 `ValidPoint`，`Map.cs:637-644`）。
    ///
    /// 与 Crystal 一致，这里**不检查门**：门是否打开是另一层判断
    /// （`CheckDoorOpen`，`Map.cs:646-651`），需要门状态时由调用方叠加。
    #[must_use]
    pub fn walkable(&self, x: i32, y: i32) -> bool {
        self.cell(x, y).is_some_and(MapCell::is_walkable)
    }

    /// 枚举所有可走格，等价于 `GetWalkableCells`（`Map.cs:516-527`），顺序也是
    /// `for x { for y { } }`。
    pub fn walkable_cells(&self) -> impl Iterator<Item = Point> + '_ {
        let height = self.height.max(1) as usize;
        self.cells
            .iter()
            .enumerate()
            .filter(|(_, cell)| cell.is_walkable())
            .map(move |(index, _)| Point::new((index / height) as i32, (index % height) as i32))
    }
}

/// 单元格解码参数，由 [`plan_for`] 按版本生成。
#[derive(Debug)]
struct Plan {
    version: MapVersion,
    width: u32,
    height: u32,
    /// 首格在文件中的偏移（v5 已经跳过瓦片索引表）。
    start: usize,
    /// 每格字节数。
    stride: usize,
    /// 墙体判定规则。
    walls: WallRule,
    /// 服务器读取门字节的格内偏移；`None` 表示该版本不读门。
    door_at: Option<usize>,
    /// 服务器读取光照/钓鱼字节的格内偏移；`None` 表示该版本不读光照。
    light_at: Option<usize>,
    /// `Some(xor)` 表示 `o+0`/`o+2`/`o+4` 是三个图像号，读取后要与 `xor` 异或。
    image_xor: Option<u16>,
    /// 客户端附加字节的格内偏移（服务器不读，只做保留）。
    aux: Option<ClientAux>,
}

/// 墙体判定规则：每个变体对应 `Map.cs` 里的一段判定代码。
#[derive(Debug, Clone, Copy)]
enum WallRule {
    /// v0/v2/v3：`o+0` bit15 → `HighWall`，`o+2` bit15 → `LowWall`，`o+4` bit15 → `HighWall`
    /// （`Map.cs:126-136`、`Map.cs:209-218`、`Map.cs:250-259`，后写覆盖前写）。
    OverrideWords,
    /// v1：`u32@0 ^ 0xAA38AA38` 的 bit29 → `HighWall`（`Map.cs:173-174`），
    /// `(u16@6 ^ xor)` 的 bit15 → `LowWall`（`Map.cs:177-178`）。
    Wemade2010 { xor: u16 },
    /// v4（低墙字在 `o+2`，`Map.cs:294-299`）与 v7（低墙字在 `o+6`，`Map.cs:375-379`）：
    /// `u16@0` bit15 → `HighWall`，`u16@low` bit15 → `LowWall`。
    TwoWords { low: usize },
    /// v5/v6：`byte@0` 的 bit0 为 0 → `HighWall`，否则 bit1 为 0 → `LowWall`，否则 `Walk`
    /// （`Map.cs:322-327`、`Map.cs:351-356`）。
    Mir3Bits,
    /// v100：`u32@2` 的 bit29 → `HighWall`（`Map.cs:412-413`），`u16@12` 的 bit15 → `LowWall`
    /// （`Map.cs:415-416`）。
    CrystalV100,
}

/// 客户端单元格里 4 个附加字节的格内偏移（服务器不读，只做保留）。
#[derive(Debug, Clone, Copy)]
struct ClientAux {
    door_index: usize,
    door_offset: usize,
    front_animation_tick: usize,
    front_index: usize,
}

/// 解析入口：版本识别 → 布局规划（含尺寸校验）→ 逐格解码 → 收集门表。
fn parse(data: &[u8], file_name: &str) -> Result<Map, MapError> {
    let version = find_type(data)?;
    let plan = match plan_for(version, data) {
        Ok(plan) => plan,
        Err(error) => rescue_legacy_v0(data, version, error)?,
    };

    let width = plan.width as usize;
    let height = plan.height as usize;
    let mut cells: Vec<MapCell> = Vec::with_capacity(width * height);

    let mut cell_index: usize = 0;
    for _x in 0..width {
        for _y in 0..height {
            // 列优先：文件里第 cell_index 个格子就是 (x, y)，cell_index = x * Height + y。
            let offset = plan.start + cell_index * plan.stride;
            let cell = decode_cell(data, offset, &plan).ok_or_else(|| MapError::BadSize {
                version: plan.version,
                width: plan.width,
                height: plan.height,
                stride: plan.stride,
                expected: expected_len(&plan),
                actual: data.len(),
            })?;
            cells.push(cell);
            cell_index += 1;
        }
    }

    let doors = collect_doors(&cells, height);

    Ok(Map {
        width: plan.width as i32,
        height: plan.height as i32,
        cells,
        file_name: String::from(file_name),
        version: plan.version,
        doors,
    })
}

/// `Map.cs:73-110` 的逐行移植。
fn find_type(data: &[u8]) -> Result<MapVersion, MapError> {
    if data.len() < MIN_HEADER_BYTES {
        return Err(MapError::TooShort {
            actual: data.len(),
            needed: MIN_HEADER_BYTES,
        });
    }

    // C# 自定义格式（`Map.cs:76-79`）
    if data[2] == 0x43 && data[3] == 0x23 {
        return Ok(MapVersion::V100);
    }
    // Wemade Mir3：没有标题，文件以空白字节开头（`Map.cs:81-82`）
    if data[0] == 0 {
        return Ok(MapVersion::V5);
    }
    // Shanda Mir3：标题 `(C) SNDA, MIR3.`（`Map.cs:84-85`）
    if data[0] == 0x0F && data[5] == 0x53 && data[14] == 0x33 {
        return Ok(MapVersion::V6);
    }
    // Wemade AntiHack 迷宫图：标题 `Mir2 AntiHack`（`Map.cs:88-89`）
    if data[0] == 0x15 && data[4] == 0x32 && data[6] == 0x41 && data[19] == 0x31 {
        return Ok(MapVersion::V4);
    }
    // Wemade 2010：标题 `Map 2010 Ver 1.0`（`Map.cs:92-93`）
    if data[0] == 0x10 && data[2] == 0x61 && data[7] == 0x31 && data[14] == 0x31 {
        return Ok(MapVersion::V1);
    }
    // Shanda 2012 与一种更老的 Shanda/Wemade 格式共用头部，只能靠文件大小区分
    // （`Map.cs:96-104`）。C# 里 `A || B && C && D` 等价于 `A || (B && C && D)`，
    // 也就是说单凭 `data[4] == 0x0F` 就会命中这个分支。
    if data[4] == 0x0F || (data[4] == 0x03 && data[18] == 0x0D && data[19] == 0x0A) {
        let width = u16::from_le_bytes([data[0], data[1]]);
        let height = u16::from_le_bytes([data[2], data[3]]);
        // `input.Length > 52 + W * H * 14` 时是 v3，否则 v2（`Map.cs:100-103`）。
        let v2_len = usize::from(width)
            .saturating_mul(usize::from(height))
            .saturating_mul(14)
            .saturating_add(52);
        if data.len() > v2_len {
            return Ok(MapVersion::V3);
        }
        return Ok(MapVersion::V2);
    }
    // 3/4 Heroes（Myth/Lifcos）：`Map.cs:107-108`
    if data[0] == 0x0D && data[1] == 0x4C && data[7] == 0x20 && data[11] == 0x6D {
        return Ok(MapVersion::V7);
    }
    // 其余都是老版 Mir2 的 12 字节格式（`Map.cs:109`）
    Ok(MapVersion::V0)
}

/// 按版本解析头部并校验文件长度，得到逐格解码参数。
fn plan_for(version: MapVersion, data: &[u8]) -> Result<Plan, MapError> {
    let plan = match version {
        // 老版 Mir2：`Width = Int16@0`、`Height = Int16@2`（`Map.cs:115-117`），
        // 单元格从 52 开始（`Map.cs:121`），每格 12 字节（`Map.cs:125`）。
        MapVersion::V0 => Plan {
            version,
            width: u32::from(read_u16(data, 0, version)?),
            height: u32::from(read_u16(data, 2, version)?),
            start: 52,
            stride: 12,
            walls: WallRule::OverrideWords,
            door_at: Some(8),
            light_at: Some(11),
            image_xor: Some(0),
            aux: Some(AUX_MIR2),
        },
        // Wemade 2010：`Width = w ^ xor`、`Height = h ^ xor`，`w`/`xor`/`h` 在 21/23/25
        // （`Map.cs:156-164`）；单元格从 54 开始（`Map.cs:168`），每格 15 字节（`Map.cs:170-192`）。
        MapVersion::V1 => {
            let width_word = read_u16(data, 21, version)?;
            let xor = read_u16(data, 23, version)?;
            let height_word = read_u16(data, 25, version)?;
            Plan {
                version,
                width: u32::from(width_word ^ xor),
                height: u32::from(height_word ^ xor),
                start: 54,
                stride: 15,
                walls: WallRule::Wemade2010 { xor },
                door_at: Some(8),
                light_at: Some(13),
                image_xor: None,
                aux: Some(AUX_LEGACY_15),
            }
        }
        // 老 Shanda：宽高同 v0（`Map.cs:198-200`），单元格从 52 开始，每格 14 字节
        // （`Map.cs:204-233`）。
        MapVersion::V2 => Plan {
            version,
            width: u32::from(read_u16(data, 0, version)?),
            height: u32::from(read_u16(data, 2, version)?),
            start: 52,
            stride: 14,
            walls: WallRule::OverrideWords,
            door_at: Some(6),
            light_at: Some(11),
            image_xor: Some(0),
            aux: Some(AUX_MIR2),
        },
        // Shanda 2012：宽高同 v0（`Map.cs:239-241`），单元格从 52 开始，每格 36 字节
        // （`Map.cs:245-273`）。
        MapVersion::V3 => Plan {
            version,
            width: u32::from(read_u16(data, 0, version)?),
            height: u32::from(read_u16(data, 2, version)?),
            start: 52,
            stride: 36,
            walls: WallRule::OverrideWords,
            door_at: Some(6),
            light_at: Some(18),
            image_xor: Some(0),
            aux: Some(AUX_MIR2),
        },
        // Wemade AntiHack：宽高 = `w ^ xor`，`w`/`xor`/`h` 在 31/33/35（`Map.cs:278-285`）；
        // 单元格从 64 开始（`Map.cs:289`），每格 12 字节（`Map.cs:291-306`），不读光照。
        MapVersion::V4 => {
            let width_word = read_u16(data, 31, version)?;
            let xor = read_u16(data, 33, version)?;
            let height_word = read_u16(data, 35, version)?;
            Plan {
                version,
                width: u32::from(width_word ^ xor),
                height: u32::from(height_word ^ xor),
                start: 64,
                stride: 12,
                walls: WallRule::TwoWords { low: 2 },
                door_at: Some(6),
                light_at: None,
                image_xor: Some(xor),
                aux: Some(AUX_MIR2),
            }
        }
        // Wemade Mir3：宽高在 22/24（`Map.cs:311-314`）；先跳过 3 字节 × ceil(W/2) × (H/2)
        // 的瓦片索引表（`Map.cs:318`），之后每格 14 字节（`Map.cs:319-334`）。Mir3 没有门。
        MapVersion::V5 => {
            let width = u32::from(read_u16(data, 22, version)?);
            let height = u32::from(read_u16(data, 24, version)?);
            let half_width = width / 2 + width % 2;
            let tile_table = 3 * u64::from(half_width) * u64::from(height / 2);
            Plan {
                version,
                width,
                height,
                start: usize::try_from(28 + tile_table).unwrap_or(usize::MAX),
                stride: 14,
                walls: WallRule::Mir3Bits,
                door_at: None,
                light_at: Some(13),
                image_xor: None,
                aux: None,
            }
        }
        // Shanda Mir3：宽高在 16/18（`Map.cs:339-341`），单元格从 40 开始（`Map.cs:346`），
        // 每格 20 字节（`Map.cs:348-358`），只有 1 字节墙体位图，不读门与光照。
        MapVersion::V6 => Plan {
            version,
            width: u32::from(read_u16(data, 16, version)?),
            height: u32::from(read_u16(data, 18, version)?),
            start: 40,
            stride: 20,
            walls: WallRule::Mir3Bits,
            door_at: None,
            light_at: None,
            image_xor: None,
            aux: None,
        },
        // 3/4 Heroes：宽高在 21/25（`Map.cs:363-367`，中间 2 字节是 xor，服务器不读），
        // 单元格从 54 开始（`Map.cs:370`），每格 15 字节（`Map.cs:372-393`）。
        MapVersion::V7 => Plan {
            version,
            width: u32::from(read_u16(data, 21, version)?),
            height: u32::from(read_u16(data, 25, version)?),
            start: 54,
            stride: 15,
            walls: WallRule::TwoWords { low: 6 },
            door_at: Some(8),
            light_at: Some(12),
            image_xor: None,
            aux: Some(AUX_LEGACY_15),
        },
        // Crystal 自定义格式：`Bytes[0] == 1 && Bytes[1] == 0`（`Map.cs:399`），
        // 宽高在 4/6（`Map.cs:400-402`），单元格从 8 开始（`Map.cs:406`），每格 26 字节
        // （`Map.cs:408-428`）。
        MapVersion::V100 => {
            let major = read_u8(data, 0, version)?;
            let minor = read_u8(data, 1, version)?;
            if major != 1 || minor != 0 {
                return Err(MapError::UnsupportedPayloadVersion { major, minor });
            }
            Plan {
                version,
                width: u32::from(read_u16(data, 4, version)?),
                height: u32::from(read_u16(data, 6, version)?),
                start: 8,
                stride: 26,
                walls: WallRule::CrystalV100,
                door_at: Some(14),
                light_at: Some(25),
                image_xor: None,
                aux: None,
            }
        }
    };

    if plan.width == 0 || plan.height == 0 {
        return Err(MapError::InvalidDimensions {
            version,
            width: plan.width,
            height: plan.height,
        });
    }
    validate_size(&plan, data)?;
    Ok(plan)
}

/// 老版 Mir2 的「宽度低字节为 0」兼容：`FindType` 会把它判成 Mir3 v5（`Map.cs:81-82`）。
///
/// 老版 Mir2 把宽度放在偏移 0（`Map.cs:115`），宽度是 256 的倍数时首字节为 0，于是
/// `input[0] == 0` 先命中 v5 分支（`docs/world/map-format.md` §2 注意 2）。这里只在
/// **v5 布局尺寸不成立、v0 布局尺寸成立**时回退为 v0，并打一条警告——Crystal 自身会在
/// v5 解析中途抛异常，因此这个回退只会救回本来加载不了的地图，不会改变任何正常文件的解析结果。
fn rescue_legacy_v0(data: &[u8], version: MapVersion, error: MapError) -> Result<Plan, MapError> {
    if version != MapVersion::V5 {
        return Err(error);
    }
    let Ok(plan) = plan_for(MapVersion::V0, data) else {
        return Err(error);
    };
    warn!(
        width = plan.width,
        height = plan.height,
        "地图首字节为 0，按 Map.cs:81-82 判为 v5，但 v5 布局尺寸不成立、v0 布局成立，按 v0 解析"
    );
    Ok(plan)
}

/// 布局需要的字节数：`start + W * H * stride`。
fn expected_len(plan: &Plan) -> usize {
    plan.start.saturating_add(
        (plan.width as usize)
            .saturating_mul(plan.height as usize)
            .saturating_mul(plan.stride),
    )
}

/// 校验文件长度：短了报错，长了只记录（Crystal 从不校验，`Map.cs:432-514`）。
fn validate_size(plan: &Plan, data: &[u8]) -> Result<(), MapError> {
    let expected = expected_len(plan);
    if data.len() < expected {
        return Err(MapError::BadSize {
            version: plan.version,
            width: plan.width,
            height: plan.height,
            stride: plan.stride,
            expected,
            actual: data.len(),
        });
    }
    if data.len() > expected {
        // `resource/map` 下确实存在这种地图，例如 `EM001.map`（30×35，布局 12652 字节、
        // 实际 20692 字节）、`whitevillage.map`（500×500，多出 500000 字节）。
        debug!(
            layout_bytes = expected,
            file_bytes = data.len(),
            "地图文件比布局长，尾部字节按 Crystal 的行为忽略"
        );
    }
    Ok(())
}

/// 解码一个单元格：`offset` 是它在文件里的起始位置。
///
/// 所有读取都带边界检查（返回 `None`），但调用前已用 [`validate_size`] 保证不会越界。
fn decode_cell(data: &[u8], offset: usize, plan: &Plan) -> Option<MapCell> {
    let end = offset.checked_add(plan.stride)?;
    let bytes = data.get(offset..end)?;

    let mut cell = MapCell {
        attribute: attribute_of(plan.walls, bytes)?,
        ..MapCell::default()
    };

    if let Some(light_at) = plan.light_at {
        cell.light = *bytes.get(light_at)?;
        // `if (light >= 100 && light <= 119) FishingAttribute = light - 100`（`Map.cs:149-150`）
        if (100..=119).contains(&cell.light) {
            cell.fishing_attribute = (cell.light - 100) as i8;
        }
    }

    if let Some(door_at) = plan.door_at {
        cell.door_raw = *bytes.get(door_at)?;
        // `if (fileBytes[offSet] > 0) DoorIndex[x, y] = AddDoor(fileBytes[offSet], ...)`
        // （`Map.cs:142-143`），而 `AddDoor` 内部先做 `& 0x7F`（`Map.cs:52`）。
        if cell.door_raw > 0 {
            cell.door_index = Some(cell.door_raw & DOOR_INDEX_MASK);
        }
    }

    if let Some(xor) = plan.image_xor {
        cell.back_image = u16_at(bytes, 0)? ^ xor;
        cell.middle_image = u16_at(bytes, 2)? ^ xor;
        cell.front_image = u16_at(bytes, 4)? ^ xor;
    }

    if let Some(aux) = plan.aux {
        cell.client_door_index = *bytes.get(aux.door_index)? & DOOR_INDEX_MASK;
        cell.door_offset = *bytes.get(aux.door_offset)?;
        cell.front_animation_tick = *bytes.get(aux.front_animation_tick)?;
        cell.front_index = *bytes.get(aux.front_index)?;
    }

    Some(cell)
}

/// 墙体判定：每个分支与 `Map.cs` 里的判定顺序一致，后写覆盖前写。
fn attribute_of(rule: WallRule, bytes: &[u8]) -> Option<CellAttribute> {
    let mut attribute = CellAttribute::Walk;

    match rule {
        WallRule::OverrideWords => {
            if u16_at(bytes, 0)? & 0x8000 != 0 {
                attribute = CellAttribute::HighWall;
            }
            if u16_at(bytes, 2)? & 0x8000 != 0 {
                attribute = CellAttribute::LowWall;
            }
            if u16_at(bytes, 4)? & 0x8000 != 0 {
                attribute = CellAttribute::HighWall;
            }
        }
        WallRule::Wemade2010 { xor } => {
            // `((BitConverter.ToInt32(fileBytes, offSet) ^ 0xAA38AA38) & 0x20000000) != 0`（Map.cs:173）
            if (u32_at(bytes, 0)? ^ 0xAA38_AA38) & 0x2000_0000 != 0 {
                attribute = CellAttribute::HighWall;
            }
            // `((BitConverter.ToInt16(fileBytes, offSet) ^ xor) & 0x8000) != 0`（Map.cs:177）
            if (u16_at(bytes, 6)? ^ xor) & 0x8000 != 0 {
                attribute = CellAttribute::LowWall;
            }
        }
        WallRule::TwoWords { low } => {
            if u16_at(bytes, 0)? & 0x8000 != 0 {
                attribute = CellAttribute::HighWall;
            }
            if u16_at(bytes, low)? & 0x8000 != 0 {
                attribute = CellAttribute::LowWall;
            }
        }
        WallRule::Mir3Bits => {
            let flag = *bytes.first()?;
            if flag & 0x01 != 1 {
                attribute = CellAttribute::HighWall;
            } else if flag & 0x02 != 2 {
                attribute = CellAttribute::LowWall;
            }
        }
        WallRule::CrystalV100 => {
            if u32_at(bytes, 2)? & 0x2000_0000 != 0 {
                attribute = CellAttribute::HighWall;
            }
            if u16_at(bytes, 12)? & 0x8000 != 0 {
                attribute = CellAttribute::LowWall;
            }
        }
    }

    Some(attribute)
}

/// 按 `AddDoor`（`Map.cs:50-59`）收集门表：同索引复用，位置取第一次出现处。
fn collect_doors(cells: &[MapCell], height: usize) -> Vec<Door> {
    let height = height.max(1);
    let mut doors: Vec<Door> = Vec::new();
    // 索引被 `& 0x7F` 掩过，必然落在 0..=127，所以用定长数组做去重。
    let mut seen = [false; 128];
    for (index, cell) in cells.iter().enumerate() {
        let Some(door_index) = cell.door_index else {
            continue;
        };
        let slot = usize::from(door_index);
        if seen[slot] {
            continue;
        }
        seen[slot] = true;
        // 列优先：x = i / Height，y = i % Height
        doors.push(Door {
            index: door_index,
            location: Point::new((index / height) as i32, (index % height) as i32),
        });
    }
    doors
}

/// 读取小端 `u16`；越界返回 `None`（Crystal 里对应 `BitConverter.ToInt16` 抛异常）。
fn u16_at(bytes: &[u8], at: usize) -> Option<u16> {
    let slice: [u8; 2] = bytes.get(at..at.checked_add(2)?)?.try_into().ok()?;
    Some(u16::from_le_bytes(slice))
}

/// 读取小端 `u32`；越界返回 `None`。
fn u32_at(bytes: &[u8], at: usize) -> Option<u32> {
    let slice: [u8; 4] = bytes.get(at..at.checked_add(4)?)?.try_into().ok()?;
    Some(u32::from_le_bytes(slice))
}

/// 读取 1 字节；越界返回 `None`。
fn u8_at(bytes: &[u8], at: usize) -> Option<u8> {
    bytes.get(at).copied()
}

/// 读取头部 `u16`，失败时给出带版本信息的错误。
fn read_u16(data: &[u8], at: usize, version: MapVersion) -> Result<u16, MapError> {
    u16_at(data, at).ok_or(MapError::Truncated {
        version,
        offset: at,
        needed: 2,
        actual: data.len(),
    })
}

/// 读取头部 1 字节，失败时给出带版本信息的错误。
fn read_u8(data: &[u8], at: usize, version: MapVersion) -> Result<u8, MapError> {
    u8_at(data, at).ok_or(MapError::Truncated {
        version,
        offset: at,
        needed: 1,
        actual: data.len(),
    })
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;

    /// 仓库里的真实地图目录（`crates/mir-world` → 仓库根 → `resource/map`）。
    const REAL_MAP_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../resource/map");

    const V0_HEADER: usize = 52;
    const V0_STRIDE: usize = 12;

    fn real_map_path(name: &str) -> PathBuf {
        Path::new(REAL_MAP_DIR).join(name)
    }

    fn set_u8(data: &mut [u8], at: usize, value: u8) {
        data[at] = value;
    }

    fn set_u16(data: &mut [u8], at: usize, value: u16) {
        data[at..at + 2].copy_from_slice(&value.to_le_bytes());
    }

    fn set_u32(data: &mut [u8], at: usize, value: u32) {
        data[at..at + 4].copy_from_slice(&value.to_le_bytes());
    }

    /// 列优先：v0 第 `(x, y)` 格在文件里的偏移（`i = x * Height + y`）。
    fn v0_offset(x: usize, y: usize, height: usize) -> usize {
        V0_HEADER + (x * height + y) * V0_STRIDE
    }

    fn parse_ok(data: &[u8]) -> Map {
        match Map::from_bytes(data) {
            Ok(map) => map,
            Err(error) => panic!("应当解析成功，实际失败：{error}"),
        }
    }

    fn cell_at(map: &Map, x: i32, y: i32) -> MapCell {
        match map.cell(x, y) {
            Some(cell) => *cell,
            None => panic!("({x}, {y}) 越界"),
        }
    }

    #[derive(Debug, Default)]
    struct AttributeCounts {
        walk: usize,
        high: usize,
        low: usize,
        door_cells: usize,
    }

    impl AttributeCounts {
        fn of(map: &Map) -> Self {
            let mut counts = Self::default();
            for cell in &map.cells {
                match cell.attribute {
                    CellAttribute::Walk => counts.walk += 1,
                    CellAttribute::HighWall => counts.high += 1,
                    CellAttribute::LowWall => counts.low += 1,
                }
                if cell.door_index.is_some() {
                    counts.door_cells += 1;
                }
            }
            counts
        }
    }

    #[test]
    fn v0_synthetic_round_trip() {
        const WIDTH: usize = 3;
        const HEIGHT: usize = 2;

        let mut data = vec![0_u8; V0_HEADER + V0_STRIDE * WIDTH * HEIGHT];
        set_u16(&mut data, 0, WIDTH as u16); // Width = Int16@0（Map.cs:115）
        set_u16(&mut data, 2, HEIGHT as u16); // Height = Int16@2（Map.cs:117）

        // 先给每格写入可区分、且不置 bit15 的图像/保留字节。
        for x in 0..WIDTH {
            for y in 0..HEIGHT {
                let base = v0_offset(x, y, HEIGHT);
                let seed = (x * 10 + y) as u16;
                set_u16(&mut data, base, 0x0A00 + seed); // o+0 后层图像
                set_u16(&mut data, base + 2, 0x0B00 + seed); // o+2 中层图像
                set_u16(&mut data, base + 4, 0x0C00 + seed); // o+4 前层图像
                set_u8(&mut data, base + 6, 0x20 + seed as u8); // o+6 客户端门索引
                set_u8(&mut data, base + 7, 0x30 + seed as u8); // o+7 客户端门偏移
                set_u8(&mut data, base + 9, 0x40 + seed as u8); // o+9 前层动画计时
                set_u8(&mut data, base + 10, 0x50 + seed as u8); // o+10 前层瓦片序号
            }
        }

        // (0, 0)：可走 + 门字节 0x8A（索引 10，Map.cs:142-143 与 Map.cs:52）
        set_u8(&mut data, v0_offset(0, 0, HEIGHT) + 8, 0x8A);
        // (0, 1)：o+4 bit15 → HighWall（Map.cs:135-136），门索引同样复用 10
        set_u16(&mut data, v0_offset(0, 1, HEIGHT) + 4, 0x8C01);
        set_u8(&mut data, v0_offset(0, 1, HEIGHT) + 8, 0x8A);
        // (1, 0)：o+0 bit15 → HighWall（Map.cs:126-127），门 0x80 → 索引 0，光照 105 → 钓鱼 5
        set_u16(&mut data, v0_offset(1, 0, HEIGHT), 0x8A0A);
        set_u8(&mut data, v0_offset(1, 0, HEIGHT) + 8, 0x80);
        set_u8(&mut data, v0_offset(1, 0, HEIGHT) + 11, 105);
        // (1, 1)：o+2 bit15 → LowWall（Map.cs:130-131），光照 119 → 钓鱼 19
        set_u16(&mut data, v0_offset(1, 1, HEIGHT) + 2, 0x8B0B);
        set_u8(&mut data, v0_offset(1, 1, HEIGHT) + 11, 119);
        // (2, 0)：o+2 与 o+4 都置 bit15 → 后写的 o+4 胜出，仍是 HighWall
        set_u16(&mut data, v0_offset(2, 0, HEIGHT) + 2, 0x8B0C);
        set_u16(&mut data, v0_offset(2, 0, HEIGHT) + 4, 0x8C0C);
        // (2, 1)：光照 120 超出 100..=119，不产生钓鱼属性
        set_u8(&mut data, v0_offset(2, 1, HEIGHT) + 11, 120);

        let map = parse_ok(&data);

        assert_eq!(map.width, WIDTH as i32);
        assert_eq!(map.height, HEIGHT as i32);
        assert_eq!(map.version, MapVersion::V0);
        assert_eq!(map.file_name, "<memory>");
        assert_eq!(map.cells.len(), WIDTH * HEIGHT);

        // (0, 0)：可走，门 10，无光照
        let cell = cell_at(&map, 0, 0);
        assert_eq!(cell.attribute, CellAttribute::Walk);
        assert!(cell.is_walkable());
        assert_eq!(cell.back_image, 0x0A00);
        assert_eq!(cell.middle_image, 0x0B00);
        assert_eq!(cell.front_image, 0x0C00);
        assert_eq!(cell.client_door_index, 0x20);
        assert_eq!(cell.door_offset, 0x30);
        assert_eq!(cell.front_animation_tick, 0x40);
        assert_eq!(cell.front_index, 0x50);
        assert_eq!(cell.door_raw, 0x8A);
        assert_eq!(cell.door_index, Some(10));
        assert_eq!(cell.light, 0);
        assert_eq!(cell.fishing_attribute, MapCell::FISHING_ATTRIBUTE_NONE);

        // (0, 1)：第三个字 bit15 → HighWall，且门索引 10 被复用
        let cell = cell_at(&map, 0, 1);
        assert_eq!(cell.attribute, CellAttribute::HighWall);
        assert!(cell.is_high_wall());
        assert_eq!(cell.front_image, 0x8C01);
        assert_eq!(cell.door_index, Some(10));

        // (1, 0)：第一个字 bit15 → HighWall；0x80 → 索引 0；光照 105 → 钓鱼 5
        let cell = cell_at(&map, 1, 0);
        assert_eq!(cell.attribute, CellAttribute::HighWall);
        assert_eq!(cell.back_image, 0x8A0A);
        assert_eq!(cell.middle_image, 0x0B0A);
        assert_eq!(cell.front_image, 0x0C0A);
        assert_eq!(cell.door_raw, 0x80);
        assert_eq!(cell.door_index, Some(0));
        assert_eq!(cell.light, 105);
        assert_eq!(cell.fishing_attribute, 5);
        assert_eq!(cell.door_offset, 0x30 + 10);

        // (1, 1)：第二个字 bit15 → LowWall；光照 119 → 钓鱼 19；无门
        let cell = cell_at(&map, 1, 1);
        assert_eq!(cell.attribute, CellAttribute::LowWall);
        assert!(cell.is_low_wall());
        assert_eq!(cell.middle_image, 0x8B0B);
        assert_eq!(cell.door_index, None);
        assert_eq!(cell.light, 119);
        assert_eq!(cell.fishing_attribute, 19);

        // (2, 0)：o+4 覆盖 o+2 → HighWall；两个图像号都保留
        let cell = cell_at(&map, 2, 0);
        assert_eq!(cell.attribute, CellAttribute::HighWall);
        assert_eq!(cell.middle_image, 0x8B0C);
        assert_eq!(cell.front_image, 0x8C0C);
        assert_eq!(cell.door_index, None);

        // (2, 1)：光照 120 不在 100..=119，钓鱼属性保持 -1
        let cell = cell_at(&map, 2, 1);
        assert_eq!(cell.attribute, CellAttribute::Walk);
        assert_eq!(cell.light, 120);
        assert_eq!(cell.fishing_attribute, MapCell::FISHING_ATTRIBUTE_NONE);

        // 列优先的索引公式：cells[x * Height + y] 就是 (x, y)
        assert_eq!(map.cells[HEIGHT + 1], cell_at(&map, 1, 1)); // 1 * Height + 1
        assert_eq!(map.cells[2 * HEIGHT], cell_at(&map, 2, 0)); // 2 * Height + 0

        // 越界
        assert!(map.cell(-1, 0).is_none());
        assert!(map.cell(WIDTH as i32, 0).is_none());
        assert!(map.cell(0, HEIGHT as i32).is_none());

        // walkable() / walkable_cells()
        assert!(map.walkable(0, 0));
        assert!(!map.walkable(1, 0));
        assert!(!map.walkable(1, 1));
        assert!(map.walkable(2, 1));
        assert!(!map.walkable(-1, 0));
        assert_eq!(map.walkable_cells().count(), 2);

        // 门表：按列优先加载顺序，同索引复用（索引 10 先出现在 (0,0)，索引 0 在 (1,0)）
        assert_eq!(map.doors.len(), 2);
        assert_eq!(
            map.doors[0],
            Door {
                index: 10,
                location: Point::new(0, 0)
            }
        );
        assert_eq!(
            map.doors[1],
            Door {
                index: 0,
                location: Point::new(1, 0)
            }
        );
    }

    #[test]
    fn v2_synthetic_door_and_fishing() {
        const WIDTH: usize = 2;
        const HEIGHT: usize = 2;
        const STRIDE: usize = 14;

        let mut data = vec![0_u8; 52 + STRIDE * WIDTH * HEIGHT];
        set_u16(&mut data, 0, WIDTH as u16); // 宽高同 v0（Map.cs:198-200）
        set_u16(&mut data, 2, HEIGHT as u16);
        // `FindType` 靠 `data[4] == 0x0F` 命中 v2/v3 分支（Map.cs:96）；
        // 长度恰好等于 `52 + W * H * 14` 时返回 v2（Map.cs:100-103）。
        set_u8(&mut data, 4, 0x0F);

        set_u16(&mut data, 52 + 4, 0x8000); // o+4 bit15 → HighWall（Map.cs:217-218）
        set_u8(&mut data, 52 + 6, 0x83); // o+6 门 → 索引 3（Map.cs:223-224）
        set_u8(&mut data, 52 + 11, 101); // o+11 光照 → 钓鱼 1（Map.cs:227-230）

        let map = parse_ok(&data);
        assert_eq!(map.version, MapVersion::V2);
        assert_eq!((map.width, map.height), (WIDTH as i32, HEIGHT as i32));

        let cell = cell_at(&map, 0, 0);
        assert_eq!(cell.attribute, CellAttribute::HighWall);
        assert_eq!(cell.front_image, 0x8000);
        assert_eq!(cell.door_raw, 0x83);
        assert_eq!(cell.door_index, Some(3));
        assert_eq!(cell.client_door_index, 3); // v2 的服务器与客户端都读 o+6
        assert_eq!(cell.light, 101);
        assert_eq!(cell.fishing_attribute, 1);

        assert_eq!(cell_at(&map, 1, 1).attribute, CellAttribute::Walk);
        assert_eq!(map.doors.len(), 1);
        assert_eq!(map.doors[0].index, 3);
    }

    #[test]
    fn v3_synthetic_layout() {
        const STRIDE: usize = 36;

        let mut data = vec![0_u8; 52 + STRIDE * 2 * 2];
        set_u16(&mut data, 0, 2);
        set_u16(&mut data, 2, 2);
        // 同样命中 v2/v3 分支；长度 196 > 52 + 2*2*14 = 108 → v3（Map.cs:100-103）
        set_u8(&mut data, 4, 0x0F);

        // 第 0 格：o+0 bit15 → HighWall（Map.cs:250-251），门 o+6 = 0x85 → 索引 5（Map.cs:263-264），
        // 光照 o+18 = 104 → 钓鱼 4（Map.cs:267-270）
        set_u16(&mut data, 52, 0x8000);
        set_u8(&mut data, 52 + 6, 0x85);
        set_u8(&mut data, 52 + 18, 104);
        // 第 1 格：只置 o+2 bit15 → LowWall（Map.cs:254-255）
        set_u16(&mut data, 52 + STRIDE + 2, 0x8000);
        // 第 2 格：只置 o+4 bit15 → HighWall（Map.cs:258-259）
        set_u16(&mut data, 52 + 2 * STRIDE + 4, 0x8000);
        // 第 3 格：全 0 → Walk

        let map = parse_ok(&data);
        assert_eq!(map.version, MapVersion::V3);

        let cell = cell_at(&map, 0, 0);
        assert_eq!(cell.attribute, CellAttribute::HighWall);
        assert_eq!(cell.back_image, 0x8000);
        assert_eq!(cell.door_index, Some(5));
        assert_eq!(cell.light, 104);
        assert_eq!(cell.fishing_attribute, 4);
        // 文件第 1 格是 (0, 1)、第 2 格是 (1, 0)（列优先 i = x * Height + y）
        assert_eq!(cell_at(&map, 0, 1).attribute, CellAttribute::LowWall);
        assert_eq!(cell_at(&map, 1, 0).attribute, CellAttribute::HighWall);
        assert_eq!(cell_at(&map, 1, 1).attribute, CellAttribute::Walk);
    }

    #[test]
    fn v1_synthetic_layout() {
        const HEADER: usize = 54;
        const STRIDE: usize = 15;
        let xor: u16 = 5;

        let mut data = vec![0_u8; HEADER + STRIDE * 2 * 2];
        // 识别标记 `Map 2010 Ver 1.0`（Map.cs:92-93）
        set_u8(&mut data, 0, 0x10);
        set_u8(&mut data, 2, 0x61);
        set_u8(&mut data, 7, 0x31);
        set_u8(&mut data, 14, 0x31);
        // `Width = w ^ xor`、`Height = h ^ xor`，w/xor/h 在 21/23/25（Map.cs:158-164）
        set_u16(&mut data, 21, 7); // 7 ^ 5 = 2
        set_u16(&mut data, 23, xor);
        set_u16(&mut data, 25, 7);

        // 第 0 格：`u32@0 ^ 0xAA38AA38 = 0x20000000`，bit29 置位 → HighWall（Map.cs:173-174）
        set_u32(&mut data, HEADER, 0x8A38_AA38);
        // 门 o+8 = 0x82 → 索引 2（Map.cs:182-183）；光照 o+13 = 100 → 钓鱼 0（Map.cs:186-189）
        set_u8(&mut data, HEADER + 8, 0x82);
        set_u8(&mut data, HEADER + 13, 100);

        // 第 1 格：`u32@0 = 0x20000000` → 异或后 bit29 为 0，不设高墙；
        // `u16@6 ^ xor` 的 bit15 → LowWall（Map.cs:177-178）
        set_u32(&mut data, HEADER + STRIDE, 0x2000_0000);
        set_u16(&mut data, HEADER + STRIDE + 6, 0x8000 ^ xor);

        // 第 2、3 格：异或后两个 bit 都为 0 → Walk
        set_u32(&mut data, HEADER + 2 * STRIDE, 0x2000_0000);
        set_u32(&mut data, HEADER + 3 * STRIDE, 0x2000_0000);

        let map = parse_ok(&data);
        assert_eq!(map.version, MapVersion::V1);
        assert_eq!((map.width, map.height), (2, 2));

        let cell = cell_at(&map, 0, 0);
        assert_eq!(cell.attribute, CellAttribute::HighWall);
        assert_eq!(cell.door_index, Some(2));
        assert_eq!(cell.light, 100);
        assert_eq!(cell.fishing_attribute, 0);

        // 文件第 1 格是 (0, 1)、第 2 格是 (1, 0)（列优先 i = x * Height + y）
        assert_eq!(cell_at(&map, 0, 1).attribute, CellAttribute::LowWall);
        assert_eq!(cell_at(&map, 1, 0).attribute, CellAttribute::Walk);
        assert_eq!(cell_at(&map, 1, 1).attribute, CellAttribute::Walk);
    }

    #[test]
    fn v4_synthetic_layout() {
        const HEADER: usize = 64;
        const STRIDE: usize = 12;
        let xor: u16 = 3;

        let mut data = vec![0_u8; HEADER + STRIDE * 2 * 2];
        // 识别标记 `Mir2 AntiHack`（Map.cs:88-89）
        set_u8(&mut data, 0, 0x15);
        set_u8(&mut data, 4, 0x32);
        set_u8(&mut data, 6, 0x41);
        set_u8(&mut data, 19, 0x31);
        // 宽高 = `w ^ xor`，w/xor/h 在 31/33/35（Map.cs:278-285）
        set_u16(&mut data, 31, 1);
        set_u16(&mut data, 33, xor);
        set_u16(&mut data, 35, 1);

        // 第 0 格：`o+0 ^ xor = 0x8000` → HighWall（Map.cs:294-295）；
        // 门 o+6 = 0x86 → 索引 6（Map.cs:303-304）
        set_u16(&mut data, HEADER, 0x8000 ^ xor);
        set_u8(&mut data, HEADER + 6, 0x86);
        // 第 1 格：`o+2 ^ xor = 0x8000` → LowWall（Map.cs:298-299）
        set_u16(&mut data, HEADER + STRIDE + 2, 0x8000 ^ xor);

        let map = parse_ok(&data);
        assert_eq!(map.version, MapVersion::V4);

        let cell = cell_at(&map, 0, 0);
        assert_eq!(cell.attribute, CellAttribute::HighWall);
        assert_eq!(cell.back_image, 0x8000); // 异或还原
        assert_eq!(cell.door_index, Some(6));
        assert_eq!(cell.light, 0); // v4 不读光照（Map.cs:276-307）
        assert_eq!(cell.fishing_attribute, MapCell::FISHING_ATTRIBUTE_NONE);

        // 文件第 1 格是 (0, 1)（列优先 i = x * Height + y）
        assert_eq!(cell_at(&map, 0, 1).attribute, CellAttribute::LowWall);
        assert_eq!(cell_at(&map, 1, 0).attribute, CellAttribute::Walk);
    }

    #[test]
    fn v5_synthetic_wall_bits() {
        const STRIDE: usize = 14;

        // 瓦片索引表：`28 + 3 * ceil(W/2) * (H/2)`（Map.cs:318），2×2 时为 3 字节
        let start = 28 + 3;
        let mut data = vec![0_u8; start + STRIDE * 2 * 2];
        // 首字节为 0 → Wemade Mir3（Map.cs:81-82）。宽高在 22/24（Map.cs:311-314）
        set_u16(&mut data, 22, 2);
        set_u16(&mut data, 24, 2);

        // flag=0x03 → Walk；0x01 → LowWall；0x02 → HighWall；0x00 → HighWall（Map.cs:322-327）
        set_u8(&mut data, start, 0x03);
        set_u8(&mut data, start + STRIDE, 0x01);
        set_u8(&mut data, start + 2 * STRIDE, 0x02);
        // 光照在 o+13（Map.cs:330），110 → 钓鱼 10
        set_u8(&mut data, start + 13, 110);

        let map = parse_ok(&data);
        assert_eq!(map.version, MapVersion::V5);
        assert_eq!((map.width, map.height), (2, 2));

        let cell = cell_at(&map, 0, 0);
        assert_eq!(cell.attribute, CellAttribute::Walk);
        assert_eq!(cell.light, 110);
        assert_eq!(cell.fishing_attribute, 10);
        assert_eq!(cell.door_index, None); // Mir3 没有门

        // 文件第 1 格是 (0, 1)、第 2 格是 (1, 0)（列优先 i = x * Height + y）
        assert_eq!(cell_at(&map, 0, 1).attribute, CellAttribute::LowWall);
        assert_eq!(cell_at(&map, 1, 0).attribute, CellAttribute::HighWall);
        assert_eq!(cell_at(&map, 1, 1).attribute, CellAttribute::HighWall);
    }

    #[test]
    fn v6_synthetic_wall_bits() {
        const HEADER: usize = 40;
        const STRIDE: usize = 20;

        let mut data = vec![0_u8; HEADER + STRIDE * 2 * 2];
        // 识别标记 `(C) SNDA, MIR3.`（Map.cs:84-85）；宽高在 16/18（Map.cs:339-341）
        set_u8(&mut data, 0, 0x0F);
        set_u8(&mut data, 5, 0x53);
        set_u8(&mut data, 14, 0x33);
        set_u16(&mut data, 16, 2);
        set_u16(&mut data, 18, 2);

        // flag=0x03 → Walk；0x01 → LowWall；0x02 → HighWall；0x00 → HighWall（Map.cs:351-356）
        set_u8(&mut data, HEADER, 0x03);
        set_u8(&mut data, HEADER + STRIDE, 0x01);
        set_u8(&mut data, HEADER + 2 * STRIDE, 0x02);

        let map = parse_ok(&data);
        assert_eq!(map.version, MapVersion::V6);
        // 文件第 1 格是 (0, 1)、第 2 格是 (1, 0)（列优先 i = x * Height + y）
        assert_eq!(cell_at(&map, 0, 0).attribute, CellAttribute::Walk);
        assert_eq!(cell_at(&map, 0, 1).attribute, CellAttribute::LowWall);
        assert_eq!(cell_at(&map, 1, 0).attribute, CellAttribute::HighWall);
        assert_eq!(cell_at(&map, 1, 1).attribute, CellAttribute::HighWall);

        let cell = cell_at(&map, 0, 0);
        assert_eq!(cell.light, 0); // v6 只读 1 字节墙体位图（Map.cs:337-359）
        assert_eq!(cell.door_index, None);
    }

    #[test]
    fn v7_synthetic_layout() {
        const HEADER: usize = 54;
        const STRIDE: usize = 15;

        let mut data = vec![0_u8; HEADER + STRIDE * 2 * 2];
        // 识别标记（Map.cs:107-108）；宽高在 21/25（Map.cs:363-367）
        set_u8(&mut data, 0, 0x0D);
        set_u8(&mut data, 1, 0x4C);
        set_u8(&mut data, 7, 0x20);
        set_u8(&mut data, 11, 0x6D);
        set_u16(&mut data, 21, 2);
        set_u16(&mut data, 25, 2);

        // 第 0 格：o+0 bit15 → HighWall（Map.cs:375-376）；门 o+8 = 0x81 → 索引 1（Map.cs:383-384）；
        // 光照 o+12 = 119 → 钓鱼 19（Map.cs:387-390）
        set_u16(&mut data, HEADER, 0x8000);
        set_u8(&mut data, HEADER + 8, 0x81);
        set_u8(&mut data, HEADER + 12, 119);
        // 第 1 格：低墙字在 o+6（Map.cs:378-379）
        set_u16(&mut data, HEADER + STRIDE + 6, 0x8000);

        let map = parse_ok(&data);
        assert_eq!(map.version, MapVersion::V7);

        let cell = cell_at(&map, 0, 0);
        assert_eq!(cell.attribute, CellAttribute::HighWall);
        assert_eq!(cell.door_index, Some(1));
        assert_eq!(cell.light, 119);
        assert_eq!(cell.fishing_attribute, 19);
        // 文件第 1 格是 (0, 1)（列优先 i = x * Height + y）
        assert_eq!(cell_at(&map, 0, 1).attribute, CellAttribute::LowWall);
        assert_eq!(cell_at(&map, 1, 0).attribute, CellAttribute::Walk);
    }

    #[test]
    fn v100_synthetic_layout() {
        const HEADER: usize = 8;
        const STRIDE: usize = 26;

        let mut data = vec![0_u8; HEADER + STRIDE * 2 * 2];
        // 识别：`Bytes[2]==0x43 && Bytes[3]==0x23`（Map.cs:76-79），payload 版本 1.0（Map.cs:399）；
        // 宽高在 4/6（Map.cs:400-402）
        set_u8(&mut data, 0, 1);
        set_u8(&mut data, 1, 0);
        set_u8(&mut data, 2, 0x43);
        set_u8(&mut data, 3, 0x23);
        set_u16(&mut data, 4, 2);
        set_u16(&mut data, 6, 2);

        // 第 0 格：`u32@2` 的 bit29 → HighWall（Map.cs:412-413）；门 o+14 = 0x87 → 索引 7
        // （Map.cs:420-421）；光照 o+25 = 118 → 钓鱼 18（Map.cs:424-427）
        set_u32(&mut data, HEADER + 2, 0x2000_0000);
        set_u8(&mut data, HEADER + 14, 0x87);
        set_u8(&mut data, HEADER + 25, 118);
        // 第 1 格：`u16@12` 的 bit15 → LowWall（Map.cs:415-416）
        set_u16(&mut data, HEADER + STRIDE + 12, 0x8000);

        let map = parse_ok(&data);
        assert_eq!(map.version, MapVersion::V100);

        let cell = cell_at(&map, 0, 0);
        assert_eq!(cell.attribute, CellAttribute::HighWall);
        assert_eq!(cell.door_index, Some(7));
        assert_eq!(cell.light, 118);
        assert_eq!(cell.fishing_attribute, 18);
        // 文件第 1 格是 (0, 1)（列优先 i = x * Height + y）
        assert_eq!(cell_at(&map, 0, 1).attribute, CellAttribute::LowWall);
        assert_eq!(cell_at(&map, 1, 0).attribute, CellAttribute::Walk);
    }

    #[test]
    fn rejects_short_truncated_and_invalid_files() {
        // 少于 20 字节：FindType 会读到 input[19]（Map.cs:88-96）
        assert!(matches!(
            Map::from_bytes(&[0_u8; 19]),
            Err(MapError::TooShort { .. })
        ));

        // 截断的 v0：声明 3×2 却只给了一格的数据（Crystal 会在循环里抛异常）
        let mut truncated = vec![0_u8; V0_HEADER + V0_STRIDE];
        set_u16(&mut truncated, 0, 3);
        set_u16(&mut truncated, 2, 2);
        assert!(matches!(
            Map::from_bytes(&truncated),
            Err(MapError::BadSize { .. })
        ));

        // 高度为 0
        let mut invalid = vec![0_u8; V0_HEADER];
        set_u16(&mut invalid, 0, 3);
        set_u16(&mut invalid, 2, 0);
        assert!(matches!(
            Map::from_bytes(&invalid),
            Err(MapError::InvalidDimensions { .. })
        ));

        // v100 payload 版本不是 1.0（Map.cs:399）
        let mut payload = vec![0_u8; 8 + 26];
        set_u8(&mut payload, 0, 2);
        set_u8(&mut payload, 2, 0x43);
        set_u8(&mut payload, 3, 0x23);
        set_u16(&mut payload, 4, 1);
        set_u16(&mut payload, 6, 1);
        assert!(matches!(
            Map::from_bytes(&payload),
            Err(MapError::UnsupportedPayloadVersion { major: 2, .. })
        ));
    }

    #[test]
    fn tolerates_trailing_bytes_like_crystal() {
        // Crystal 从不校验文件长度（Map.cs:432-514），`resource/map` 里也确实有比布局更长的地图
        // （EM001.map 30×35：布局 12652 字节、实际 20692 字节）。
        let mut data = vec![0_u8; V0_HEADER + V0_STRIDE * 2 * 2 + 100];
        set_u16(&mut data, 0, 2);
        set_u16(&mut data, 2, 2);
        set_u16(&mut data, V0_HEADER, 0x8000); // (0, 0) 高墙

        let map = parse_ok(&data);
        assert_eq!((map.width, map.height), (2, 2));
        assert_eq!(cell_at(&map, 0, 0).attribute, CellAttribute::HighWall);
        assert_eq!(cell_at(&map, 1, 1).attribute, CellAttribute::Walk);
    }

    #[test]
    fn rescues_legacy_v0_with_zero_width_low_byte() {
        // 宽度 256 → 首字节为 0，`FindType` 会判成 v5（Map.cs:81-82）；
        // v5 布局尺寸不成立、v0 布局成立，因此回退为 v0（map-format.md §2 注意 2）。
        let mut data = vec![0_u8; V0_HEADER + V0_STRIDE * 256 * 2];
        set_u16(&mut data, 0, 256);
        set_u16(&mut data, 2, 2);

        let map = parse_ok(&data);
        assert_eq!(map.version, MapVersion::V0);
        assert_eq!((map.width, map.height), (256, 2));
        assert_eq!(map.cells.len(), 256 * 2);
    }

    #[test]
    fn loads_bichon_700x700() {
        let path = real_map_path("0.map");
        if !path.exists() {
            println!("跳过 loads_bichon_700x700：缺少 {}", path.display());
            return;
        }
        let map = match Map::load(&path) {
            Ok(map) => map,
            Err(error) => panic!("加载 {} 失败：{error}", path.display()),
        };

        assert_eq!(map.width, 700, "0.map 宽（docs/world/map-format.md §6.2）");
        assert_eq!(map.height, 700, "0.map 高");
        assert_eq!(map.version, MapVersion::V0);
        assert_eq!(map.file_name, "0");

        let counts = AttributeCounts::of(&map);
        println!(
            "0.map 700×700：可走 {}，高墙 {}，低墙 {}，门格 {}，门 {}",
            counts.walk,
            counts.high,
            counts.low,
            counts.door_cells,
            map.doors.len()
        );
        assert_eq!(counts.walk, 327_180, "可走格数");
        assert_eq!(counts.high, 162_820, "高墙格数");
        assert_eq!(counts.low, 0, "低墙格数");
        assert_eq!(counts.door_cells, 42, "门格数（0x8A 40 格 + 0x80 2 格）");
        assert_eq!(map.doors.len(), 2, "门表大小（索引 10、索引 0）");
        assert_eq!(map.walkable_cells().count(), 327_180);

        // 文档 §6.2 的抽样格：只有列优先 i = x * 700 + y 才取到这组字节
        let cell = cell_at(&map, 265, 261);
        assert_eq!(cell.back_image, 0x0357);
        assert_eq!(cell.middle_image, 0);
        assert_eq!(cell.front_image, 0x0AA4);
        assert_eq!(cell.door_raw, 0x8A);
        assert_eq!(cell.door_index, Some(10));
        assert_eq!(cell.light, 5);
        assert_eq!(cell.fishing_attribute, MapCell::FISHING_ATTRIBUTE_NONE);
        assert!(cell.is_walkable());

        // 左上角与右下角都在墙里（文档 §6.2 抽样）
        assert_eq!(cell_at(&map, 0, 0).attribute, CellAttribute::HighWall);
        assert_eq!(cell_at(&map, 699, 699).attribute, CellAttribute::HighWall);
    }

    #[test]
    fn loads_0100_15x18() {
        let path = real_map_path("0100.map");
        if !path.exists() {
            println!("跳过 loads_0100_15x18：缺少 {}", path.display());
            return;
        }
        let map = match Map::load(&path) {
            Ok(map) => map,
            Err(error) => panic!("加载 {} 失败：{error}", path.display()),
        };

        assert_eq!(
            map.width, 15,
            "0100.map 宽（docs/world/map-format.md §6.1）"
        );
        assert_eq!(map.height, 18, "0100.map 高");
        assert_eq!(map.version, MapVersion::V0);
        assert_eq!(map.file_name, "0100");

        let counts = AttributeCounts::of(&map);
        println!(
            "0100.map 15×18：可走 {}，高墙 {}，低墙 {}，门格 {}",
            counts.walk, counts.high, counts.low, counts.door_cells
        );
        assert_eq!(counts.walk, 47, "可走格数");
        assert_eq!(counts.high, 223, "高墙格数");
        assert_eq!(counts.low, 0, "低墙格数");
        assert_eq!(counts.door_cells, 0, "该图没有门");

        // 文档 §6.1 的抽样格 (7, 6)：可走，前层图号 0x0E3C
        let cell = cell_at(&map, 7, 6);
        assert!(cell.is_walkable());
        assert_eq!(cell.back_image, 0);
        assert_eq!(cell.front_image, 0x0E3C);

        // (0, 0) 是墙体，两层都是 0x8000
        let corner = cell_at(&map, 0, 0);
        assert_eq!(corner.attribute, CellAttribute::HighWall);
        assert_eq!(corner.back_image, 0x8000);
        assert_eq!(corner.middle_image, 0);
        assert_eq!(corner.front_image, 0x8000);
    }
}
