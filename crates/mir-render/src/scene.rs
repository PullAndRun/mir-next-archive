//! 地图场景：地图 + 图库 + 解码缓存，按 Crystal 的两趟循环画出一帧。
//!
//! # 绘制顺序
//!
//! 与 `MapControl.CreateTexture` 一致（`GameScene.cs:10549-10635`）：
//!
//! 1. 清屏（黑，`GameScene.cs:10570`）；
//! 2. 铺地层 `DrawFloor`（`GameScene.cs:10658-10764`），逐行 y、逐列 x：
//!    * 后层 `BackImage`：只在 **x、y 都是偶数** 的格子上画（`:10703`），
//!      96×64 的菱形瓦片正好铺满 2×2 格；
//!    * 中层：尺寸为 48×32 或 96×64 时画在瓦片左上角（`:10714-10724`）；
//!    * 前层：尺寸为 48×32 或 96×64 时画在瓦片左上角（`:10727-10758`），
//!      门关闭时不做 `DoorOffset` 帧号调整；
//! 3. 对象层 `DrawObjects`（`GameScene.cs:10798-10946`），逐行 y、逐列 x：
//!    * 中层里「非格尺寸」的图像以格子底边为锚点向上画（`:10839-10874`）；
//!    * 前层里「非格尺寸」的图像同样以底边为锚点向上画（`:10877-10937`），
//!      并按 `o+8` 的动画帧号与 `0x80` 混合标志选择帧/混合方式。
//!
//! 同一行里按 x 升序逐格先画中层再画前层（Crystal 的 x 循环里 middle 在 front 之前，
//! 行与行之间按 y 升序，因此靠下的格子覆盖靠上的格子。
//!
//! # 解码缓存
//!
//! 每张图只解码一次，键是 `(MapLibs 编号, 图像下标)`。缓存上限
//! [`MAX_CACHED_IMAGES`] 张，超过后整体清空（地图视图里同时出现的图远少于这个数，
//! 不会反复抖动）。

use std::collections::HashMap;
use std::path::Path;

use mir_asset::{Image, ImageInfo, format};
use mir_core::Point;
use mir_world::{Map, MapCell, MapVersion};

use crate::entities::{EntityLibraries, EntityRenderer, EntityView};
use crate::geometry::{self, CELL_HEIGHT, CELL_WIDTH, Viewport};
use crate::tiles::TileLibraries;
use crate::{BACKGROUND, Frame, RenderError};

/// 解码缓存上限（张）。一张 96×64 的 RGBA 图约 24 KiB，4096 张约 100 MiB。
pub const MAX_CACHED_IMAGES: usize = 4096;

/// Crystal 的 `MapControl.AnimationCount` 在本 crate 里的对应物：每帧递增的动画计数。
///
/// Crystal 在 `GameScene.cs:1192` 每次 `Process` 递增它；M6 由客户端按重绘节拍递增，
/// 测试里保持 0 就能得到确定性的静态帧。
const DEFAULT_ANIMATION_FRAME: u32 = 0;

/// 一张地图的渲染场景。
///
/// ```no_run
/// use mir_core::Point;
/// use mir_render::{Frame, MapScene};
///
/// let mut scene = MapScene::load("resource/map/0.map", "resource/data")?;
/// let center = Point::new(330, 270);
/// let mut frame = Frame::new(800, 600);
/// scene.render(&mut frame, center);
/// let (x, y) = scene.world_to_screen(center, Point::new(331, 271));
/// assert_eq!((x, y), (432, 288));
/// # Ok::<(), mir_render::RenderError>(())
/// ```
#[derive(Debug)]
pub struct MapScene {
    /// 已解析的地图（按列优先存放，见 `mir-world`）。
    pub map: Map,
    /// 当前视口；[`MapScene::render`] 会按传入帧的尺寸更新它。
    viewport: Viewport,
    /// 地图图库。
    libraries: TileLibraries,
    /// 实体图库与解码缓存（玩家、怪物、NPC、地面物品）。
    entities: EntityRenderer,
    /// 解码缓存。
    cache: DecodeCache,
    /// 动画计数（Crystal `MapControl.AnimationCount`）。
    animation_frame: u32,
}

impl MapScene {
    /// 读取地图与素材目录，构造场景。
    ///
    /// # Errors
    /// 地图读取失败、素材目录不存在、没有可用图库或图库损坏时返回 [`RenderError`]。
    pub fn load(
        map_path: impl AsRef<Path>,
        data_dir: impl AsRef<Path>,
    ) -> Result<Self, RenderError> {
        let map = Map::load(map_path.as_ref())?;
        let libraries = TileLibraries::open_dir(data_dir.as_ref())?;
        // 实体图库是可选增强：素材目录里没有 `hum.wzl` / `mon*.wzl` 时仍然能画地图，
        // 实体退化为彩色占位方块（见 `entities` 模块）。
        let entities = EntityRenderer::new(EntityLibraries::open_dir(data_dir.as_ref()));
        Ok(Self {
            map,
            viewport: Viewport::DEFAULT,
            libraries,
            entities,
            cache: DecodeCache::default(),
            animation_frame: DEFAULT_ANIMATION_FRAME,
        })
    }

