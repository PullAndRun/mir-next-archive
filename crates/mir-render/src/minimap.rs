//! 小地图：从 `resource/data/mmap.wzl` 解出地图底图，并按地图格数投影玩家/怪物位置。
//!
//! Crystal 的小地图由两个控件共用同一个图库：
//!
//! * 图库名：`MLibrary.cs:23` — `MiniMap = new MLibrary(Settings.DataPath + "MMap")`，
//!   也就是客户端数据目录下的 `MMap.wzl`（本项目是 `resource/data/mmap.wzl`）；
//! * 小地图面板：`MainDialogs.cs:1764-1880` 的 `MiniMapDialog`，
//!   图像号用 `MapControl.MiniMap`（`MainDialogs.cs:1916`），
//!   视窗固定 120×108（`MainDialogs.cs:1912`）、画在面板内偏移 `(3, 22)`（`:1913-1914`）；
//! * 大地图：`BigMapDialog.cs:642-676`，图像号用 `MapInfo.BigMap`（`:642`）。
//!
//! 两个图像号都来自服务端：`S.MapInformation` 的 `MiniMap` / `BigMap`
//! （`Shared/ServerPackets.cs:479`，值来自 `MapInfo.MiniMap/BigMap`，`MirDatabase/MapInfo.cs:20`），
//! 而 `S.NewMapInfo` 里只有 `BigMap`（`Shared/Data/ClientData.cs:324`，线格式见
//! `crates/mir-protocol/src/data/client_map_info.rs`）。
//!
//! # 投影公式（照抄 Crystal）
//!
//! `MainDialogs.cs:1916-1931`：
//!
//! ```text
//! scaleX = 图像宽 / 地图宽(格)          scaleY = 图像高 / 地图高(格)
//! view.x = round(scaleX × 玩家.x) - 120 / 2     再夹到 [0, 图像宽 - 120]
//! view.y = round(scaleY × 玩家.y) - 108 / 2     再夹到 [0, 图像高 - 108]
//! 点像素 = 视窗原点 + (格坐标 × scale - 视窗原点(图像像素))
//! ```
//!
//! 图像是 **1:1 裁剪**绘制的（`MainDialogs.cs:1933` 的 `Draw(index, viewRect, drawLocation, …)`
//! 传的 `viewRect` 是图像内的裁剪区，不是缩放区），缩放只体现在「格 → 像素」的换算上。
//!
//! # 点位颜色
//!
//! `MainDialogs.cs:1945-1961`：队友蓝（`0,0,255`）、其他玩家白、NPC / `AI == 6` 绿
//! （`0,255,50`）、其余（怪物等）红（`255,0,0`），点大小 2×2（`:1961` 的
//! `RadarTexture` 2×2）。**自己**在 Crystal 里是白色的（走 `PlayerObject` 分支），
//! 本项目按需求把「自己」画成黄色（[`PLAYER_COLOR`]），并在文档里注明这处偏差。
//!
//! 与 Crystal 一样跳过：地面物品、尸体、法术对象、潜行中的对象（`MainDialogs.cs:1941`）。
//!
//! # 大地图（`BigMapDialog`）
//!
//! 同一个 `mmap.wzl` 图库的另一个用途：`MapInfo.BigMap`（`BigMapDialog.cs:642`）是
//! 整张大地图的帧号，面板画法见 [`crate::MinimapScene::draw_big_map`]：
//!
//! | 常量 | 值 | Crystal 出处 |
//! |---|---|---|
//! | [`BIG_MAP_VIEW_WIDTH`] | 568 | `BigMapDialog.cs:559` 的 `Size = new Size(568, 380)` |
//! | [`BIG_MAP_VIEW_HEIGHT`] | 380 | 同上 |
//! | [`BIG_MAP_OFFSET_X`] | 14 | `BigMapDialog.cs:652` 的 `14 + (568 - 宽) / 2` |
//! | [`BIG_MAP_OFFSET_Y`] | 52 | `BigMapDialog.cs:653` 的 `52 + (380 - 高) / 2` |
//!
//! 缩放：`绘制宽 = min(568, 图像宽)`、`绘制高 = min(380, 图像高)`（`:650`），
//! 整图像素 = `格 × (绘制宽 / 地图宽)`（`:661-662`）。Crystal 的
//! `MLibrary.Draw(index, point, size, colour)` 就是按这个尺寸整体缩放（`MLibrary.cs:756-776`）。
//!
//! # 没做
//!
//! * 大地图的 NPC 列表、移动点按钮、世界地图、搜索框与「传送到 NPC」按钮
//!   （`BigMapDialog.cs:101-255,320-432`）；
//! * 「已探明区域」遮罩：Crystal 也没有（它直接画整张 `mmap.wzl` 大图）；
//! * `MiniMapDialog` 的两档尺寸（`Index = 2090` 大档 / `2091` 小档，
//!   `MainDialogs.cs:2046-2072`）与底部按钮（邮件/大地图/亮度）；
//! * 图库缺失时的 `SetSmallMode` 行为：本项目改为画纯色占位 + 地图名（不 panic）。

// Crystal 的 `scaleX/scaleY` 是 C# `float`（32 位，`MainDialogs.cs:1917-1918`），这里照抄；
// 地图尺寸远小于 2^23（8192 格以内都能被 f32 精确表示），转换不会真的丢精度。
#![allow(clippy::cast_precision_loss)]

use std::path::{Path, PathBuf};

use mir_asset::{Image, ImageLibrary};
use mir_core::Point;

use crate::frame::Frame;

/// 小地图视窗宽（`MainDialogs.cs:1912` 的 `new Rectangle(0, 0, 120, 108)`）。
pub const VIEW_WIDTH: u32 = 120;
/// 小地图视窗高（同上）。
pub const VIEW_HEIGHT: u32 = 108;

