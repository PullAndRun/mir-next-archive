//! 实体（玩家、怪物、NPC、地面物品）的软件绘制。
//!
//! 地图本身由 [`crate::MapScene`] 画完后，本模块在同一个[`Frame`]上追加实体精灵。
//! 所有精灵都从 `resource/data` 的平面 `.wzl` 图库里按需解码，并按水晶（Crystal）
//! 客户端同样的等距锚点定位；素材缺失时画一个彩色占位方块，保证无素材的 CI 也能
//! 得到非空画面。
//!
//! # 精灵库映射
//!
//! Crystal 的客户端按对象的 `Race` 选择图库（`Client/MirObjects/PlayerObject.cs:553-561`、
//! `MonsterObject.cs:208`、`NPCObject.cs:77-80`、`ItemObject.cs:32,49`）。本项目的素材
//! 语料是**扁平**的 `resource/data/*.wzl`（平铺的多外观库），因此这里用文件名直接映射：
//!
//! | 实体 | 图库文件 | 图像号规则 | Crystal 出处 |
//! |---|---|---|---|
//! | 玩家 | `hum.wzl` | 外观 `appearance` 占 600 帧；站立帧 `appearance*600 + direction*8 + n` | 外观选择 `PlayerObject.cs:553`；帧号 `PlayerObject.cs:761`；站立帧集合 `Frames.cs:157` |
//! | 怪物 | `mon{image/10+1}.wzl` | `image` 每 10 个一组对应一个 `Mon1..MonN.wzl`；每个图像占 430 帧 | 图像 → 图库 `MonsterObject.cs:208`；默认帧集合 `Frames.cs:25-31` |
//! | NPC | `npc.wzl`、`npc2.wzl`… | `image` 每 100 个换一个库；每个图像占 60 帧 | `NPCObject.cs:77-80`；默认 NPC 帧集合 `Frames.cs:18-19` |
//! | 物品 | `items.wzl`（缺省回退 `dnitems.wzl`） | 图像号就是物品的 `Image` | `ItemObject.cs:32,37,49` |
//!
//! 关于 430/600/60 这几个步长：Crystal 的 `.Lib` 是「一个外观一个库」，帧起点由
//! `Frame.Start` 给出（`Frames.cs:157-171`）；扁平 `.wzl` 把多个外观塞进一个文件，
//! 所以需要步长才能定位到第 N 个外观。这些步长来自本素材语料（盛大 Mir2 扁平包）的
//! 实际布局：`hum.wzx` 有 14400 帧 = 24×600，`npc.wzx` 有 4900 帧 = 81×60 + 40。
//! 怪物每个图像 430 帧是经典 Mir2 的布局，但**并非所有库都恰好这么多**（`mon1.wzx`
//! 只有 1086 帧）；因此绘制时按「精确帧 → 该图像的站立帧 → 库首帧」逐级回退，
//! 实在没有素材就画占位方块。`!Mon.dat` 里存有每只怪的精确动作帧表，本模块尚未解析。
//!
//! # 锚点
//!
//! * 玩家/怪物/NPC：与 Crystal 一样以**格原点**为锚点，再叠加图像头里的
//!   `offset_x/offset_y`（`MLibrary.Draw(..., offSet: true)`，`MLibrary.cs:658-665`；
//!   角色 `BodyLibrary.Draw(..., DrawLocation, ..., true)`，`PlayerObject.cs:5045`）。
//! * 物品：Crystal 把物品在格子里居中（`ItemObject.cs:78-79`），不叠加图像偏移
//!   （`ItemObject.cs:72-73` 的 `Draw` 使用默认 `offSet: false`）。
//!
//! 实体按地图 Y 升序、同一行按 X 升序绘制（与 `GameScene.cs:10798-11008` 的对象
//! 循环顺序一致），靠下的对象覆盖靠上的对象。
//!
//! # 与 Crystal 的差异
//!
//! * 不画头发、武器、坐骑、翅膀、光效、中毒/等级特效（这些需要额外的外观字段，
//!   `EntityView` 只带一个 `image`）。
//! * 不画名字文字；`name` 字段保留给调试叠层/以后加字体时使用。
//! * 不做行走/跑步的逐帧插值与动画，只按 `animation_frame` 播放站立帧。

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use mir_asset::{Image, ImageLibrary};
use mir_core::Point;
use mir_core::enums::MirDirection;

