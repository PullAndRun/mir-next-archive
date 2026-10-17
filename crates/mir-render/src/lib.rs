//! 《传奇-Next》客户端地图渲染：纯软件、无窗口、可确定性测试的 Mir2 等距地图光栅化。
//!
//! 这个 crate 只做「地图 → RGBA8 帧缓冲」这一件事，不碰窗口、输入、网络与音频；
//! winit / softbuffer 只出现在 `mir-client` 里。渲染结果只由（地图、素材、中心格、
//! 动画帧号、帧尺寸）决定，同样的输入得到逐字节相同的像素。
//!
//! # 与 Crystal 的对应关系
//!
//! Crystal 的地图绘制在 `Client/MirScenes/GameScene.cs` 的 `MapControl` 里，
//! 分成两个阶段：
//!
//! 1. **铺地层** `DrawFloor`（`GameScene.cs:10658-10764`）：后层 `BackImage`
//!    以 2×2 格为一块画成 96×64 的菱形瓦片，中层与「扁平」前层瓦片在同一循环里
//!    画到 `FloorTexture` 上。
//! 2. **对象层** `DrawObjects`（`GameScene.cs:10787-11008`）：按地图 Y 升序、同一行按
//!    X 升序，画「高」的前层/中层图像（树、墙、灯），每张图以格子锚点为底边向上生长。
//!
//! 本 crate 把两阶段原样拆成 [`MapScene::render`] 里的两趟循环，顺序与 Crystal 一致；
//! 因为不缓存 `FloorTexture`，每帧都完整重画，结果与 Crystal 首次进入地图时相同。
//!
//! # 坐标系统（三个不同的锚点）
//!
//! Crystal 对同一个格子有三种屏幕锚点，三者不能混用：
//!
//! | 锚点 | 公式 | Crystal 出处 | 用途 |
//! |---|---|---|---|
//! | 格原点 | `(cell - center + OffSet) * (48, 32)` | `PlayerObject.cs:847`、`GameScene.cs:10323` | 鼠标拾取、角色 `DrawLocation`；本 crate 的 [`MapScene::world_to_screen`] |
//! | 瓦片左上角 | 格原点再左移 `OffSetX` | `GameScene.cs:10680`、`GameScene.cs:10820` | 铺地瓦片、前层图像的 `drawX` |
//! | 精灵底边 | 格原点下移一格（+32） | `GameScene.cs:10814` | 高前层/中层图像以底边对齐的 `drawY` |
//!
//! 其中 `OffSetX = 帧宽 / 2 / 48`、`OffSetY = 帧高 / 2 / 32 - 1`
//! （`GameScene.cs:10398-10399`）。`- OffSetX` 与 `+1 格` 都是 Crystal 的历史写法，
//! 本 crate 原样保留，细节见 [`geometry`] 模块文档。
//!
//! # 素材库编号
//!
//! 地图文件里的图像号不带库号，库号由地图版本与额外字节决定（`MapCode.cs:240-252`）：
//!
//! | 图层 | v0 库号 | 说明 |
//! |---|---|---|
//! | 后层 `BackImage` | `0` → `Tiles.wzl` | 固定 0（`MapCode.cs:240`） |
//! | 中层 `MiddleImage` | `1` → `SmTiles.wzl` | 固定 1（`MapCode.cs:241`） |
//! | 前层 `FrontImage` | `o+10 + 2` | 2 = `Objects.wzl`，3 = `Objects2.wzl` …（`MapCode.cs:252`） |
//!
//! Crystal 的 `MapLibs` 数组有 400 项（`MLibrary.cs:44`），本 crate 只映射 Mir2 的
//! `0..=28` 与 `190`（详细名称表见 [`LIBRARY_FILES`]），因为
//! `resource/data` 是平铺的 `.wzl`，没有 Crystal 的 `Map/WemadeMir2/…` 目录结构。
//!
//! # 透明色
//!
//! `resource/data/*.wzl` 的 8 位调色板 alpha 全是 255（见 `mir-asset` crate 文档），
//! 而地图图像（尤其是树、灯）以**纯黑**为透明色。Crystal 的 `.Lib` 在写库时已经由
//! `removeBlack` 把纯黑变成 alpha 0（`LibraryEditor/Graphics/MLibraryV2.cs:419-426`），
//! `.wzl` 没有这一步，所以本 crate 在解码后对调色板图像补做同样的处理
//! （[`MapScene`] 的缓存里，见 `scene::remove_black`）；alpha 混合本身仍是标准
//! source-over（[`Frame::blit`]）。
//!
//! # 本里程碑不画的东西
//!
//! * 光照、昼夜、天气粒子（`GameScene.cs:11024-11300`）、小地图/大地图；
//! * 地图背景图（`DrawBackground`，`GameScene.cs:10765-10785`：只有 `ID1`/`ID2`
//!   等少数地图名才画）与 Shanda 瓦片动画层（`MapLibs[190] AniTiles1`，
//!   `GameScene.cs:10825-10836`；v0 地图没有 `TileAnimationImage` 数据）；
//! * 角色的头发/武器/坐骑/特效（UI 在 `mir-client` 里，用本 crate 的文字与图库原语绘制）；
//! * 实体名字文字（[`text`] 已能画中文，但实体层还没接名字绘制）。
//! * 门的开关状态：静态帧按「门关闭」处理，不做 `DoorOffset` 的帧号调整
//!   （`GameScene.cs:10902-10917`）；
//! * 后层/中层的动画：v0 的客户端读不到这两层的动画字节（`MapCode.cs:239-253`），
//!   只有前层动画会随 [`MapScene::set_animation_frame`] 播放。
//!
//! # 文字
//!
//! [`text`] 模块用 `ab_glyph` 栅格化 TrueType 字体，支持中文；[`pixel`] 是找不到字体时
//! 的 3×5 点阵兜底。UI 本身在 `mir-client` 里，本 crate 只提供「文字 → RGBA」原语。
//!
//! # 实体层
//!
//! [`MapScene::render_with_entities`] 在地形之后按地图 Y、X 顺序追加玩家、怪物、
//! NPC 与地面物品精灵（`GameScene.cs:10798-11008` 的对象顺序）。实体图库选用、
//! 帧号公式与素材缺失时的占位行为见 [`entities`] 模块文档；简单说：
//!
//! | 实体 | 文件 | 帧号 |
//! |---|---|---|
//! | 玩家 | `hum.wzl` | `appearance*600 + direction*8 + n` |
//! | 怪物 | `mon{image/10+1}.wzl` | `(image%10)*430 + direction*8 + n` |
//! | NPC | `npc.wzl` / `npc2..4.wzl` | `(image%100)*60` |
//! | 物品 | `items.wzl` | `image`（金币用 112..116，`ItemObject.cs:55-64`） |
//!
//! 精灵缺失时画 12×18 的彩色占位方块（按类型着色），所以没有 `resource/` 的 CI
//! 也能在 `Frame` 里看到实体。血条是 3px 色块，不依赖字体。
//!
//! 阴影：Crystal 客户端**没有**地图/对象的阴影绘制。`MImage` 虽然解析了
//! `ShadowX/ShadowY`（`MLibrary.cs:887-888`），但全仓库没有任何地方用它们画阴影，
//! 所以这里也没有阴影遍。
//!
//! # 版本支持范围
//!
//! 完整支持 **v0 / v4**（后层与中层库号固定为 0/1、前层为 `o+10 + 2`，见
//! `MapCode.cs:240-252`、`MapCode.cs:430-442`）。v2/v3 的前层库号是 `o+10 + 120`，
//! 但后层/中层库号来自格内另外的字节，`mir-world` 目前没有暴露，因此这些版本只会
//! 画出前层（而且 120 号以上的 Shanda 库不在 [`LIBRARY_FILES`] 里，实际也画不出来）。
//! v1/v5/v6/v7/v100 的图层映射各不相同，M6 不做保证。
//!
//! # 从 Crystal 原样搬过来的“坑”
//!
//! 这些都是对拍源码时确认过的行为，改动它们会让画面与 Crystal 不一致：
//!
//! 1. **铺地/精灵的 x 比格原点左 `OffSetX`**（`GameScene.cs:10680`、`:10820`），
//!    而鼠标拾取与角色 `DrawLocation` 不带这个偏移（`GameScene.cs:10323`、
//!    `PlayerObject.cs:847`）。800×600 下两者相差 8 像素。
//! 2. **高精灵的 y 比格原点低一格**（`GameScene.cs:10814` 的 `+ 1`），
//!    图像以这个 y 为底边向上画（`GameScene.cs:10935`）。
//! 3. **96×64 的前层图**在静止帧（动画字节为 0）会被当作「格尺寸」烘进铺地层，
//!    左上角对齐瓦片原点；一旦有动画帧（字节非 0），同一张图改走对象层并以**底边**
//!    对齐，位置比铺地层高 32 像素（`GameScene.cs:10752-10755` vs `:10935`）。
//! 4. **后层只画偶数 x/y 的格子**（`GameScene.cs:10703`），即使奇数格也带
//!    `BackImage` 也忽略。
//! 5. **中层第一帧在对象层被跳过**（`GameScene.cs:10842` 要求 `index > 0`），
//!    但铺地层会画（`:10715`）；所以中层第 0 帧只出现在铺地层。
//! 6. **`DrawBlend` 是加法混合**，不是普通 alpha 覆盖：`DXManager.SetBlend(true)`
//!    用 `SourceAlpha / One`（`DXManager.cs` 的 `SetBlend`）。灯光类前层
//!    （`0.map` 里的 `0x8A` 动画字节）靠加法让黑色背景消失。
//! 7. **前层动画字节 `o+8` 同时是服务器眼里的门字节**：客户端把它读成
//!    `FrontAnimationFrame`（`MapCode.cs:250`），服务器把它读成门（`Map.cs:142`，
//!    见 `docs/world/map-format.md` §4.4）。`mir-world` 的 `MapCell::door_raw` 就是
//!    这个字节，所以本 crate 用 `door_raw` 当作动画帧数。
//! 8. **门的帧号调整**用的是另一套字节：`o+6 & 0x7F` 是客户端门索引、`o+7` 是帧步长
//!    （`MapCode.cs:248-249`、`GameScene.cs:10902-10917`），与服务器门字节不同。
//! 9. **28 号库特殊**：`fileIndex == 28` 且图像自带偏移时，前层图按「下一格」
//!    加上图像偏移定位（`GameScene.cs:10932-10933`）；而其它普通前层图忽略图像偏移，
//!    只有 2723..=2732 的灯光图应用偏移（`:10928`）。
//! 10. **200 号库只在铺地层被跳过**（`GameScene.cs:10731`），对象层没有这个判断。
//! 11. **铺地层会跳过 `y <= 0` 的行**（`GameScene.cs:10689`），本实现没有跳过；
//!     这只影响地图最上面一行，属于 Crystal 的小 bug。