/// 大地图视区宽（`BigMapDialog.cs:559` 的 `Size = new Size(568, 380)`）。
pub const BIG_MAP_VIEW_WIDTH: u32 = 568;
/// 大地图视区高（同上）。
pub const BIG_MAP_VIEW_HEIGHT: u32 = 380;
/// 大地图画面相对对话框左上角的 X 偏移（`BigMapDialog.cs:652`：
/// `viewRect.X = 14 + (568 - 宽) / 2`）。
pub const BIG_MAP_OFFSET_X: u32 = 14;
/// 大地图画面相对对话框左上角的 Y 偏移（`BigMapDialog.cs:653`：
/// `viewRect.Y = 52 + (380 - 高) / 2`）。
pub const BIG_MAP_OFFSET_Y: u32 = 52;

/// 点位边长：Crystal `DXManager.Draw(RadarTexture, new Rectangle(0, 0, 2, 2), …)`
/// （`MainDialogs.cs:1961`）。
pub const MARKER_SIZE: i32 = 2;

/// 自己的点颜色：需求指定的黄色
/// （Crystal 画白色，`MainDialogs.cs:1950-1953`；这里刻意不同，文档已注明）。
pub const PLAYER_COLOR: [u8; 4] = [255, 255, 0, 255];
/// 队友/同组玩家的点颜色（`MainDialogs.cs:1948`，蓝）。
pub const GROUP_COLOR: [u8; 4] = [0, 0, 255, 255];
/// 其他玩家的点颜色（`MainDialogs.cs:1952`，白）。
pub const PLAYER_OTHER_COLOR: [u8; 4] = [255, 255, 255, 255];
/// NPC 的点颜色（`MainDialogs.cs:1956`，`(0, 255, 50)`）。
pub const NPC_COLOR: [u8; 4] = [0, 255, 50, 255];
/// 怪物等其余对象的点颜色（`MainDialogs.cs:1959`，红）。
pub const MONSTER_COLOR: [u8; 4] = [255, 0, 0, 255];

/// 点位类别（决定颜色，见模块文档）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MarkerKind {
    /// 自己。
    SelfPlayer,
    /// 队友/同组玩家。
    Group,
    /// 其他玩家。
    Player,
    /// NPC。
    Npc,
    /// 怪物、宠物等其余对象。
    Monster,
}

impl MarkerKind {
    /// 点的颜色（对应模块文档里的 Crystal 行号）。
    #[must_use]
    pub const fn color(self) -> [u8; 4] {
        match self {
            Self::SelfPlayer => PLAYER_COLOR,
            Self::Group => GROUP_COLOR,
            Self::Player => PLAYER_OTHER_COLOR,
            Self::Npc => NPC_COLOR,
            Self::Monster => MONSTER_COLOR,
        }
    }
}

/// 小地图上的一个点。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MinimapMarker {
    /// 地图格坐标（与 `MapInfo.Width/Height` 同一坐标系）。
    pub cell: Point,
    /// 类别（决定颜色）。
    pub kind: MarkerKind,
}

/// 一帧小地图要画什么。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MinimapView {
    /// 图像号：`S.MapInformation.MiniMap`（`ServerPackets.cs:479`）。
    ///
    /// `0` 表示这张图没有小地图（Crystal `MainDialogs.cs:1898-1907` 也会切到无图模式）；
    /// 是否把 0 当成「没有」由调用方决定（`mir-client` 的 `ui::minimap` 会画占位），
    /// 因为 `mmap.wzl` 的 0 号帧本身是一张真实地图图。
    pub image_index: u16,
    /// 地图宽（格）：`ClientMapInfo.Width`（`ClientData.cs:322`）。
    pub map_width: u32,
    /// 地图高（格）。
    pub map_height: u32,
    /// 玩家所在格（Crystal 用 `MapObject.User.CurrentLocation`，`MainDialogs.cs:1921-1922`）。
    pub player: Point,
    /// 其他点（不含玩家自己）。
    pub markers: Vec<MinimapMarker>,
}

impl Default for MinimapView {
    fn default() -> Self {
        Self {
            image_index: 0,
            map_width: 0,
            map_height: 0,
            player: Point::new(0, 0),
            markers: Vec::new(),
        }
    }
}

/// 图像与地图格之间的换算结果。
///
/// 字段与 Crystal `MainDialogs.cs:1916-1943` 的局部变量一一对应。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MinimapProjection {
    /// 图像内裁剪区：`(x, y, 宽, 高)`，对应 Crystal 的 `viewRect`（`:1920-1931`）。
    pub source: (i32, i32, u32, u32),
    /// 每格对应的横向像素数（`:1917`）。
    pub scale_x: f32,
    /// 每格对应的纵向像素数（`:1918`）。
    pub scale_y: f32,
    /// 裁剪区左上角对应的格坐标（`:1936-1937` 的 `startPointX/Y`）。
    pub start_cell: (i32, i32),
}

impl MinimapProjection {
    /// 把格坐标投影到视窗内像素（相对视窗左上角，四舍五入到整像素）。
    ///
    /// Crystal：`float x = ((格.x - startPointX) * scaleX) + drawLocation.X`
    /// （`MainDialogs.cs:1942-1943`，`drawLocation` 就是视窗原点），再用浮点画 2×2 的点
    /// （`:1961`）；这里取整到最近的像素。
    #[must_use]
    pub fn cell_to_view(&self, cell: Point) -> (i32, i32) {
        let x = (cell.x - self.start_cell.0) as f32 * self.scale_x;
        let y = (cell.y - self.start_cell.1) as f32 * self.scale_y;
        (round_to_i32(x), round_to_i32(y))
    }