use crate::Frame;
use crate::geometry::{self, CELL_HEIGHT, CELL_WIDTH, Viewport};
use crate::scene::remove_black;

/// 玩家每个外观占的帧数（`hum.wzx` 的 14400 帧 / 24 个外观）。
pub const PLAYER_FRAMES_PER_APPEARANCE: u32 = 600;
/// 玩家每个方向占的帧数：4 帧站立 + 4 帧空位（本素材语料的布局）。
pub const PLAYER_DIRECTION_STRIDE: u32 = 8;
/// 怪物每个图像占的帧数（经典 Mir2 布局）。
pub const MONSTER_FRAMES_PER_IMAGE: u32 = 430;
/// 怪物每个方向占的帧数：4 帧站立 + 4 帧空位。
pub const MONSTER_DIRECTION_STRIDE: u32 = 8;
/// NPC 每个图像占的帧数。
pub const NPC_FRAMES_PER_IMAGE: u32 = 60;
/// 实体解码缓存上限（张）。一张 64×80 的 RGBA 图约 20 KiB。
pub const MAX_ENTITY_CACHED_IMAGES: usize = 512;

/// 实体类型。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum EntityKind {
    /// 玩家（自己或其他人）。
    Player,
    /// 怪物。
    Monster,
    /// NPC。
    Npc,
    /// 地面物品（含金币）。
    Item,
}

impl EntityKind {
    /// 占位方块的颜色（正常状态）。
    #[must_use]
    pub const fn color(self) -> [u8; 4] {
        match self {
            Self::Player => [96, 148, 255, 255],
            Self::Monster => [224, 72, 72, 255],
            Self::Npc => [96, 208, 112, 255],
            Self::Item => [240, 208, 72, 255],
        }
    }

    /// 占位方块的颜色（死亡状态）。
    #[must_use]
    pub const fn dead_color(self) -> [u8; 4] {
        match self {
            Self::Player | Self::Monster => [110, 110, 110, 255],
            Self::Npc | Self::Item => self.color(),
        }
    }
}

/// 画一个实体所需的全部信息。
///
/// 这是 `mir-render` 与客户端网络快照之间的唯一接口：网络层把 `ServerPacket`
/// 转成这个结构，渲染层不依赖 `mir-protocol`。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EntityView {
    /// 地图坐标。
    pub position: Point,
    /// 朝向（0=上，顺时针）。
    pub direction: MirDirection,
    /// 实体类型。
    pub kind: EntityKind,
    /// 图像号：玩家是外观（盔甲）编号，怪物是 `Monster` 枚举值，NPC 是 `Image`，
    /// 物品是物品的 `Image`。
    pub image: u16,
    /// 名字（本里程碑不画文字，保留给调试与后续 UI）。
    pub name: Option<String>,
    /// 是否死亡。
    pub dead: bool,
    /// 生命百分比（0..=100）；`None` 表示不画血条。
    pub hp_percent: Option<u8>,
}

/// 精灵图库标识：决定去哪个 `.wzl` 找图。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
enum SpriteLibrary {
    /// `hum.wzl`。
    Hum,
    /// `npc.wzl` / `npc2.wzl` / `npc3.wzl` / `npc4.wzl`。
    Npc(u8),
    /// `mon1.wzl` … `monN.wzl`。
    Monster(u16),
    /// `items.wzl`（找不到时回退 `dnitems.wzl`）。
    Item,
}

impl SpriteLibrary {
    /// 图库的主文件名（不含扩展名）。
    fn stem(self) -> String {
        match self {
            Self::Hum => "hum".to_owned(),
            Self::Npc(1) => "npc".to_owned(),
            Self::Npc(index) => format!("npc{index}"),
            Self::Monster(index) => format!("mon{index}"),
            Self::Item => "items".to_owned(),
        }
    }
}