    /// 用已经加载好的地图与图库构造场景（测试、工具链用）。
    #[must_use]
    pub fn from_parts(map: Map, libraries: TileLibraries, viewport: Viewport) -> Self {
        Self {
            map,
            viewport,
            libraries,
            entities: EntityRenderer::empty(),
            cache: DecodeCache::default(),
            animation_frame: DEFAULT_ANIMATION_FRAME,
        }
    }

    /// 用地图、图库与实体图库构造场景（无 `resource/data` 的测试可以把实体图库留空）。
    #[must_use]
    pub fn from_parts_with_entities(
        map: Map,
        libraries: TileLibraries,
        viewport: Viewport,
        entity_libraries: EntityLibraries,
    ) -> Self {
        Self {
            map,
            viewport,
            libraries,
            entities: EntityRenderer::new(entity_libraries),
            cache: DecodeCache::default(),
            animation_frame: DEFAULT_ANIMATION_FRAME,
        }
    }

    /// 当前视口（决定 `OffSetX/OffSetY`）。
    #[must_use]
    pub const fn viewport(&self) -> Viewport {
        self.viewport
    }

    /// 手工设置视口；下一次 [`MapScene::render`] 会按帧尺寸覆盖它。
    pub fn set_viewport(&mut self, viewport: Viewport) {
        self.viewport = viewport;
    }

    /// 图库集合。
    #[must_use]
    pub const fn libraries(&self) -> &TileLibraries {
        &self.libraries
    }

    /// 实体图库集合。
    #[must_use]
    pub fn entity_libraries(&self) -> &EntityLibraries {
        self.entities.libraries()
    }

    /// 实体图库集合（可变；换素材目录后可以重新设置）。
    pub fn entity_libraries_mut(&mut self) -> &mut EntityLibraries {
        self.entities.libraries_mut()
    }

    /// 已缓存的实体图像张数（含解码失败的空占位）。
    #[must_use]
    pub fn entity_image_cache_len(&self) -> usize {
        self.entities.cache_len()
    }

    /// 取一张物品图标（`items.wzl`，缺失时回退 `dnitems.wzl`）。
    ///
    /// 背包与商店面板用它画格子里的物品图；素材缺失时返回 `None`，调用方退化成
    /// 文字占位，不影响界面其余部分。
    #[must_use]
    pub fn item_icon(&mut self, image: u16) -> Option<Image> {
        self.entities.item_icon(image)
    }

    /// 当前动画计数。
    #[must_use]
    pub const fn animation_frame(&self) -> u32 {
        self.animation_frame
    }

    /// 设置动画计数（Crystal 的 `MapControl.AnimationCount`）。
    pub fn set_animation_frame(&mut self, frame: u32) {
        self.animation_frame = frame;
    }

    /// 换一张地图，保留图库与实体图库（客户端切图用）。
    ///
    /// Crystal 收到 `S.MapChanged` 后如果地图编号变了会重新 `MapControl.LoadMap()`
    /// （`Client/MirScenes/GameScene.cs:3974-3988`）。这里只重读地形与清空解码缓存：
    /// 瓦片/实体图库与素材目录无关，没必要跟着重读。
    ///
    /// # Errors
    /// 地图文件缺失或损坏时返回 [`RenderError`]，调用方应保留旧地图（画面不切成黑屏）。
    pub fn reload_map(&mut self, map_path: impl AsRef<Path>) -> Result<(), RenderError> {
        let map = Map::load(map_path.as_ref())?;
        self.map = map;
        // 旧地图的图像号在新地图里含义不同，缓存必须整体丢弃
        // （Crystal 每次 `LoadMap` 都会重建 `MapControl` 的纹理缓存）。
        self.cache = DecodeCache::default();
        Ok(())
    }

    /// 已缓存的图像张数（含解码失败的空占位）。
    #[must_use]
    pub fn image_cache_len(&self) -> usize {
        self.cache.images.len()
    }

    /// 地图坐标 → 屏幕坐标（格原点，鼠标拾取与角色绘制用）。
    ///
    /// 使用当前 [`MapScene::viewport`]；[`MapScene::render`] 结束后的视口与刚渲染的
    /// 帧尺寸一致。公式见 [`geometry::world_to_screen`]。
    #[must_use]
    pub fn world_to_screen(&self, center: Point, cell: Point) -> (i32, i32) {
        geometry::world_to_screen(self.viewport, center, cell)
    }

    /// 屏幕坐标 → 地图坐标，鼠标拾取用（`GameScene.cs:10321-10324`）。
    #[must_use]
    pub fn screen_to_cell(&self, center: Point, screen: Point) -> Point {
        geometry::screen_to_cell(self.viewport, center, screen)
    }