    /// 视窗内像素是否落在 120×108 里（超出就剪掉，避免画到面板外）。
    #[must_use]
    pub fn in_view(x: i32, y: i32) -> bool {
        x >= 0 && y >= 0 && (x as u32) < VIEW_WIDTH && (y as u32) < VIEW_HEIGHT
    }
}

/// `f32` → `i32`，四舍五入且饱和（坐标可能是负值：视窗左/上边外的对象）。
fn round_to_i32(value: f32) -> i32 {
    if value.is_finite() {
        value.round() as i32
    } else {
        0
    }
}

/// `f32` → `i32`，**截断**取整（Crystal 的 `(int)` 强转，`MainDialogs.cs:1921-1922`）。
fn trunc_to_i32(value: f32) -> i32 {
    if value.is_finite() {
        value.trunc() as i32
    } else {
        0
    }
}

/// 大地图的整图缩放结果（`BigMapDialog.cs:649-676`）。
///
/// ```text
/// 绘制尺寸 = (min(568, 图像宽), min(380, 图像高))       // :650
/// 居中偏移 = ((568 - 宽) / 2, (380 - 高) / 2)           // :652-653
/// scaleX  = 绘制宽 / 地图宽(格)                         // :661
/// scaleY  = 绘制高 / 地图高(格)                         // :662
/// ```
///
/// [`Self::offset`] 是**相对 568×380 视区左上角**的居中偏移；调用方再把视区放到
/// 对话框里的 `(14, 52)`（[`BIG_MAP_OFFSET_X`] / [`BIG_MAP_OFFSET_Y`]）。
///
/// # 与 Crystal 的一处偏差
///
/// Crystal 的大地图把**整张图**缩放绘制（`BigMapDialog.cs:676` 的
/// `Draw(index, DisplayLocation, Size, …)`，缩放实现见 `MLibrary.cs:756-776`），
/// 但点位仍减去 `startPointX/Y`（`:678-705`）。玩家不在图像边缘时 `startPoint`
/// 非 0，点位会整体偏移，而玩家自己的点却按 `格 × 比例` 画（`:707-710`）——
/// 这是 Crystal 自带的 bug。本作按「整图像素 = 格 × 比例」投影（需求也要求
/// 「画整张图 + 自己位置点」）。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BigMapProjection {
    /// 图像绘制尺寸（宽, 高），`min(视区, 图像)`（`BigMapDialog.cs:650`）。
    pub size: (u32, u32),
    /// 图像在视区内的居中偏移（相对视区左上角，`BigMapDialog.cs:652-653`）。
    pub offset: (i32, i32),
    /// 每格对应的横向像素数（`BigMapDialog.cs:661`）。
    pub scale_x: f32,
    /// 每格对应的纵向像素数（`:662`）。
    pub scale_y: f32,
}

impl BigMapProjection {
    /// 把地图格投影到视区内像素（相对视区左上角，四舍五入到整像素）。
    ///
    /// Crystal：`x = 格.x * ScaleX`、`y = 格.y * ScaleY`（`:707-710` 的玩家点），
    /// 再减去图像偏移得到绘制位置；这里把两步合并成一个返回值。
    #[must_use]
    pub fn cell_to_view(&self, cell: Point) -> (i32, i32) {
        (
            round_to_i32(self.offset.0 as f32 + cell.x as f32 * self.scale_x),
            round_to_i32(self.offset.1 as f32 + cell.y as f32 * self.scale_y),
        )
    }

    /// 视区内像素是否落在 568×380 里（超出就剪掉，避免画到对话框外）。
    #[must_use]
    pub fn in_view(x: i32, y: i32) -> bool {
        x >= 0 && y >= 0 && (x as u32) < BIG_MAP_VIEW_WIDTH && (y as u32) < BIG_MAP_VIEW_HEIGHT
    }
}

/// 小地图图库来源。
enum Source {
    /// 真实图库（`mmap.wzl` + `mmap.wzx`）。
    Library(ImageLibrary),
    /// 内存里的合成图库（无头测试用；`mir-asset` 只读不写，没法现造 `.wzl`）。
    Images(std::collections::BTreeMap<u16, Image>),
}

/// 小地图图像解码缓存与图库句柄。
///
/// 缺素材时是「空场景」：[`Self::image_size`] 返回 `None`，调用方画纯色占位。
pub struct MinimapScene {
    /// 图库。
    source: Option<Source>,
    /// 已打开的文件路径（诊断/日志用）。
    path: Option<PathBuf>,
    /// 解码缓存：`None` 表示该帧号没有可用图像（越界/空图/解码失败）。
    cache: std::collections::HashMap<u16, Option<Image>>,
}

impl std::fmt::Debug for MinimapScene {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MinimapScene")
            .field("path", &self.path)
            .field("has_source", &self.source.is_some())
            .field("cached", &self.cache.len())
            .finish()
    }
}

impl Default for MinimapScene {
    fn default() -> Self {
        Self::empty()
    }
}

impl MinimapScene {
    /// 空场景（没有素材）：所有查询都返回 `None`，绘制变成纯色占位。
    #[must_use]
    pub fn empty() -> Self {
        Self {
            source: None,
            path: None,
            cache: std::collections::HashMap::new(),
        }
    }