/// 实体图库集合：按需打开 `resource/data` 里的 `.wzl`，只缓存打开过的库。
///
/// 与 [`crate::TileLibraries`] 不同，这里不预先打开任何文件：`resource/data` 里有
/// 800 多个 `.wzl`，全部打开会浪费句柄；实体只用得到 `hum`、`npc*`、`mon*`、`items`。
#[derive(Debug, Default)]
pub struct EntityLibraries {
    /// 素材目录（诊断用）。
    directory: PathBuf,
    /// 小写文件名（不含扩展名）→ 路径。
    files: BTreeMap<String, PathBuf>,
    /// 已尝试打开的图库；`None` 表示文件不存在或打开失败。
    opened: BTreeMap<String, Option<ImageLibrary>>,
}

impl EntityLibraries {
    /// 扫描目录，建立文件名索引（不打开任何图像库）。
    ///
    /// 目录不存在时返回空集合（地图渲染仍然可用，实体退化成占位方块）。
    #[must_use]
    pub fn open_dir(directory: impl AsRef<Path>) -> Self {
        let directory = directory.as_ref().to_path_buf();
        let mut files = BTreeMap::new();
        if let Ok(entries) = std::fs::read_dir(&directory) {
            for entry in entries.flatten() {
                let path = entry.path();
                let Some(extension) = path.extension() else {
                    continue;
                };
                let extension = extension.to_string_lossy().to_ascii_lowercase();
                if extension != "wzl" && extension != "lib" {
                    continue;
                }
                let Some(stem) = path.file_stem() else {
                    continue;
                };
                files.insert(stem.to_string_lossy().to_ascii_lowercase(), path);
            }
        }
        Self {
            directory,
            files,
            opened: BTreeMap::new(),
        }
    }

    /// 空图库集合（测试与无素材环境）。
    #[must_use]
    pub fn empty(directory: impl Into<PathBuf>) -> Self {
        Self {
            directory: directory.into(),
            files: BTreeMap::new(),
            opened: BTreeMap::new(),
        }
    }

    /// 扫描时使用的素材目录。
    #[must_use]
    pub fn directory(&self) -> &Path {
        &self.directory
    }

    /// 目录里找到了多少个可用的图像库文件（不要求已打开）。
    #[must_use]
    pub fn file_count(&self) -> usize {
        self.files.len()
    }

    /// 已尝试打开（成功或失败）的图库数量。
    #[must_use]
    pub fn opened_count(&self) -> usize {
        self.opened.len()
    }

    /// 取一个图库；文件不存在或打开失败返回 `None`。
    fn library(&mut self, stem: &str) -> Option<&ImageLibrary> {
        if !self.opened.contains_key(stem) {
            // 物品图库 `items` 缺失时回退到 `dnitems`：两者都是 Crystal
            // `Libraries.FloorItems` 在本项目扁平语料里的对应物，帧号一致。
            let fallback = if stem == "items" {
                self.files.get("dnitems")
            } else {
                None
            };
            let opened =
                self.files
                    .get(stem)
                    .or(fallback)
                    .and_then(|path| match ImageLibrary::open(path) {
                        Ok(library) => Some(library),
                        Err(error) => {
                            eprintln!("警告：实体图库 {} 打开失败：{error}", path.display());
                            None
                        }
                    });
            self.opened.insert(stem.to_owned(), opened);
        }
        self.opened.get(stem).and_then(Option::as_ref)
    }

    /// 解码单张图。
    fn image(&mut self, library: SpriteLibrary, index: u32) -> Option<Image> {
        let index = usize::try_from(index).ok()?;
        self.library(&library.stem())?
            .image(index)
            .ok()
            .filter(|image| !image.is_empty())
    }
}

/// 实体绘制器：图库 + 解码缓存。
#[derive(Debug, Default)]
pub struct EntityRenderer {
    /// 实体图库。
    libraries: EntityLibraries,
    /// `(图库, 帧号) → 解码结果`；`None` 表示该帧不存在或解码失败。
    cache: BTreeMap<(SpriteLibrary, u32), Option<Image>>,
}

impl EntityRenderer {
    /// 用已有图库构造绘制器。
    #[must_use]
    pub fn new(libraries: EntityLibraries) -> Self {
        Self {
            libraries,
            cache: BTreeMap::new(),
        }
    }

