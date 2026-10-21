//! 《传奇-Next》世界层：地图、视野、行走与规则。
//!
//! 当前包含 Crystal 服务器端 `.map` 地图文件读取器（[`map`] 模块），对应
//! `Server/MirEnvir/Map.cs`（下称 `Map.cs`）中的 `Load()` / `FindType()` /
//! `LoadMapCellsv*()` / `LoadMapCellsV100()`。

pub mod map;
pub mod mnm;

pub use map::{Door, Map, MapCell, MapError, MapVersion};