    /// 打开数据目录里的 `mmap.wzl`（大小写不敏感）。
    ///
    /// 文件不存在或打不开时**退化成空场景**并返回 `(空场景, 错误原因)`：
    /// 小地图缺素材不应该影响主画面（需求：显示纯色占位 + 地图名，不 panic）。
    /// 成功时返回 `(场景, None)`。
    #[must_use]
    pub fn open_dir(directory: impl AsRef<Path>) -> (Self, Option<String>) {
        let directory = directory.as_ref();
        let Some(path) = find_case_insensitive(directory, "mmap.wzl") else {
            return (
                Self::empty(),
                Some(format!("{} 下没有 mmap.wzl", directory.display())),
            );
        };
        match ImageLibrary::open(&path) {
            Ok(library) => (
                Self {
                    source: Some(Source::Library(library)),
                    path: Some(path),
                    cache: std::collections::HashMap::new(),
                },
                None,
            ),
            Err(error) => (
                Self::empty(),
                Some(format!("打开 {} 失败：{error}", path.display())),
            ),
        }
    }

    /// 用合成图库建场景（无头测试用）。
    ///
    /// 键是图像号（`MapInfo.MiniMap`），值是该图像的 RGBA 像素。
    #[must_use]
    pub fn from_images(images: std::collections::BTreeMap<u16, Image>) -> Self {
        Self {
            source: Some(Source::Images(images)),
            path: None,
            cache: std::collections::HashMap::new(),
        }
    }

    /// 是否有素材（`false` 时调用方应当画占位）。
    #[must_use]
    pub fn is_available(&self) -> bool {
        self.source.is_some()
    }

    /// 打开时用的文件路径（没有素材时为 `None`）。
    #[must_use]
    pub fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }

    /// 取一张小地图图像（解码后缓存）；图像号不存在/空图/解码失败返回 `None`。
    pub fn image(&mut self, index: u16) -> Option<&Image> {
        if !self.cache.contains_key(&index) {
            let loaded = match self.source.as_ref()? {
                Source::Library(library) => library
                    .image(usize::from(index))
                    .ok()
                    .filter(|image| !image.is_empty()),
                Source::Images(images) => images
                    .get(&index)
                    .filter(|image| !image.is_empty())
                    .cloned(),
            };
            self.cache.insert(index, loaded);
        }
        self.cache.get(&index)?.as_ref()
    }

    /// 一张小地图图像的尺寸（像素）；没有图像时返回 `None`。
    pub fn image_size(&mut self, index: u16) -> Option<(u32, u32)> {
        let image = self.image(index)?;
        Some((u32::from(image.info.width), u32::from(image.info.height)))
    }

    /// 计算投影：把哪一块图像画到 120×108 的视窗里、以及格→像素的缩放系数。
    ///
    /// 完全照抄 `MainDialogs.cs:1912-1937`；`map_width/map_height` 为 0
    /// （还没收到 `NewMapInfo`）时返回 `None`。
    pub fn projection(&mut self, view: &MinimapView) -> Option<MinimapProjection> {
        if view.map_width == 0 || view.map_height == 0 {
            return None;
        }
        let (image_width, image_height) = self.image_size(view.image_index)?;
        if image_width == 0 || image_height == 0 {
            return None;
        }
        let scale_x = image_width as f32 / view.map_width as f32;
        let scale_y = image_height as f32 / view.map_height as f32;

        // `viewRect.Location = ((int)(scaleX * player.X) - w/2, (int)(scaleY * player.Y) - h/2)`
        // （`MainDialogs.cs:1920-1922`；`(int)` 是截断，不是四舍五入）。
        let mut x = trunc_to_i32(scale_x * view.player.x as f32) - VIEW_WIDTH as i32 / 2;
        let mut y = trunc_to_i32(scale_y * view.player.y as f32) - VIEW_HEIGHT as i32 / 2;

        // 右/下越界时贴边（:1925-1928），再夹到 0（:1930-1931）。
        let max_x = image_width as i32 - VIEW_WIDTH as i32;
        let max_y = image_height as i32 - VIEW_HEIGHT as i32;
        if x + VIEW_WIDTH as i32 >= image_width as i32 {
            x = max_x;
        }
        if y + VIEW_HEIGHT as i32 >= image_height as i32 {
            y = max_y;
        }
        x = x.max(0);
        y = y.max(0);

        // 图像比视窗小的时候，裁剪区不能超出图像（Crystal 的 Draw 会被库内部裁掉；
        // 这里显式收窄，省得后面越界取样）。
        let width = VIEW_WIDTH.min(image_width.saturating_sub(x.max(0) as u32));
        let height = VIEW_HEIGHT.min(image_height.saturating_sub(y.max(0) as u32));

        // `startPointX = (int)(viewRect.X / scaleX)`（`MainDialogs.cs:1936-1937`）。
        let start_cell = (
            trunc_to_i32(x as f32 / scale_x),
            trunc_to_i32(y as f32 / scale_y),
        );
        Some(MinimapProjection {
            source: (x, y, width, height),
            scale_x,
            scale_y,
            start_cell,
        })
    }

    /// 把底图与点位画到 `frame` 的 `(dest_x, dest_y)` 处（左上角），视窗 120×108。
    ///
    /// 返回值：是否真的画了底图（`false` = 没有素材，调用方应画纯色占位）。
    /// 点位需要投影，而投影来自底图尺寸（`scaleX = 图像宽 / 地图宽`，`MainDialogs.cs:1917`），
    /// 所以没有底图时点位也不画——此时由调用方画占位色块 + 地图名。
    pub fn draw(
        &mut self,
        frame: &mut Frame,
        dest_x: i32,
        dest_y: i32,
        view: &MinimapView,
    ) -> bool {
        let projection = self.projection(view);
        let mut drew_image = false;
        if let Some(projection) = &projection {
            if let Some(image) = self.image(view.image_index) {
                blit_crop(frame, image, projection.source, dest_x, dest_y);
                drew_image = true;
            }
        }
        let Some(projection) = projection else {
            return drew_image;
        };
        for marker in &view.markers {
            draw_marker(frame, dest_x, dest_y, projection, marker.cell, marker.kind);
        }
        // 自己在最后画，保证不会被别的点盖住（Crystal 的遍历顺序不保证，这里是刻意的）。
        draw_marker(
            frame,
            dest_x,
            dest_y,
            projection,
            view.player,
            MarkerKind::SelfPlayer,
        );
        drew_image
    }

    /// 计算大地图的整图缩放：把哪一张图缩放成多大、画在视区里的哪里。
    ///
    /// 完全照抄 `BigMapDialog.cs:649-662`；`image_index == 0`（这张图没有大地图，
    /// `BigMapDialog.Show` 会直接返回，`:289`）或还没收到 `NewMapInfo`（宽高为 0）时
    /// 返回 `None`。
    pub fn big_map_projection(&mut self, view: &MinimapView) -> Option<BigMapProjection> {
        if view.image_index == 0 || view.map_width == 0 || view.map_height == 0 {
            return None;
        }
        let (image_width, image_height) = self.image_size(view.image_index)?;
        if image_width == 0 || image_height == 0 {
            return None;
        }
        // `Rectangle viewRect = new Rectangle(0, 0, Math.Min(568, Size.Width), Math.Min(380, Size.Height))`
        // （`BigMapDialog.cs:650`）。
        let width = image_width.min(BIG_MAP_VIEW_WIDTH);
        let height = image_height.min(BIG_MAP_VIEW_HEIGHT);
        // `viewRect.X = 14 + (568 - viewRect.Width) / 2`（`:652`）；
        // 14/52 是视区在对话框里的位置，`offset` 只记「视区内居中」的那部分。
        let offset_x = i32::try_from((BIG_MAP_VIEW_WIDTH - width) / 2).unwrap_or(0);
        let offset_y = i32::try_from((BIG_MAP_VIEW_HEIGHT - height) / 2).unwrap_or(0);
        // `ScaleX = Size.Width / (float)MapInfo.Width`（`:661-662`）。
        let scale_x = width as f32 / view.map_width as f32;
        let scale_y = height as f32 / view.map_height as f32;
        Some(BigMapProjection {
            size: (width, height),
            offset: (offset_x, offset_y),
            scale_x,
            scale_y,
        })
    }

    /// 把大地图（整图缩放）与点位画到 568×380 的视区里，视区左上角是 `(dest_x, dest_y)`。
    ///
    /// 调用方应把 `(dest_x, dest_y)` 放在对话框的 `(14, 52)` 处
    /// （[`BIG_MAP_OFFSET_X`] / [`BIG_MAP_OFFSET_Y`]，`BigMapDialog.cs:650-653`）。
    /// 返回值：是否真的画了底图（`false` = 没有素材，调用方应画纯色占位 + 地图名）。
    pub fn draw_big_map(
        &mut self,
        frame: &mut Frame,
        dest_x: i32,
        dest_y: i32,
        view: &MinimapView,
    ) -> bool {
        let projection = self.big_map_projection(view);
        let mut drew_image = false;
        if let Some(projection) = &projection {
            if let Some(image) = self.image(view.image_index) {
                blit_scaled(
                    frame,
                    image,
                    dest_x + projection.offset.0,
                    dest_y + projection.offset.1,
                    projection.size,
                );
                drew_image = true;
            }
        }
        let Some(projection) = projection else {
            return drew_image;
        };
        for marker in &view.markers {
            draw_big_marker(frame, dest_x, dest_y, projection, marker.cell, marker.kind);
        }
        // 自己也用 2×2 的点，颜色是需求指定的黄色（[`PLAYER_COLOR`]）。
        draw_big_marker(
            frame,
            dest_x,
            dest_y,
            projection,
            view.player,
            MarkerKind::SelfPlayer,
        );
        drew_image
    }
}