    /// 空绘制器（所有实体都画占位方块）。
    #[must_use]
    pub fn empty() -> Self {
        Self::default()
    }

    /// 当前图库集合。
    #[must_use]
    pub fn libraries(&self) -> &EntityLibraries {
        &self.libraries
    }

    /// 当前图库集合（可变）。
    pub fn libraries_mut(&mut self) -> &mut EntityLibraries {
        &mut self.libraries
    }

    /// 已成功解码并缓存的图像张数（缺图/解码失败的占位不计）。
    #[must_use]
    pub fn cache_len(&self) -> usize {
        self.cache.values().filter(|image| image.is_some()).count()
    }

    /// 在已经画好地形的帧上追加实体。
    ///
    /// `animation_frame` 对应 Crystal 的 `MapControl.AnimationCount`
    /// （`GameScene.cs:1188-1193`），只用于站立帧的循环；`0` 得到确定性的静态帧。
    pub fn render(
        &mut self,
        frame: &mut Frame,
        viewport: Viewport,
        center: Point,
        entities: &[EntityView],
        animation_frame: u32,
    ) {
        // 按 Y 升序、同一行按 X 升序；`sort_by_key` 是稳定排序，同格实体保持输入顺序。
        let mut order: Vec<usize> = (0..entities.len()).collect();
        order.sort_by_key(|&index| (entities[index].position.y, entities[index].position.x));
        for index in order {
            let entity = &entities[index];
            let (origin_x, origin_y) = geometry::world_to_screen(viewport, center, entity.position);
            let top = self.draw_entity(frame, entity, origin_x, origin_y, animation_frame);
            if let Some(percent) = entity.hp_percent {
                let bar_y = top.unwrap_or(origin_y) - 6;
                draw_health_bar(frame, origin_x, bar_y, percent, entity.dead);
            } else if entity.dead {
                draw_dead_marker(frame, origin_x, origin_y);
            }
        }
    }

    /// 画一个实体，返回精灵顶边的屏幕 Y（用于血条定位）。
    fn draw_entity(
        &mut self,
        frame: &mut Frame,
        entity: &EntityView,
        origin_x: i32,
        origin_y: i32,
        animation_frame: u32,
    ) -> Option<i32> {
        let animation = if entity.dead { 0 } else { animation_frame };
        let (library, exact, base, item) = sprite_target(entity, animation);
        let candidates = [exact, base, 0];
        for index in candidates {
            if let Some(image) = self.image(library, index) {
                let (x, y) = if item {
                    (
                        origin_x + (CELL_WIDTH - i32::from(image.info.width)) / 2,
                        origin_y + (CELL_HEIGHT - i32::from(image.info.height)) / 2,
                    )
                } else {
                    (
                        origin_x + i32::from(image.info.offset_x),
                        origin_y + i32::from(image.info.offset_y),
                    )
                };
                frame.blit(&image, x, y);
                return Some(y);
            }
        }
        // 素材缺失：画一个小方块，至少让测试与无素材客户端看到"这里有个东西"。
        draw_placeholder(frame, origin_x, origin_y, entity);
        None
    }

    /// 取一张实体图（带缓存与纯黑透明处理）。
    fn image(&mut self, library: SpriteLibrary, index: u32) -> Option<Image> {
        let key = (library, index);
        if let Some(cached) = self.cache.get(&key) {
            return cached.clone();
        }
        if self.cache.len() >= MAX_ENTITY_CACHED_IMAGES {
            self.cache.clear();
        }
        let image = self.libraries.image(library, index).map(|mut image| {
            remove_black(&mut image);
            image
        });
        self.cache.insert(key, image.clone());
        image
    }
}