    /// 渲染以 `center` 为中心的一屏。
    ///
    /// 会把 [`MapScene::viewport`] 更新为帧尺寸，然后清屏、画铺地层、画对象层。
    /// 同样的 `(地图, 素材, center, 动画计数, 帧尺寸)` 得到逐字节相同的像素。
    pub fn render(&mut self, frame: &mut Frame, center: Point) {
        self.viewport = Viewport::new(frame.width, frame.height);
        frame.clear(BACKGROUND);
        let (columns, rows) = geometry::cell_bounds(self.viewport, center);
        self.render_floor(frame, center, &columns, &rows);
        self.render_objects(frame, center, &columns, &rows);
    }

    /// 画地形 + 实体（玩家、怪物、NPC、地面物品）。
    ///
    /// 等价于先 [`MapScene::render`] 再 [`MapScene::render_entities`]；实体在地形
    /// 对象层之后按地图 Y、X 排序绘制。素材缺失的实体画彩色占位方块。
    pub fn render_with_entities(
        &mut self,
        frame: &mut Frame,
        center: Point,
        entities: &[EntityView],
    ) {
        let now_ms = u64::from(self.animation_frame).saturating_mul(100);
        self.render_with_entities_at(frame, center, entities, now_ms);
    }

    /// 画地形 + 实体，并按真实时间（Unix 毫秒）推进实体的动作帧。
    ///
    /// 窗口循环用这个入口；它给 [`EntityView::action_started_ms`] 一个真实的时间基准，
    /// 因此走/跑/攻击/受击/死亡会按 Crystal 的 `Interval` 逐帧播放。
    pub fn render_with_entities_at(
        &mut self,
        frame: &mut Frame,
        center: Point,
        entities: &[EntityView],
        now_ms: u64,
    ) {
        self.render(frame, center);
        self.render_entities_at(frame, center, entities, now_ms);
    }

    /// 只画实体层（地形已经画好的情况下使用）。
    ///
    /// 使用当前的 [`MapScene::viewport`]；如果还没调用过 [`MapScene::render`]，
    /// 视口是 [`Viewport::DEFAULT`]。
    pub fn render_entities(&mut self, frame: &mut Frame, center: Point, entities: &[EntityView]) {
        let now_ms = u64::from(self.animation_frame).saturating_mul(100);
        self.render_entities_at(frame, center, entities, now_ms);
    }

    /// 只画实体层，并按真实时间推进动作帧。
    pub fn render_entities_at(
        &mut self,
        frame: &mut Frame,
        center: Point,
        entities: &[EntityView],
        now_ms: u64,
    ) {
        let viewport = self.viewport;
        self.entities
            .render_at(frame, viewport, center, entities, now_ms);
    }

    /// 取一张特效精灵（`effect.wzl` / `magic.wzl`），缺素材时返回 `None`。
    ///
    /// 特效由客户端表现层（`ui::spellfx`）按事件落点贴到地图格上；图库走
    /// [`EntityRenderer`] 的按需打开 + 解码缓存，不会在启动时读几 MB 的 `magic.wzl`。
    pub fn effect_image(&mut self, kind: crate::entities::FxKind, index: u32) -> Option<Image> {
        self.entities.effect_image(kind, index)
    }

    /// 铺地层（`GameScene.cs:10658-10764`）。
    fn render_floor(
        &mut self,
        frame: &mut Frame,
        center: Point,
        columns: &std::ops::RangeInclusive<i32>,
        rows: &std::ops::RangeInclusive<i32>,
    ) {
        for y in rows.clone() {
            for x in columns.clone() {
                let Some(cell) = self.map.cell(x, y).copied() else {
                    continue;
                };
                let cell_point = Point::new(x, y);
                let (tile_x, tile_y) = geometry::tile_origin(self.viewport, center, cell_point);
                self.draw_back_tile(frame, &cell, x, y, tile_x, tile_y);
                self.draw_middle_flat(frame, &cell, tile_x, tile_y);
                self.draw_front_flat(frame, &cell, tile_x, tile_y);
            }
        }
    }

    /// 对象层（`GameScene.cs:10798-10946` 的逐格部分）。
    fn render_objects(
        &mut self,
        frame: &mut Frame,
        center: Point,
        columns: &std::ops::RangeInclusive<i32>,
        rows: &std::ops::RangeInclusive<i32>,
    ) {
        for y in rows.clone() {
            for x in columns.clone() {
                let Some(cell) = self.map.cell(x, y).copied() else {
                    continue;
                };
                let cell_point = Point::new(x, y);
                let (sprite_x, sprite_bottom) =
                    geometry::sprite_anchor(self.viewport, center, cell_point);
                self.draw_middle_raised(frame, &cell, sprite_x, sprite_bottom);
                self.draw_front_raised(frame, &cell, sprite_x, sprite_bottom);
            }
        }
    }