/// 画一个 2×2 的点（Crystal `DXManager.Draw(RadarTexture, new Rectangle(0, 0, 2, 2),
/// new Vector3(x - 0.5, y - 0.5, 0), colour)`，`MainDialogs.cs:1961`）。
fn draw_marker(
    frame: &mut Frame,
    dest_x: i32,
    dest_y: i32,
    projection: MinimapProjection,
    cell: Point,
    kind: MarkerKind,
) {
    let (x, y) = projection.cell_to_view(cell);
    let color = kind.color();
    for offset_y in 0..MARKER_SIZE {
        for offset_x in 0..MARKER_SIZE {
            let px = x + offset_x;
            let py = y + offset_y;
            if !MinimapProjection::in_view(px, py) {
                continue;
            }
            let (Ok(frame_x), Ok(frame_y)) =
                (u32::try_from(dest_x + px), u32::try_from(dest_y + py))
            else {
                continue;
            };
            frame.set_pixel(frame_x, frame_y, color);
        }
    }
}

/// 画大地图上的一个 2×2 点（与 [`draw_marker`] 同一尺寸/颜色，`MainDialogs.cs:1961`）。
///
/// 位置用 [`BigMapProjection::cell_to_view`]（`格 × 比例 + 居中偏移`），
/// 剪裁区是 568×380（`BigMapDialog.cs:650`）。
fn draw_big_marker(
    frame: &mut Frame,
    dest_x: i32,
    dest_y: i32,
    projection: BigMapProjection,
    cell: Point,
    kind: MarkerKind,
) {
    let (x, y) = projection.cell_to_view(cell);
    let color = kind.color();
    for offset_y in 0..MARKER_SIZE {
        for offset_x in 0..MARKER_SIZE {
            let px = x + offset_x;
            let py = y + offset_y;
            if !BigMapProjection::in_view(px, py) {
                continue;
            }
            let (Ok(frame_x), Ok(frame_y)) =
                (u32::try_from(dest_x + px), u32::try_from(dest_y + py))
            else {
                continue;
            };
            frame.set_pixel(frame_x, frame_y, color);
        }
    }
}