/// 计算一个实体要用的图库、精确帧与「该图像首帧」。
///
/// `item` 为真时调用方按物品居中规则定位。
fn sprite_target(entity: &EntityView, animation: u32) -> (SpriteLibrary, u32, u32, bool) {
    match entity.kind {
        EntityKind::Player => {
            let base = u32::from(entity.image) * PLAYER_FRAMES_PER_APPEARANCE;
            let exact = player_standing_frame(entity.image, entity.direction, animation);
            (SpriteLibrary::Hum, exact, base, false)
        }
        EntityKind::Monster => {
            let library = monster_library(entity.image);
            let base = u32::from(entity.image % 10) * MONSTER_FRAMES_PER_IMAGE;
            let exact = monster_standing_frame(entity.image, entity.direction, animation);
            (SpriteLibrary::Monster(library), exact, base, false)
        }
        EntityKind::Npc => {
            let (library, base) = npc_library(entity.image);
            (SpriteLibrary::Npc(library), base, base, false)
        }
        EntityKind::Item => {
            let base = u32::from(entity.image);
            (SpriteLibrary::Item, base, base, true)
        }
    }
}

/// 玩家/怪物/NPC 的图像号 → 图库与首帧。
///
/// * NPC：`image < 100` 用 `npc.wzl`，`100..200` 用 `npc2.wzl`，依此类推
///   （`NPCObject.cs:77-80` 的 `Libraries.NPCs[Image]` 在本项目扁平语料里的对应）；
/// * 每个 NPC 占 60 帧（本素材语料布局），取站立首帧。
#[must_use]
pub fn npc_library(image: u16) -> (u8, u32) {
    let library = u8::try_from(image / 100 + 1).unwrap_or(u8::MAX).clamp(1, 4);
    let slot = u32::from(image % 100);
    (library, slot * NPC_FRAMES_PER_IMAGE)
}

/// 怪物图像号 → `monN.wzl` 编号。
///
/// 经典 Mir2 的怪物图像按 10 个一组分库：0..=9 → `Mon1`，10..=19 → `Mon2`……
/// 本项目的扁平语料沿用这个分组（见 `resource/data/mon1.wzl` … `mon53.wzl`）。
#[must_use]
pub const fn monster_library(image: u16) -> u16 {
    image / 10 + 1
}

/// 玩家站立帧号（供测试与本模块文档使用）。
#[must_use]
pub fn player_standing_frame(appearance: u16, direction: MirDirection, animation: u32) -> u32 {
    u32::from(appearance) * PLAYER_FRAMES_PER_APPEARANCE
        + u32::from(direction_index(direction)) * PLAYER_DIRECTION_STRIDE
        + animation % (PLAYER_DIRECTION_STRIDE / 2)
}

/// 怪物站立帧号（供测试与本模块文档使用）。
#[must_use]
pub fn monster_standing_frame(image: u16, direction: MirDirection, animation: u32) -> u32 {
    u32::from(image % 10) * MONSTER_FRAMES_PER_IMAGE
        + u32::from(direction_index(direction)) * MONSTER_DIRECTION_STRIDE
        + animation % (MONSTER_DIRECTION_STRIDE / 2)
}

/// `MirDirection` 的线上序号（0=Up）。
fn direction_index(direction: MirDirection) -> u8 {
    direction as u8
}

/// 画素材缺失时的彩色占位方块。
fn draw_placeholder(frame: &mut Frame, origin_x: i32, origin_y: i32, entity: &EntityView) {
    let color = if entity.dead {
        entity.kind.dead_color()
    } else {
        entity.kind.color()
    };
    let left = origin_x + (CELL_WIDTH - 12) / 2;
    let top = origin_y + (CELL_HEIGHT - 18) / 2;
    frame.fill_rect(left - 1, top - 1, 14, 20, [0, 0, 0, 255]);
    frame.fill_rect(left, top, 12, 18, color);
}

/// 画 3px 血条（黑底 + 绿/黄/红前景）。
fn draw_health_bar(frame: &mut Frame, origin_x: i32, y: i32, percent: u8, dead: bool) {
    let width: i32 = 18;
    let left = origin_x + (CELL_WIDTH - width) / 2;
    let bar_width = u32::try_from(width + 2).unwrap_or(0);
    frame.fill_rect(left - 1, y - 1, bar_width, 5, [0, 0, 0, 255]);
    let color = if dead {
        [90, 90, 90, 255]
    } else if percent > 50 {
        [72, 208, 96, 255]
    } else if percent > 25 {
        [240, 208, 72, 255]
    } else {
        [232, 72, 72, 255]
    };
    let filled = u32::from(percent.min(100)) * u32::try_from(width).unwrap_or(0) / 100;
    frame.fill_rect(left, y, filled, 3, color);
}