    /// 后层瓦片：只在偶数 x/y 上画（`GameScene.cs:10703-10711`）。
    fn draw_back_tile(
        &mut self,
        frame: &mut Frame,
        cell: &MapCell,
        x: i32,
        y: i32,
        tile_x: i32,
        tile_y: i32,
    ) {
        if x.rem_euclid(2) != 0 || y.rem_euclid(2) != 0 {
            return;
        }
        let Some(library) = self.back_library() else {
            return;
        };
        // `(BackImage & 0x1FFFFFFF) - 1`（:10707）；bit15 在客户端被提升为
        // 0x20000000（MapCode.cs:254-255），掩码后等价于 `raw & 0x7FFF`。
        let image = u32::from(cell.back_image & 0x7FFF);
        if image == 0 {
            return;
        }
        self.blit_tile(frame, library, image - 1, tile_x, tile_y);
    }

    /// 中层里「格尺寸」的图像，画在瓦片左上角（`GameScene.cs:10713-10724`）。
    fn draw_middle_flat(&mut self, frame: &mut Frame, cell: &MapCell, tile_x: i32, tile_y: i32) {
        let Some(index) = image_index(cell.middle_image) else {
            return;
        };
        let Some(library) = self.middle_library() else {
            return;
        };
        let Some(info) = self.image_info(library, index) else {
            return;
        };
        if is_cell_sized(info) {
            self.blit_tile(frame, library, index, tile_x, tile_y);
        }
    }

    /// 前层里「格尺寸」的图像，画在瓦片左上角（`GameScene.cs:10726-10758`）。
    fn draw_front_flat(&mut self, frame: &mut Frame, cell: &MapCell, tile_x: i32, tile_y: i32) {
        let Some(index) = image_index(cell.front_image & 0x7FFF) else {
            return;
        };
        let library = self.front_library(cell);
        // Crystal 跳过 200 号库（Mir3 的 Tilesc），`:10731`。
        if library == 200 {
            return;
        }
        let Some(info) = self.image_info(library, index) else {
            return;
        };
        // 门的帧号调整（`:10737-10749`）需要服务端的门状态；静态帧按门关闭处理。
        if is_cell_sized(info) {
            self.blit_tile(frame, library, index, tile_x, tile_y);
        }
    }

    /// 中层里「非格尺寸」的图像，以格子底边为锚点向上画（`GameScene.cs:10839-10874`）。
    fn draw_middle_raised(
        &mut self,
        frame: &mut Frame,
        cell: &MapCell,
        sprite_x: i32,
        sprite_bottom: i32,
    ) {
        let Some(index) = image_index(cell.middle_image) else {
            return;
        };
        // Crystal 这一支要求 `index > 0`，中层第 0 帧只在铺地层画（`:10842`）。
        if index == 0 {
            return;
        }
        let Some(library) = self.middle_library() else {
            return;
        };
        // v0 的客户端不读中层的动画字节（MapCode.cs:239-253），所以这里只有静态帧。
        let Some(info) = self.image_info(library, index) else {
            return;
        };
        if is_cell_sized(info) {
            return;
        }
        self.blit_tile(
            frame,
            library,
            index,
            sprite_x,
            sprite_bottom - i32::from(info.height),
        );
    }

    /// 前层里「非格尺寸」的图像（树、墙、灯），按动画帧与混合标志绘制
    /// （`GameScene.cs:10877-10937`）。
    fn draw_front_raised(
        &mut self,
        frame: &mut Frame,
        cell: &MapCell,
        sprite_x: i32,
        sprite_bottom: i32,
    ) {
        let Some(base_index) = image_index(cell.front_image & 0x7FFF) else {
            return;
        };
        let library = self.front_library(cell);
        // 客户端把 `o+8` 读成 FrontAnimationFrame（MapCode.cs:250），
        // mir-world 把同一个字节按服务器语义存成 `door_raw`（Map.cs:142）。
        let mut animation = u16::from(cell.door_raw);
        let blend = animation & 0x80 != 0;
        animation &= 0x7F;
        let mut index = base_index;
        if animation > 0 {
            let tick = u32::from(cell.front_animation_tick);
            let animation = u32::from(animation);
            let period = animation + animation * tick;
            index += (self.animation_frame % period) / (1 + tick);
        }
        let Some(info) = self.image_info(library, index) else {
            return;
        };
        if is_cell_sized(info) && animation == 0 {
            // 已在铺地层画过（`:10920-10921`）。
            return;
        }

        if blend {
            if library == 14 || library == 27 || (100..199).contains(&library) {
                // 这几类库的混合前层按「三格高」定位并应用图像自带偏移（`:10925-10926`）。
                self.blit_tile_add(
                    frame,
                    library,
                    index,
                    sprite_x + i32::from(info.offset_x),
                    sprite_bottom - 3 * CELL_HEIGHT + i32::from(info.offset_y),
                );
            } else {
                // 2723..=2732 是灯光类图像，Crystal 对它们应用图像偏移（`:10928`）。
                let (offset_x, offset_y) = if (2723..=2732).contains(&index) {
                    (info.offset_x, info.offset_y)
                } else {
                    (0, 0)
                };
                self.blit_tile_add(
                    frame,
                    library,
                    index,
                    sprite_x + i32::from(offset_x),
                    sprite_bottom - i32::from(info.height) + i32::from(offset_y),
                );
            }
        } else if library == 28 && (info.offset_x != 0 || info.offset_y != 0) {
            // 28 号库是特例：画在「下一格」并按图像偏移定位（`:10932-10933`）。
            self.blit_tile(
                frame,
                library,
                index,
                sprite_x + i32::from(info.offset_x),
                sprite_bottom - CELL_HEIGHT + i32::from(info.offset_y),
            );
        } else {
            // 普通高精灵：底边对齐格子锚点，忽略图像自带偏移（`:10935`）。
            self.blit_tile(
                frame,
                library,
                index,
                sprite_x,
                sprite_bottom - i32::from(info.height),
            );
        }
    }