/// 把一张图像的某个源像素按 source-over 写到帧上（`alpha == 0` 跳过）。
fn copy_pixel(frame: &mut Frame, image: &Image, source_x: u32, source_y: u32, dest: (u32, u32)) {
    let image_width = u32::from(image.info.width);
    if source_x >= image_width || source_y >= u32::from(image.info.height) {
        return;
    }
    let offset =
        usize::try_from((u64::from(source_y) * u64::from(image_width) + u64::from(source_x)) * 4)
            .unwrap_or(usize::MAX);
    let Some(pixel) = image.pixels.get(offset..offset + 4) else {
        return;
    };
    match pixel[3] {
        0 => {}
        255 => frame.set_pixel(dest.0, dest.1, [pixel[0], pixel[1], pixel[2], 255]),
        alpha => frame.blend_pixel(
            i32::try_from(dest.0).unwrap_or(0),
            i32::try_from(dest.1).unwrap_or(0),
            [pixel[0], pixel[1], pixel[2], 255],
            alpha,
        ),
    }
}

/// 把整张 `image` **最近邻缩放**到 `size` 并画到 `(dest_x, dest_y)`（左上角）。
///
/// Crystal 的大地图用 `MLibrary.Draw(index, point, size, colour)`（`MLibrary.cs:756-776`）
/// 做同样的整图缩放（`Matrix.Scaling(size.Width / image.Width, …)`）；Direct3D 采样是
/// 双线性，这里按项目惯例用最近邻（渲染层一律最近邻，结果可确定性测试）。
fn blit_scaled(frame: &mut Frame, image: &Image, dest_x: i32, dest_y: i32, size: (u32, u32)) {
    let source_width = u32::from(image.info.width);
    let source_height = u32::from(image.info.height);
    let (target_width, target_height) = size;
    if source_width == 0 || source_height == 0 || target_width == 0 || target_height == 0 {
        return;
    }
    for row in 0..target_height {
        let source_y = row * source_height / target_height;
        let Ok(frame_y) = u32::try_from(dest_y + i32::try_from(row).unwrap_or(0)) else {
            continue;
        };
        for column in 0..target_width {
            let source_x = column * source_width / target_width;
            let Ok(frame_x) = u32::try_from(dest_x + i32::try_from(column).unwrap_or(0)) else {
                continue;
            };
            copy_pixel(frame, image, source_x, source_y, (frame_x, frame_y));
        }
    }
}

/// 把 `image` 的 `source` 区域逐像素复制到 `frame` 的 `(dest_x, dest_y)`。
///
/// `Frame` 只有整图 `blit`，没有裁剪绘制，所以这里自己做；小地图视窗只有
/// 120×108，逐像素开销可以忽略。alpha < 255 的像素做 source-over 混合
/// （Crystal 的 `Draw(..., fade)` 用的也是混合）。
fn blit_crop(
    frame: &mut Frame,
    image: &Image,
    source: (i32, i32, u32, u32),
    dest_x: i32,
    dest_y: i32,
) {
    let (source_x, source_y, width, height) = source;
    for row in 0..height {
        for column in 0..width {
            let x = source_x + i32::try_from(column).unwrap_or(0);
            let y = source_y + i32::try_from(row).unwrap_or(0);
            if x < 0 || y < 0 {
                continue;
            }
            let (Ok(x), Ok(y)) = (u32::try_from(x), u32::try_from(y)) else {
                continue;
            };
            let (Ok(frame_x), Ok(frame_y)) = (
                u32::try_from(dest_x + i32::try_from(column).unwrap_or(0)),
                u32::try_from(dest_y + i32::try_from(row).unwrap_or(0)),
            ) else {
                continue;
            };
            copy_pixel(frame, image, x, y, (frame_x, frame_y));
        }
    }
}

/// 在目录里找名字不区分大小写的文件（与 `tiles.rs` 同一策略）。
fn find_case_insensitive(directory: &Path, file_name: &str) -> Option<PathBuf> {
    let direct = directory.join(file_name);
    if direct.is_file() {
        return Some(direct);
    }
    std::fs::read_dir(directory)
        .ok()?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .find(|candidate| {
            candidate.is_file()
                && candidate
                    .file_name()
                    .is_some_and(|name| name.to_string_lossy().eq_ignore_ascii_case(file_name))
        })
}

#[cfg(test)]
mod tests {
    use mir_asset::{Image, ImageInfo};

    use super::*;

    /// 造一张纯色的合成小地图图像（RGB565 之外的格式不影响这里：直接给 RGBA）。
    fn solid_image(width: u16, height: u16, color: [u8; 4]) -> Image {
        let mut pixels = Vec::with_capacity(usize::from(width) * usize::from(height) * 4);
        for _ in 0..u32::from(width) * u32::from(height) {
            pixels.extend_from_slice(&color);
        }
        Image {
            info: ImageInfo {
                width,
                height,
                offset_x: 0,
                offset_y: 0,
                format: 0,
                shadow_x: 0,
                shadow_y: 0,
                reserved: [0; 3],
                has_mask: false,
                data_offset: 0,
                data_len: 0,
            },
            pixels,
        }
    }

    /// 合成图库：300×200 的灰色底图放在 7 号帧。
    fn synthetic_scene() -> MinimapScene {
        let mut images = std::collections::BTreeMap::new();
        images.insert(7, solid_image(300, 200, [40, 60, 80, 255]));
        MinimapScene::from_images(images)
    }