//!
//! # 示例
//!
//! ```no_run
//! use mir_core::Point;
//! use mir_render::{Frame, MapScene};
//!
//! let mut scene = MapScene::load("resource/map/0.map", "resource/data")?;
//! let mut frame = Frame::new(800, 600);
//! scene.render(&mut frame, Point::new(330, 270));
//! frame.save_png("target/render-map0.png")?;
//! # Ok::<(), mir_render::RenderError>(())
//! ```

pub mod entities;
mod error;
mod frame;
pub mod geometry;
mod scene;
pub mod text;
mod tiles;

pub use entities::{
    EntityKind, EntityLibraries, EntityRenderer, EntityView, MAX_ENTITY_CACHED_IMAGES,
    MONSTER_DIRECTION_STRIDE, MONSTER_FRAMES_PER_IMAGE, NPC_FRAMES_PER_IMAGE,
    PLAYER_DIRECTION_STRIDE, PLAYER_FRAMES_PER_APPEARANCE, monster_library, monster_standing_frame,
    npc_library, player_standing_frame,
};
pub use error::RenderError;
pub use frame::{Frame, scale_to_fit};
pub use geometry::{CELL_HEIGHT, CELL_WIDTH, Viewport};
// 客户端 UI 画物品图标时需要 `Image` 的尺寸/像素字段，但不应直接依赖 `mir-asset`。
pub use mir_asset::Image;
pub use scene::MapScene;
pub use text::{TextClip, TextError, TextRenderer, font_candidates, pixel};
pub use tiles::{LIBRARY_FILES, TileLibraries};

/// 帧缓冲的默认背景色：不透明黑（`MapControl.BackColour = Color.Black`，
/// `GameScene.cs:10406`）。
pub const BACKGROUND: [u8; 4] = [0, 0, 0, 255];