    /// 后层图库编号：v0/v4 固定为 0（Tiles，`MapCode.cs:240`、`MapCode.cs:430`）。
    ///
    /// 其它版本的后层编号来自格内字节（v2/v3 是 `o+12 + 100`），`mir-world` 尚未暴露，
    /// 因此这里返回 `None`，宁可不画也不画错库。
    fn back_library(&self) -> Option<u16> {
        matches!(self.map.version, MapVersion::V0 | MapVersion::V4).then_some(0)
    }

    /// 中层图库编号：v0/v4 固定为 1（SmTiles，`MapCode.cs:241`、`MapCode.cs:431`）。
    fn middle_library(&self) -> Option<u16> {
        matches!(self.map.version, MapVersion::V0 | MapVersion::V4).then_some(1)
    }

    /// 前层图库编号 = 格内 `o+10` 字节 + 版本偏移（`MapCode.cs:252`、`:347`、`:389`、`:442`）。
    fn front_library(&self, cell: &MapCell) -> u16 {
        let base = match self.map.version {
            MapVersion::V2 | MapVersion::V3 => 120,
            _ => 2,
        };
        u16::from(cell.front_index) + base
    }

    /// 取图像头信息（带缓存）。
    fn image_info(&mut self, library: u16, index: u32) -> Option<ImageInfo> {
        let key = (library, index);
        if let Some(info) = self.cache.infos.get(&key) {
            return *info;
        }
        let info = self.libraries.info(library, index);
        self.cache.infos.insert(key, info);
        info
    }

    /// 把一张图贴到帧上（带解码缓存与纯黑透明处理）。
    fn blit_tile(&mut self, frame: &mut Frame, library: u16, index: u32, x: i32, y: i32) {
        self.blit_tile_mode(frame, library, index, x, y, false);
    }

    /// 把一张图以加法混合贴到帧上（Crystal 的 `DrawBlend`）。
    fn blit_tile_add(&mut self, frame: &mut Frame, library: u16, index: u32, x: i32, y: i32) {
        self.blit_tile_mode(frame, library, index, x, y, true);
    }

    /// blit 的公共实现：`additive` 为真时走加法混合。
    fn blit_tile_mode(
        &mut self,
        frame: &mut Frame,
        library: u16,
        index: u32,
        x: i32,
        y: i32,
        additive: bool,
    ) {
        let key = (library, index);
        if !self.cache.images.contains_key(&key) {
            if self.cache.images.len() >= MAX_CACHED_IMAGES {
                self.cache.images.clear();
            }
            let image = self.libraries.image(library, index).map(|mut image| {
                remove_black(&mut image);
                image
            });
            self.cache.images.insert(key, image);
        }
        if let Some(Some(image)) = self.cache.images.get(&key) {
            if additive {
                frame.blit_add(image, x, y);
            } else {
                frame.blit(image, x, y);
            }
        }
    }
}

/// 图像号转下标：`0` 表示「没有这张图」，其它值减 1（`GameScene.cs:10707`）。
fn image_index(raw: u16) -> Option<u32> {
    raw.checked_sub(1).map(u32::from)
}

/// Crystal 只把 1×1 格或 2×2 格大小的中层/前层画进铺地层
/// （`GameScene.cs:10719-10720`、`:10752-10753`）。
fn is_cell_sized(info: ImageInfo) -> bool {
    (i32::from(info.width) == CELL_WIDTH && i32::from(info.height) == CELL_HEIGHT)
        || (i32::from(info.width) == CELL_WIDTH * 2 && i32::from(info.height) == CELL_HEIGHT * 2)
}