/// 死亡但无血量信息时的灰色小横条。
fn draw_dead_marker(frame: &mut Frame, origin_x: i32, origin_y: i32) {
    let left = origin_x + (CELL_WIDTH - 18) / 2;
    frame.fill_rect(
        left,
        origin_y + CELL_HEIGHT - 4,
        18,
        3,
        [110, 110, 110, 255],
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    fn view(kind: EntityKind, image: u16) -> EntityView {
        EntityView {
            position: Point::new(2, 3),
            direction: MirDirection::Down,
            kind,
            image,
            name: None,
            dead: false,
            hp_percent: None,
        }
    }

    #[test]
    fn monster_libraries_follow_classic_ten_per_file_grouping() {
        assert_eq!(monster_library(0), 1);
        assert_eq!(monster_library(9), 1);
        assert_eq!(monster_library(10), 2);
        assert_eq!(monster_library(27), 3);
        assert_eq!(monster_library(208), 21);
    }

    #[test]
    fn npc_libraries_switch_every_hundred_images() {
        assert_eq!(npc_library(0), (1, 0));
        assert_eq!(npc_library(7), (1, 420));
        assert_eq!(npc_library(100), (2, 0));
        assert_eq!(npc_library(250), (3, 50 * NPC_FRAMES_PER_IMAGE));
        assert_eq!(npc_library(999), (4, 99 * NPC_FRAMES_PER_IMAGE));
    }

    #[test]
    fn standing_frames_use_direction_and_animation() {
        assert_eq!(player_standing_frame(2, MirDirection::Up, 0), 1200);
        assert_eq!(
            player_standing_frame(2, MirDirection::Down, 5),
            1200 + 4 * 8 + 1
        );
        assert_eq!(monster_standing_frame(27, MirDirection::Up, 0), 7 * 430);
        assert_eq!(
            monster_standing_frame(27, MirDirection::Down, 3),
            7 * 430 + 4 * 8 + 3
        );
    }

    #[test]
    fn missing_libraries_draw_colored_placeholders() {
        let mut renderer = EntityRenderer::empty();
        let mut frame = Frame::new(200, 200);
        frame.clear([0, 0, 0, 255]);
        let entities = [
            view(EntityKind::Player, 0),
            EntityView {
                position: Point::new(3, 3),
                ..view(EntityKind::Monster, 5)
            },
            EntityView {
                position: Point::new(2, 4),
                ..view(EntityKind::Npc, 0)
            },
            EntityView {
                position: Point::new(1, 3),
                ..view(EntityKind::Item, 0)
            },
        ];
        renderer.render(
            &mut frame,
            Viewport::new(200, 200),
            Point::new(2, 3),
            &entities,
            0,
        );
        assert!(frame.distinct_color_count() >= 5, "四种占位色都应出现");
        assert_eq!(renderer.cache_len(), 0, "没有素材就不该缓存图像");
    }

    #[test]
    fn entities_are_sorted_by_y_then_x() {
        // 两个同格实体：后画的会覆盖先画的；排序必须稳定。
        let mut entities = vec![
            view(EntityKind::Player, 0),
            EntityView {
                kind: EntityKind::Monster,
                ..view(EntityKind::Monster, 5)
            },
            EntityView {
                position: Point::new(2, 2),
                ..view(EntityKind::Item, 0)
            },
        ];
        let mut order: Vec<usize> = (0..entities.len()).collect();
        order.sort_by_key(|&index| (entities[index].position.y, entities[index].position.x));
        assert_eq!(order, vec![2, 0, 1]);
        entities.clear();
    }

    #[test]
    fn hp_bar_is_clipped_inside_the_frame() {
        let mut frame = Frame::new(40, 40);
        frame.clear([0, 0, 0, 255]);
        draw_health_bar(&mut frame, 0, -10, 100, false);
        // 完全在帧外：什么也不画。
        assert_eq!(frame.distinct_color_count(), 1);
        draw_health_bar(&mut frame, 20, 10, 50, false);
        assert!(frame.distinct_color_count() >= 2);
    }
}