    fn view() -> MinimapView {
        MinimapView {
            image_index: 7,
            map_width: 600,
            map_height: 400,
            player: Point::new(300, 200),
            markers: Vec::new(),
        }
    }

    #[test]
    fn empty_scene_reports_no_image() {
        let mut scene = MinimapScene::empty();
        assert!(!scene.is_available());
        assert!(scene.path().is_none());
        assert!(scene.image(7).is_none());
        assert!(scene.image_size(7).is_none());
        assert!(scene.projection(&view()).is_none());
    }

    #[test]
    fn missing_directory_degrades_without_panicking() {
        let directory =
            std::env::temp_dir().join(format!("mir-render-minimap-missing-{}", std::process::id()));
        let (mut scene, reason) = MinimapScene::open_dir(&directory);
        assert!(reason.is_some(), "缺目录时应当给出原因");
        assert!(!scene.is_available());
        // 退化后仍然能画（画不出底图，但不会 panic）。
        let mut frame = Frame::new(200, 200);
        let drew = scene.draw(&mut frame, 10, 10, &view());
        assert!(!drew);
    }

    #[test]
    fn projection_centers_on_player_like_crystal() {
        let mut scene = synthetic_scene();
        let projection = scene.projection(&view()).expect("应当能算出投影");
        // 图像 300×200、地图 600×400 → 每格 0.5 像素。
        assert!((projection.scale_x - 0.5).abs() < 1e-6);
        assert!((projection.scale_y - 0.5).abs() < 1e-6);
        // 玩家 (300, 200) → 图像像素 (150, 100)，视窗 120×108 居中 → (90, 46)。
        assert_eq!(projection.source.0, 90);
        assert_eq!(projection.source.1, 46);
        assert_eq!(
            (projection.source.2, projection.source.3),
            (VIEW_WIDTH, VIEW_HEIGHT)
        );
        // startCell = 90 / 0.5 = 180、46 / 0.5 = 92。
        assert_eq!(projection.start_cell, (180, 92));
        // 玩家自己投影到视窗中心附近。
        let (x, y) = projection.cell_to_view(Point::new(300, 200));
        assert_eq!((x, y), (60, 54));
    }

    #[test]
    fn projection_clamps_at_map_edges() {
        let mut scene = synthetic_scene();
        let mut corner = view();
        corner.player = Point::new(0, 0);
        let projection = scene.projection(&corner).expect("投影");
        assert_eq!(projection.source.0, 0);
        assert_eq!(projection.source.1, 0);

        let mut far = corner.clone();
        far.player = Point::new(599, 399);
        let projection = scene.projection(&far).expect("投影");
        // 300 - 120 = 180、200 - 108 = 92，贴右下边。
        assert_eq!(projection.source.0, 180);
        assert_eq!(projection.source.1, 92);
    }

    #[test]
    fn small_image_does_not_overflow_the_view() {
        let mut images = std::collections::BTreeMap::new();
        images.insert(3, solid_image(50, 40, [1, 2, 3, 255]));
        let mut scene = MinimapScene::from_images(images);
        let view = MinimapView {
            image_index: 3,
            map_width: 100,
            map_height: 80,
            player: Point::new(10, 10),
            markers: Vec::new(),
        };
        let projection = scene.projection(&view).expect("投影");
        assert_eq!((projection.source.2, projection.source.3), (50, 40));
    }

    #[test]
    fn zero_sized_map_has_no_projection() {
        let mut scene = synthetic_scene();
        let mut view = view();
        view.map_width = 0;
        assert!(scene.projection(&view).is_none());
    }