/// 把调色板图像里的纯黑变成透明。
///
/// `resource/data/*.wzl` 的 8 位调色板 alpha 全是 255（`mir-asset` 的 crate 文档），
/// 而地图图像以纯黑表示空像素；Crystal 的 `.Lib` 在写库时已经做过同样的处理
/// （`LibraryEditor/Graphics/MLibraryV2.cs:419-426`：`R=G=B=0` 时 alpha 置 0）。
/// 只对调色板格式生效，带真实 alpha 的 `.Lib`/DXT1 图像不受影响。
pub(crate) fn remove_black(image: &mut Image) {
    if !matches!(image.info.format, format::EMPTY | format::PALETTE8) {
        return;
    }
    remove_black_all(image);
}

/// 把纯黑变成透明，**不限制格式**。
///
/// 坐骑/翅膀的扁平素材是 RGB565（`.wzl` 没有 alpha），而 Crystal 的 `.Lib`/DXT1
/// 在转换时已经做过 `removeBlack`（`MLibraryV2.cs:419-426`）。所以这两层不能只靠
/// [`remove_black`] 的调色板分支，否则会在角色周围留下黑框。
pub(crate) fn remove_black_all(image: &mut Image) {
    for pixel in image.pixels.chunks_exact_mut(4) {
        if pixel[0] == 0 && pixel[1] == 0 && pixel[2] == 0 {
            pixel[3] = 0;
        }
    }
}

/// 解码缓存：头信息与像素分开存。
#[derive(Debug, Default)]
struct DecodeCache {
    /// `(图库编号, 图像下标) → 头信息`；`None` 表示库/图不存在或为空。
    infos: HashMap<(u16, u32), Option<ImageInfo>>,
    /// `(图库编号, 图像下标) → 解码结果`；`None` 表示缺图或格式不支持。
    images: HashMap<(u16, u32), Option<Image>>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use mir_world::MapCell;

    fn empty_map(width: i32, height: i32) -> Map {
        Map {
            width,
            height,
            cells: vec![MapCell::default(); usize::try_from(width * height).unwrap_or(0)],
            file_name: "synthetic".to_owned(),
            version: MapVersion::V0,
            doors: Vec::new(),
        }
    }

    fn empty_scene(width: i32, height: i32) -> MapScene {
        MapScene::from_parts(
            empty_map(width, height),
            TileLibraries::empty("."),
            Viewport::DEFAULT,
        )
    }

    /// 按 `(x, y)` 修改合成地图的一个格子（内部是列优先存放）。
    fn set_cell(map: &mut Map, x: i32, y: i32, edit: impl FnOnce(&mut MapCell)) {
        let height = map.height;
        let index = usize::try_from(x * height + y).expect("格子下标");
        edit(&mut map.cells[index]);
    }

    #[test]
    fn image_index_skips_zero() {
        assert_eq!(image_index(0), None);
        assert_eq!(image_index(1), Some(0));
        assert_eq!(image_index(65_535), Some(65_534));
    }

    #[test]
    fn only_two_by_two_or_cell_sized_are_flat() {
        let info = |width, height| ImageInfo {
            width,
            height,
            offset_x: 0,
            offset_y: 0,
            format: format::PALETTE8,
            shadow_x: 0,
            shadow_y: 0,
            reserved: [0; 3],
            has_mask: false,
            data_offset: 0,
            data_len: 0,
        };
        assert!(is_cell_sized(info(48, 32)));
        assert!(is_cell_sized(info(96, 64)));
        assert!(!is_cell_sized(info(48, 33)));
        assert!(!is_cell_sized(info(100, 100)));
        assert!(!is_cell_sized(info(0, 0)));
    }

    #[test]
    fn remove_black_only_touches_palette_images() {
        let mut image = Image {
            info: ImageInfo {
                width: 2,
                height: 1,
                offset_x: 0,
                offset_y: 0,
                format: format::PALETTE8,
                shadow_x: 0,
                shadow_y: 0,
                reserved: [0; 3],
                has_mask: false,
                data_offset: 0,
                data_len: 0,
            },
            pixels: vec![0, 0, 0, 255, 1, 2, 3, 255],
        };
        remove_black(&mut image);
        assert_eq!(&image.pixels[0..4], &[0, 0, 0, 0]);
        assert_eq!(&image.pixels[4..8], &[1, 2, 3, 255]);

        // RGBA32（旧 .Lib）已经有正确的 alpha，不做纯黑处理。
        let mut rgba = image.clone();
        rgba.info.format = format::RGBA32_GZIP;
        rgba.pixels.copy_from_slice(&[0, 0, 0, 255, 0, 0, 0, 128]);
        remove_black(&mut rgba);
        assert_eq!(&rgba.pixels[0..4], &[0, 0, 0, 255]);
        assert_eq!(&rgba.pixels[4..8], &[0, 0, 0, 128]);
    }

    #[test]
    fn remove_black_all_also_touches_rgb565_mount_and_wing_frames() {
        let mut image = Image {
            info: ImageInfo {
                width: 2,
                height: 1,
                offset_x: 0,
                offset_y: 0,
                format: format::RGB565,
                shadow_x: 0,
                shadow_y: 0,
                reserved: [0; 3],
                has_mask: false,
                data_offset: 0,
                data_len: 0,
            },
            pixels: vec![0, 0, 0, 255, 7, 8, 9, 255],
        };
        remove_black_all(&mut image);
        assert_eq!(&image.pixels[0..4], &[0, 0, 0, 0]);
        assert_eq!(&image.pixels[4..8], &[7, 8, 9, 255]);
    }

