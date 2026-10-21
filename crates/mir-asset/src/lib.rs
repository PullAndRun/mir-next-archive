//! 《传奇-Next》客户端素材层：打开 Legend of Mir 2 的图像库，把单张图解码成 RGBA8。
//!
//! 这个 crate 只做「容器格式 + 像素解码」两件事，不碰渲染。
//!
//! # 两个容器
//!
//! | 容器 | 识别方式 | 像素数据 | 行序 | Crystal 出处 |
//! |---|---|---|---|---|
//! | 旧版 `.Lib` | 扩展名，或头里的 `i32` 版本 1/2/3 | gzip 压缩的 RGBA32 或 DXT1 | 自上而下 | `Client/MirGraphics/MLibrary.cs`、`LibraryEditor/Graphics/MLibraryV1.cs` |
//! | `.wzl` + `.wzx` | 扩展名 + 同名 `.wzx` | zlib 压缩的 8/16/24 位 | **自下而上** | `LibraryEditor/Graphics/WeMadeLibrary.cs` |
//!
//! 注意：Crystal 的**客户端**源码里只有 `.Lib`（见 [`ImageLibrary`] 与 `lib_container`
//! 模块文档）；`.wzl`/`.wzx` 的读写代码在 `LibraryEditor` 工具里（`WeMadeLibrary.cs` 的
//! `_nType == 1` 分支），本 crate 的格式细节都标了对应的 `WeMadeLibrary.cs:行号`。
//!
//! # `.Lib` 的版本与头布局
//!
//! * **版本 1**：`MLibraryV1` 写的 DXT1 库（`MLibraryV1.cs:21`、`MLibraryV1.cs:51-55`），
//!   头是 `版本 + 数量`，偏移表从 8 开始。语料里有 4561 个（`ArtificialMir` 的怪物/装备库）。
//! * **版本 2**：`MLibraryV2` 早期的写法，同样是 8 字节头 + 偏移表。
//! * **版本 3（Crystal 布局）**：`版本 + 数量 + frameSeek`，偏移表从 12 开始
//!   （`MLibrary.cs:522-526`，写入端 `LibraryEditor/Graphics/MLibraryV2.cs:109-133`），像素是
//!   gzip RGBA32。语料里 4271 个（Jev 客户端等）。
//! * **版本 3（无 `frameSeek` 变体）**：头只有 `版本 + 数量`，偏移表从 8 开始，第一个偏移
//!   正好等于偏移表末尾；像素是 DXT1。语料里 1350 个（`ArtificialMir2` 客户端、社区武器/盔甲包）。
//!
//! `lib_container` 用 `u32@8 == 8 + 数量 × 4` 区分两种 v3 布局（见 `parse_header` 的文档）。
//! 像素存储方式没有字段标记：先看数据开头是不是 gzip 魔数 `1f 8b`，不是再看长度是不是
//! 正好 `⌈宽/4⌉ × ⌈高/4⌉ × 8`（DXT1，`MLibraryV1.cs:433-442`、`MLibraryV1.cs:466-495`）。
//!
//! # 像素格式
//!
//! | `ImageInfo::format` | 名称 | 解码方式 |
//! |---|---|---|
//! | `0`、`3` | 8 位调色板 | 查硬编码的 `WeMade` 256 色表，alpha 恒 255 |
//! | `5` | RGB565 | 只做移位展开，不做低位复制（`WeMadeLibrary.cs:223-229`） |
//! | `6` | RGB24 | 语料扩展；Crystal 会把它当 8 位调色板，见下 |
//! | `0xFF` | `.Lib` 的 gzip RGBA32 | 直接输出解压后的 RGBA |
//! | `0xFE` | `.Lib` 的 DXT1 | 按 BC1 块解码成 RGBA |
//!
//! `0xFF` 与 `0xFE` 是本实现对 `.Lib` 的标记，文件里没有这两个字节：`.Lib` 记录头没有
//! 格式字段（`MLibrary.cs:880-890`），存储方式是按数据内容识别出来的。
//!
//! # 与 Crystal 的差异（有意为之）
//!
//! * **`6` 号格式**：语料里只有 2 条（`ui1.wzl` 第 5171 张、`objects31.wzl` 第 1305 张），
//!   解压后长度正好是 `宽 × 高 × 3`，按 24 位解码能看到原始水印文字。Crystal 用
//!   `bo16bit = (byte == 5)` 判定（`WeMadeLibrary.cs:319`），会把这条记录当 8 位调色板读错；
//!   本实现按 24 位解码。
//! * **未知格式**：Crystal 把任何非 5 的格式都按 8 位调色板读；本实现对 `0`/`3`/`5`/`6`
//!   之外的值直接报 [`AssetError::UnsupportedFormat`]，避免把未知格式静默画错。
//! * **多余解压数据**：Crystal 把解压结果整块拷进贴图，多出来的字节会抛异常；这里只取
//!   需要的前 `宽 × 高 × 每像素字节` 字节。
//! * **尺寸上限**：单张图解压后超过 256 MiB 直接报 [`AssetError::ImageTooLarge`]，
//!   防止解压炸弹（Crystal 没有这个检查）。
//!
//! # 语料里的异常
//!
//! * `resource/data` 的 349 对 `.wzl`/`.wzx` 里，有 40 个 `.wzx` 头里偏移 44 声明的数量
//!   与偏移表项数不一致（例如 `hair_horse.wzx` 声明 640、表里 960 项；`chrsel.wzx`
//!   声明 618、表里 770 项）。Crystal 根本不看这个字段（`WeMadeLibrary.cs:140-147`），
//!   所以 [`ImageLibrary::len`] 也按表项数算，[`ImageLibrary::declared_count`] 只用于诊断。
//! * 偏移表里的 `0` 表示「没有这张图」，语料里约 81 万项是这样；[`ImageLibrary::info`]
//!   对它们返回宽高为 0 的空头。
//! * 旧 `.Lib` 的透明像素来自写库工具的 removeBlack（`LibraryEditor/Graphics/MLibraryV2.cs:419-426`），
//!   不是调色板规则；`.wzl` 的 8 位调色板 alpha 全是 255，没有透明色。
//!
//! # 示例
//!
//! ```no_run
//! use mir_asset::ImageLibrary;
//!
//! let library = ImageLibrary::open("resource/data/anitiles1.wzl")?;
//! println!("{} 张图，容器 {}", library.len(), library.kind().as_str());
//! let image = library.image(0)?;
//! println!(
//!     "{}×{} {}",
//!     image.info.width,
//!     image.info.height,
//!     image.info.format_name()
//! );
//! assert_eq!(
//!     image.pixels.len(),
//!     usize::from(image.info.width) * usize::from(image.info.height) * 4
//! );
//! # Ok::<(), mir_asset::AssetError>(())
//! ```

mod dxt1;
mod error;
mod image;
mod lib_container;
mod library;
mod pack;
mod palette;
mod source;
mod unpacked;
mod wil;
mod wzl;

pub use error::AssetError;
pub use image::{Image, ImageInfo, format};
pub use library::{ImageLibrary, LibraryKind};
pub use pack::{PackFile, PackManifest, PackSet, PackSummary, PackWriter};
pub use unpacked::{LibrarySource, UnpackedLibrary};

/// 索引项数量的上限，防止畸形文件让打开阶段一次性分配巨量内存。
pub(crate) const MAX_IMAGES: u64 = 4_000_000;

/// 单张图解压后的字节上限，防止解压炸弹。
pub(crate) const MAX_IMAGE_BYTES: u64 = 256 * 1024 * 1024;