    #[test]
    fn drawing_puts_map_pixels_and_a_yellow_player_dot() {
        let mut scene = synthetic_scene();
        let mut frame = Frame::new(200, 200);
        frame.clear([0, 0, 0, 255]);
        let drew = scene.draw(&mut frame, 10, 10, &view());
        assert!(drew, "有素材时应当画出底图");
        // 视窗内某处应当是底图颜色（灰蓝）。
        assert_eq!(frame.pixel(10 + 5, 10 + 5), Some([40, 60, 80, 255]));
        // 玩家点在视窗内偏移 (60, 54) → 帧坐标 (70, 64)，2×2 黄色。
        for (dx, dy) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
            assert_eq!(
                frame.pixel(10 + 60 + dx, 10 + 54 + dy),
                Some(PLAYER_COLOR),
                "玩家点应画成黄色（{dx},{dy}）"
            );
        }
        // 视窗外没有画东西。
        assert_eq!(frame.pixel(9, 9), Some([0, 0, 0, 255]));
    }

    #[test]
    fn markers_take_their_crystal_colours() {
        let mut scene = synthetic_scene();
        let mut frame = Frame::new(200, 200);
        frame.clear([0, 0, 0, 255]);
        let mut view = view();
        view.player = Point::new(300, 200);
        view.markers = vec![
            MinimapMarker {
                cell: Point::new(200, 200),
                kind: MarkerKind::Monster,
            },
            MinimapMarker {
                cell: Point::new(400, 200),
                kind: MarkerKind::Group,
            },
            MinimapMarker {
                cell: Point::new(300, 100),
                kind: MarkerKind::Npc,
            },
        ];
        assert!(scene.draw(&mut frame, 0, 0, &view));
        // 视窗原点 (=投影 start_cell 180,92)：怪物 (200,200) → ((200-180)*0.5, (200-92)*0.5) = (10, 54)
        assert_eq!(frame.pixel(10, 54), Some(MONSTER_COLOR));
        // 队友 (400,200) → (110, 54)
        assert_eq!(frame.pixel(110, 54), Some(GROUP_COLOR));
        // NPC (300,100) → (60, 4)
        assert_eq!(frame.pixel(60, 4), Some(NPC_COLOR));
    }

    #[test]
    fn markers_outside_the_view_are_clipped() {
        let mut scene = synthetic_scene();
        let mut frame = Frame::new(200, 200);
        frame.clear([0, 0, 0, 255]);
        let mut view = view();
        view.markers.push(MinimapMarker {
            cell: Point::new(10, 10),
            kind: MarkerKind::Monster,
        });
        assert!(scene.draw(&mut frame, 10, 10, &view));
        // 视窗内不应出现红色。
        let mut red = 0;
        for y in 0..VIEW_HEIGHT {
            for x in 0..VIEW_WIDTH {
                if frame.pixel(10 + x, 10 + y) == Some(MONSTER_COLOR) {
                    red += 1;
                }
            }
        }
        assert_eq!(red, 0, "视窗外的怪物点不应被画出来");
    }

    #[test]
    fn empty_scene_draws_nothing() {
        // 没有素材（例如 CI 上没有 mmap.wzl）：底图与点位都画不出来，
        // 调用方（`mir-client` 的小地图面板）会改成纯色占位 + 地图名。
        let mut scene = MinimapScene::empty();
        let mut frame = Frame::new(200, 200);
        frame.clear([0, 0, 0, 255]);
        let drew = scene.draw(&mut frame, 10, 10, &view());
        assert!(!drew, "空场景没有底图");
        assert_eq!(frame.pixel(10 + 60, 10 + 54), Some([0, 0, 0, 255]));
        assert_eq!(frame.pixel(10, 10), Some([0, 0, 0, 255]));
    }

    #[test]
    fn big_map_projection_keeps_a_small_image_at_one_to_one_and_centers_it() {
        let mut scene = synthetic_scene(); // 7 号帧 300×200
        // 地图 600×400 → 每格 0.5 像素；图像比视区小，按原尺寸居中。
        let projection = scene.big_map_projection(&view()).expect("应当能算投影");
        assert_eq!(projection.size, (300, 200));
        assert_eq!(projection.offset, (134, 90)); // (568-300)/2、(380-200)/2
        assert!((projection.scale_x - 0.5).abs() < 1e-6);
        assert!((projection.scale_y - 0.5).abs() < 1e-6);
        // 玩家 (300, 200) → (134 + 150, 90 + 100)。
        assert_eq!(projection.cell_to_view(Point::new(300, 200)), (284, 190));
        // 视区角落不属于图像：内部偏移之外不画。
        assert!(!BigMapProjection::in_view(-1, 0));
        assert!(BigMapProjection::in_view(0, 0));
        assert!(BigMapProjection::in_view(567, 379));
        assert!(!BigMapProjection::in_view(568, 380));
    }

    #[test]
    fn big_map_projection_downscales_a_large_image_to_the_view() {
        let mut images = std::collections::BTreeMap::new();
        images.insert(7, solid_image(600, 399, [40, 60, 80, 255]));
        let mut scene = MinimapScene::from_images(images);
        let mut view = view();
        view.map_width = 700;
        view.map_height = 700;
        let projection = scene.big_map_projection(&view).expect("投影");
        // `Math.Min(568, 600) × Math.Min(380, 399)`（`BigMapDialog.cs:650`）。
        assert_eq!(projection.size, (568, 380));
        assert_eq!(projection.offset, (0, 0));
        assert!((projection.scale_x - 568.0 / 700.0).abs() < 1e-6);
        assert!((projection.scale_y - 380.0 / 700.0).abs() < 1e-6);
    }

    #[test]
    fn drawing_the_big_map_scales_the_image_and_marks_the_player() {
        let mut scene = synthetic_scene();
        let mut frame = Frame::new(600, 500);
        frame.clear([0, 0, 0, 255]);
        // 视区左上角放在 (10, 20)，与对话框的 `(14, 52)` 同义（`BigMapDialog.cs:652-653`）。
        let drew = scene.draw_big_map(&mut frame, 10, 20, &view());
        assert!(drew, "有素材时应画出大地图");
        // 图像居中：偏移 (134, 90)。
        assert_eq!(
            frame.pixel(10 + 134 + 5, 20 + 90 + 5),
            Some([40, 60, 80, 255])
        );
        // 图像外（视区左上角）保持黑色。
        assert_eq!(frame.pixel(10, 20), Some([0, 0, 0, 255]));
        // 玩家点：(284, 190) 处的 2×2 黄点。
        for (dx, dy) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
            assert_eq!(
                frame.pixel(10 + 284 + dx, 20 + 190 + dy),
                Some(PLAYER_COLOR),
                "玩家点应画成黄色（{dx},{dy}）"
            );
        }
    }

    #[test]
    fn big_map_needs_an_image_and_a_non_zero_index() {
        let mut scene = synthetic_scene();
        let mut no_asset = MinimapScene::empty();
        assert!(no_asset.big_map_projection(&view()).is_none());
        // `MapInfo.BigMap == 0`：Crystal `BigMapDialog.Show` 直接返回（`:289`）。
        let mut zero = view();
        zero.image_index = 0;
        assert!(scene.big_map_projection(&zero).is_none());
        // 没有 `NewMapInfo`（宽高为 0）时也算不出比例。
        let mut no_size = view();
        no_size.map_width = 0;
        assert!(scene.big_map_projection(&no_size).is_none());
        // 画不出来时调用方画占位。
        let mut frame = Frame::new(200, 200);
        frame.clear([0, 0, 0, 255]);
        assert!(!no_asset.draw_big_map(&mut frame, 10, 10, &view()));
        assert_eq!(frame.pixel(10, 10), Some([0, 0, 0, 255]));
    }
}