    #[test]
    fn render_with_entities_draws_placeholders_without_assets() {
        use mir_core::enums::MirDirection;

        use crate::entities::{EntityKind, EntityView};

        let mut scene = empty_scene(8, 8);
        let mut frame = Frame::new(200, 200);
        frame.clear([9, 9, 9, 255]);
        let entities = [
            EntityView {
                name: Some("自己".to_owned()),
                ..EntityView::new(Point::new(4, 4), MirDirection::Down, EntityKind::Player, 0)
            },
            EntityView {
                name: Some("稻草人".to_owned()),
                hp_percent: Some(50),
                ..EntityView::new(
                    Point::new(5, 5),
                    MirDirection::Down,
                    EntityKind::Monster,
                    27,
                )
            },
        ];
        scene.render_with_entities(&mut frame, Point::new(4, 4), &entities);
        assert!(
            frame.distinct_color_count() >= 3,
            "占位方块与血条至少带来 3 种颜色"
        );
        assert_eq!(scene.entity_image_cache_len(), 0, "没有实体图库");
    }

    #[test]
    fn render_without_libraries_clears_to_background() {
        let mut scene = empty_scene(8, 8);
        let mut frame = Frame::new(64, 64);
        frame.clear([9, 9, 9, 255]);
        scene.render(&mut frame, Point::new(4, 4));
        assert!(
            frame
                .pixels
                .chunks_exact(4)
                .all(|pixel| pixel == BACKGROUND)
        );
    }

    #[test]
    fn render_updates_viewport_and_world_to_screen() {
        let mut scene = empty_scene(16, 16);
        let mut frame = Frame::new(320, 240);
        let center = Point::new(5, 6);
        scene.render(&mut frame, center);
        assert_eq!(scene.viewport(), Viewport::new(320, 240));
        // OffSetX = 160/48 = 3，OffSetY = 120/32 - 1 = 2。
        assert_eq!(
            scene.world_to_screen(center, center),
            (3 * CELL_WIDTH, 2 * CELL_HEIGHT)
        );
        assert_eq!(
            scene.world_to_screen(center, Point::new(6, 7)),
            (3 * CELL_WIDTH + CELL_WIDTH, 2 * CELL_HEIGHT + CELL_HEIGHT)
        );
    }

    #[test]
    fn animation_frame_is_configurable() {
        let mut scene = empty_scene(4, 4);
        assert_eq!(scene.animation_frame(), 0);
        scene.set_animation_frame(7);
        assert_eq!(scene.animation_frame(), 7);
        assert_eq!(scene.image_cache_len(), 0);
    }

    #[test]
    fn back_and_middle_libraries_follow_map_version() {
        let mut scene = empty_scene(2, 2);
        assert_eq!(scene.back_library(), Some(0));
        assert_eq!(scene.middle_library(), Some(1));
        let cell = MapCell {
            front_index: 5,
            ..MapCell::default()
        };
        assert_eq!(scene.front_library(&cell), 7); // 5 + 2
        scene.map.version = MapVersion::V2;
        assert_eq!(scene.back_library(), None);
        assert_eq!(scene.middle_library(), None);
        assert_eq!(scene.front_library(&cell), 125); // 5 + 120
    }

    /// 合成 `.wzl`/`.wzx`：每张图都是纯色 RGB565。
    ///
    /// 这样可以在没有 `resource/` 的 CI 上验证整条绘制链路：库号映射 → 偶数格后层 →
    /// 格尺寸中层/前层 → 高精灵底边锚点 → 前层动画帧。
    fn zlib(data: &[u8]) -> Vec<u8> {
        use std::io::Write as _;

        let mut encoder =
            flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
        encoder.write_all(data).expect("压缩测试数据");
        encoder.finish().expect("收尾 zlib")
    }

    fn wzl_record(width: u16, height: u16, color: u16) -> Vec<u8> {
        let pixels = usize::from(width) * usize::from(height);
        let mut payload = Vec::with_capacity(pixels * 2);
        for _ in 0..pixels {
            payload.extend_from_slice(&color.to_le_bytes());
        }
        let compressed = zlib(&payload);
        let mut record = vec![5u8, 0, 0, 0]; // 格式 5 = RGB565
        record.extend_from_slice(&width.to_le_bytes());
        record.extend_from_slice(&height.to_le_bytes());
        record.extend_from_slice(&0i16.to_le_bytes()); // offset_x
        record.extend_from_slice(&0i16.to_le_bytes()); // offset_y
        record.extend_from_slice(
            &u32::try_from(compressed.len())
                .expect("压缩长度")
                .to_le_bytes(),
        );
        record.extend_from_slice(&compressed);
        record
    }

    fn write_wzl(directory: &Path, stem: &str, records: &[Vec<u8>]) {
        let mut wzl = vec![0u8; 48];
        let mut offsets = Vec::new();
        for record in records {
            offsets.push(u32::try_from(wzl.len()).expect("偏移"));
            wzl.extend_from_slice(record);
        }
        let mut wzx = vec![0u8; 48];
        wzx[44..48].copy_from_slice(&u32::try_from(offsets.len()).expect("数量").to_le_bytes());
        for offset in offsets {
            wzx.extend_from_slice(&offset.to_le_bytes());
        }
        std::fs::write(directory.join(format!("{stem}.wzl")), wzl).expect("写 .wzl");
        std::fs::write(directory.join(format!("{stem}.wzx")), wzx).expect("写 .wzx");
    }

    #[test]
    fn synthetic_scene_draws_layers_in_crystal_order() {
        let directory =
            std::env::temp_dir().join(format!("mir-render-synthetic-scene-{}", std::process::id()));
        std::fs::create_dir_all(&directory).expect("创建临时目录");
        write_wzl(
            &directory,
            "tiles",
            &[
                wzl_record(96, 64, 0xF800), // 索引 0：红
                wzl_record(96, 64, 0x07E0), // 索引 1：绿
            ],
        );
        write_wzl(
            &directory,
            "smtiles",
            &[wzl_record(48, 32, 0x001F)], // 索引 0：蓝
        );
        write_wzl(
            &directory,
            "objects",
            &[
                wzl_record(48, 96, 0xFFE0), // 索引 0：黄（高精灵）
                wzl_record(48, 96, 0x07FF), // 索引 1：青（动画第二帧）
                wzl_record(48, 32, 0xF81F), // 索引 2：品红（格尺寸前层）
            ],
        );

        let mut map = empty_map(3, 3);
        // 后层：红在 (0,0)，绿在 (2,2)；奇数格的绿必须被忽略。
        set_cell(&mut map, 0, 0, |cell| cell.back_image = 1);
        set_cell(&mut map, 2, 2, |cell| cell.back_image = 2);
        set_cell(&mut map, 1, 0, |cell| cell.back_image = 2);
        // 中层：蓝在 (1,1)。
        set_cell(&mut map, 1, 1, |cell| cell.middle_image = 1);
        // 前层：黄色高精灵在 (2,1)；格尺寸品红在 (2,2)；青色动画在 (0,2)。
        set_cell(&mut map, 2, 1, |cell| cell.front_image = 1);
        set_cell(&mut map, 2, 2, |cell| cell.front_image = 3);
        set_cell(&mut map, 0, 2, |cell| {
            cell.front_image = 1;
            cell.door_raw = 2; // 客户端语义：FrontAnimationFrame = o+8
            cell.front_animation_tick = 0;
        });

        let libraries = TileLibraries::open_dir(&directory).expect("打开合成图库");
        assert_eq!(libraries.len(), 3);
        let mut scene = MapScene::from_parts(map, libraries, Viewport::new(200, 200));
        let center = Point::new(1, 1);
        // OffSetX = 200/2/48 = 2，OffSetY = 200/2/32 - 1 = 2。
        assert_eq!(scene.world_to_screen(center, center), (96, 64));

        let mut frame = Frame::new(200, 200);
        scene.render(&mut frame, center);
        let red = [248, 0, 0, 255];
        let green = [0, 252, 0, 255];
        let blue = [0, 0, 248, 255];
        let yellow = [248, 252, 0, 255];
        let magenta = [248, 0, 248, 255];
        let cyan = [0, 252, 248, 255];

        // 偶数格后层：红色瓦片在 (46,32)，奇数格 (1,0) 的绿色不画。
        assert_eq!(frame.pixel(100, 40), Some(red));
        // 格尺寸中层画在后层之上。
        assert_eq!(frame.pixel(100, 70), Some(blue));
        // (2,2) 的格尺寸前层盖住绿色后层。
        assert_eq!(frame.pixel(150, 100), Some(magenta));
        // 后层绿色在品红之外仍然可见。
        assert_eq!(frame.pixel(180, 150), Some(green));
        // 高精灵以底边为锚点：(2,1) 的黄色占据 x 142..190、y 0..96。
        assert_eq!(frame.pixel(160, 40), Some(yellow));
        // 动画帧 0：青色格显示基础帧（黄）。
        assert_eq!(frame.pixel(60, 110), Some(yellow));

        // 动画帧 1：青色格换成库里的第二帧。
        scene.set_animation_frame(1);
        scene.render(&mut frame, center);
        assert_eq!(frame.pixel(60, 110), Some(cyan));
        assert_eq!(frame.pixel(160, 40), Some(yellow));
        assert_eq!(scene.image_cache_len(), 6);

        std::fs::remove_dir_all(&directory).ok();
    }
}
