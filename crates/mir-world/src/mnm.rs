//! `.mnm`（Mir Next Map）读写。
//!
//! 运行时自有格式：头 + 元数据 + 分层的 varint/RLE 瓦片层 + 每格杂项 + 门表 + TLV。
//! 老 `.map` 只在 `Map::load` 的回退路径/导入工具里解析。

use std::io::{Read, Write};
use std::path::Path;

use flate2::Compression;
use flate2::read::ZlibDecoder;
use flate2::write::ZlibEncoder;
use mir_core::Point;

use crate::map::{Door, Map, MapCell, MapError, MapVersion};

/// 文件头魔数。
pub const MAGIC: &[u8; 4] = b"MNM1";
/// 当前格式版本。
pub const FORMAT_VERSION: u16 = 1;

/// 一张地图写出的 `.mnm` 字节。
pub fn encode(map: &Map) -> Result<Vec<u8>, MapError> {
    let width = u16::try_from(map.width)
        .map_err(|_| MapError::Mnm(format!("宽度 {} 超过 u16", map.width)))?;
    let height = u16::try_from(map.height)
        .map_err(|_| MapError::Mnm(format!("高度 {} 超过 u16", map.height)))?;
    let cells = usize::try_from(map.width)
        .ok()
        .and_then(|w| {
            usize::try_from(map.height)
                .ok()
                .and_then(|h| w.checked_mul(h))
        })
        .ok_or_else(|| MapError::Mnm("地图尺寸溢出".to_owned()))?;
    if map.cells.len() != cells {
        return Err(MapError::Mnm(format!(
            "cells 数量 {} 与 {}x{} 不符",
            map.cells.len(),
            map.width,
            map.height
        )));
    }

    let mut out = Vec::new();
    out.extend_from_slice(MAGIC);
    out.extend_from_slice(&FORMAT_VERSION.to_le_bytes());
    out.extend_from_slice(&0_u16.to_le_bytes()); // flags
    out.extend_from_slice(&width.to_le_bytes());
    out.extend_from_slice(&height.to_le_bytes());
    out.push(map.version.raw() as u8);
    out.push(0); // reserved
    let name = map.file_name.as_bytes();
    let name_len = u16::try_from(name.len()).map_err(|_| MapError::Mnm("地图名过长".to_owned()))?;
    out.extend_from_slice(&name_len.to_le_bytes());
    out.extend_from_slice(name);

    // 门表：数量 + (index, x, y)。
    let door_count =
        u16::try_from(map.doors.len()).map_err(|_| MapError::Mnm("门表过长".to_owned()))?;
    out.extend_from_slice(&door_count.to_le_bytes());
    for door in &map.doors {
        out.push(door.index);
        write_i32(&mut out, door.location.x);
        write_i32(&mut out, door.location.y);
    }

    // 六层：back / middle / front / attribute / misc / (扩展段在末尾)。
    let mut back = Vec::new();
    encode_rle_u32(map.cells.iter().map(|cell| cell.back_image), &mut back);
    write_layer(&mut out, &back)?;
    let mut middle = Vec::new();
    encode_rle_u32(
        map.cells.iter().map(|cell| u32::from(cell.middle_image)),
        &mut middle,
    );
    write_layer(&mut out, &middle)?;
    let mut front = Vec::new();
    encode_rle_u32(
        map.cells.iter().map(|cell| u32::from(cell.front_image)),
        &mut front,
    );
    write_layer(&mut out, &front)?;
    let mut attributes = Vec::with_capacity(map.cells.len());
    for cell in &map.cells {
        attributes.push(attribute_code(cell));
    }
    write_layer(&mut out, &attributes)?;
    let mut misc = Vec::with_capacity(map.cells.len() * 9);
    for cell in &map.cells {
        misc.push(cell.fishing_attribute as u8);
        misc.push(cell.door_raw);
        misc.push(cell.light);
        misc.push(cell.client_door_index);
        misc.push(cell.door_offset);
        misc.push(cell.front_animation_frame);
        misc.push(cell.front_animation_tick);
        misc.extend_from_slice(&cell.front_index.to_le_bytes());
    }
    write_layer(&mut out, &misc)?;

    // TLV 扩展段：当前为空，读取端跳过未知 tag。
    out.extend_from_slice(&0_u16.to_le_bytes());
    Ok(out)
}

/// 解析 `.mnm` 字节。
pub fn decode(data: &[u8]) -> Result<Map, MapError> {
    let mut cursor = Cursor::new(data);
    if cursor.take(4)? != MAGIC {
        return Err(MapError::Mnm("bad magic (expected MNM1)".to_owned()));
    }
    let version = cursor.u16()?;
    if version != FORMAT_VERSION {
        return Err(MapError::Mnm(format!("unsupported .mnm version {version}")));
    }
    let _flags = cursor.u16()?;
    let width = i32::from(cursor.u16()?);
    let height = i32::from(cursor.u16()?);
    let source_version = MapVersion::from_raw(u16::from(cursor.u8()?))
        .ok_or_else(|| MapError::Mnm("bad source map version".to_owned()))?;
    let _reserved = cursor.u8()?;
    let name_len = usize::from(cursor.u16()?);
    let name = String::from_utf8(cursor.take(name_len)?.to_vec())
        .map_err(|_| MapError::Mnm("map name is not UTF-8".to_owned()))?;
    let door_count = usize::from(cursor.u16()?);
    let mut doors = Vec::with_capacity(door_count);
    for _ in 0..door_count {
        let index = cursor.u8()?;
        let x = cursor.i32()?;
        let y = cursor.i32()?;
        doors.push(Door {
            index,
            location: Point::new(x, y),
        });
    }

    let cells = usize::try_from(width)
        .ok()
        .and_then(|w| usize::try_from(height).ok().and_then(|h| w.checked_mul(h)))
        .ok_or_else(|| MapError::Mnm("bad dimensions".to_owned()))?;
    let back = decode_rle_u32(&decompress(&cursor.layer()?)?, cells)?;
    let middle = decode_rle_u32(&decompress(&cursor.layer()?)?, cells)?;
    let front = decode_rle_u32(&decompress(&cursor.layer()?)?, cells)?;
    let attributes = decompress(&cursor.layer()?)?;
    if attributes.len() != cells {
        return Err(MapError::Mnm(format!(
            "attribute layer length {} != {cells}",
            attributes.len()
        )));
    }
    let misc = decompress(&cursor.layer()?)?;
    if misc.len() != cells * 9 {
        return Err(MapError::Mnm("misc layer length mismatch".to_owned()));
    }
    let mut out = Vec::with_capacity(cells);
    for i in 0..cells {
        let base = i * 9;
        let front_index = i16::from_le_bytes([misc[base + 7], misc[base + 8]]);
        out.push(MapCell {
            attribute: attribute_from_code(attributes[i]),
            fishing_attribute: misc[base] as i8,
            door_raw: misc[base + 1],
            door_index: (misc[base + 1] > 0).then_some(misc[base + 1] & 0x7F),
            light: misc[base + 2],
            back_image: back[i],
            middle_image: u16::try_from(middle[i])
                .map_err(|_| MapError::Mnm("middle image overflow".to_owned()))?,
            front_image: u16::try_from(front[i])
                .map_err(|_| MapError::Mnm("front image overflow".to_owned()))?,
            client_door_index: misc[base + 3],
            door_offset: misc[base + 4],
            front_animation_frame: misc[base + 5],
            front_animation_tick: misc[base + 6],
            front_index,
        });
    }
    // 兼容历史文件：door_index 可能来自 door_raw 的 >0 规则；保持一致。
    let ext_count = cursor.u16()?;
    for _ in 0..ext_count {
        let _tag = cursor.u16()?;
        let len = usize::try_from(cursor.u32()?)
            .map_err(|_| MapError::Mnm("TLV too large".to_owned()))?;
        let _ = cursor.take(len)?;
    }
    if cursor.remaining() != 0 {
        return Err(MapError::Mnm(format!(
            "{} trailing bytes",
            cursor.remaining()
        )));
    }
    Ok(Map {
        width,
        height,
        cells: out,
        file_name: name,
        version: source_version,
        doors,
    })
}

/// 从 `.mnm` 文件读取；路径不存在/格式错误直接返回错误。
pub fn open(path: &Path) -> Result<Map, MapError> {
    let data = std::fs::read(path)?;
    decode(&data)
}

fn write_layer(out: &mut Vec<u8>, layer: &[u8]) -> Result<(), MapError> {
    let compressed = compress(layer)?;
    let len =
        u32::try_from(compressed.len()).map_err(|_| MapError::Mnm("layer too large".to_owned()))?;
    out.extend_from_slice(&len.to_le_bytes());
    out.extend_from_slice(&compressed);
    Ok(())
}

fn compress(data: &[u8]) -> Result<Vec<u8>, MapError> {
    let mut encoder = ZlibEncoder::new(Vec::new(), Compression::default());
    encoder.write_all(data)?;
    Ok(encoder.finish()?)
}

fn decompress(data: &[u8]) -> Result<Vec<u8>, MapError> {
    let mut decoder = ZlibDecoder::new(data);
    let mut out = Vec::new();
    decoder.read_to_end(&mut out)?;
    Ok(out)
}

fn attribute_code(cell: &MapCell) -> u8 {
    match cell.attribute {
        mir_core::CellAttribute::Walk => 0,
        mir_core::CellAttribute::HighWall => 1,
        mir_core::CellAttribute::LowWall => 2,
    }
}

fn attribute_from_code(code: u8) -> mir_core::CellAttribute {
    match code {
        1 => mir_core::CellAttribute::HighWall,
        2 => mir_core::CellAttribute::LowWall,
        _ => mir_core::CellAttribute::Walk,
    }
}

fn encode_rle_u32(values: impl Iterator<Item = u32>, out: &mut Vec<u8>) {
    let mut iter = values.peekable();
    while let Some(value) = iter.next() {
        let mut run: u32 = 1;
        while iter.peek() == Some(&value) && run < u32::MAX {
            iter.next();
            run += 1;
        }
        write_varint(u64::from(run), out);
        write_varint(u64::from(value), out);
    }
}

fn decode_rle_u32(data: &[u8], count: usize) -> Result<Vec<u32>, MapError> {
    let mut cursor = Cursor::new(data);
    let mut out = Vec::with_capacity(count);
    while out.len() < count {
        let run = cursor.varint()?;
        let value = cursor.varint()?;
        if run == 0 || run > u64::try_from(count - out.len()).unwrap_or(0) {
            return Err(MapError::Mnm("bad RLE run".to_owned()));
        }
        for _ in 0..run {
            out.push(
                u32::try_from(value).map_err(|_| MapError::Mnm("RLE value overflow".to_owned()))?,
            );
        }
    }
    if cursor.remaining() != 0 {
        return Err(MapError::Mnm("trailing RLE bytes".to_owned()));
    }
    Ok(out)
}

fn write_varint(mut value: u64, out: &mut Vec<u8>) {
    while value >= 0x80 {
        out.push((value as u8) | 0x80);
        value >>= 7;
    }
    out.push(value as u8);
}

fn write_i32(out: &mut Vec<u8>, value: i32) {
    // zigzag：坐标可能为负，varint 要保留符号。
    let zigzag = ((value << 1) ^ (value >> 31)) as u32;
    write_varint(u64::from(zigzag), out);
}

struct Cursor<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> Cursor<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self { data, pos: 0 }
    }
    fn take(&mut self, len: usize) -> Result<&'a [u8], MapError> {
        let end = self
            .pos
            .checked_add(len)
            .ok_or_else(|| MapError::Mnm("cursor overflow".to_owned()))?;
        let slice = self.data.get(self.pos..end).ok_or_else(|| {
            MapError::Mnm(format!(
                "truncated .mnm at {} need {} total {}",
                self.pos,
                len,
                self.data.len()
            ))
        })?;
        self.pos = end;
        Ok(slice)
    }
    fn u8(&mut self) -> Result<u8, MapError> {
        Ok(self.take(1)?[0])
    }
    fn u16(&mut self) -> Result<u16, MapError> {
        Ok(u16::from_le_bytes(
            self.take(2)?.try_into().unwrap_or([0; 2]),
        ))
    }
    fn u32(&mut self) -> Result<u32, MapError> {
        Ok(u32::from_le_bytes(
            self.take(4)?.try_into().unwrap_or([0; 4]),
        ))
    }
    fn i32(&mut self) -> Result<i32, MapError> {
        let zigzag = self.varint()?;
        Ok(((zigzag >> 1) as i32) ^ -((zigzag & 1) as i32))
    }
    fn varint(&mut self) -> Result<u64, MapError> {
        let mut result = 0u64;
        for shift in (0..64).step_by(7) {
            let byte = self.u8()?;
            result |= u64::from(byte & 0x7F) << shift;
            if byte & 0x80 == 0 {
                return Ok(result);
            }
        }
        Err(MapError::Mnm("varint too long".to_owned()))
    }
    fn layer(&mut self) -> Result<Vec<u8>, MapError> {
        let len = usize::try_from(self.u32()?)
            .map_err(|_| MapError::Mnm("layer too large".to_owned()))?;
        Ok(self.take(len)?.to_vec())
    }
    fn remaining(&self) -> usize {
        self.data.len().saturating_sub(self.pos)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::map::MapCell;
    use mir_core::CellAttribute;
    use mir_core::Point;

    fn sample() -> Map {
        Map {
            width: 2,
            height: 2,
            cells: vec![
                MapCell {
                    attribute: CellAttribute::HighWall,
                    back_image: 0x1234_5678,
                    middle_image: 2,
                    front_image: 3,
                    front_animation_frame: 7,
                    front_index: 90,
                    light: 105,
                    fishing_attribute: 5,
                    door_raw: 0x82,
                    door_index: Some(2),
                    client_door_index: 2,
                    door_offset: 1,
                    front_animation_tick: 3,
                },
                MapCell::default(),
                MapCell {
                    attribute: CellAttribute::LowWall,
                    ..MapCell::default()
                },
                MapCell {
                    attribute: CellAttribute::Walk,
                    ..MapCell::default()
                },
            ],
            file_name: "unit".to_owned(),
            version: MapVersion::V1,
            doors: vec![Door {
                index: 2,
                location: Point::new(0, 0),
            }],
        }
    }

    #[test]
    fn real_v0_v1_v4_maps_roundtrip_cell_for_cell() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let samples = [
            root.join("resource/map/0100.map"),
            root.join("resource/community/suprcode-crystal-database/cjlaaa/Maps/0100.map"),
            root.join("resource/community/mirfiles-apoc/Apoc/Map/LABY01.map"),
        ];
        let mut checked = 0;
        for path in samples {
            if !path.exists() {
                continue;
            }
            let source = Map::load_legacy(&path).expect("load legacy");
            let encoded = encode(&source).expect("encode");
            let decoded = decode(&encoded).expect("decode");
            assert_eq!(decoded.width, source.width);
            assert_eq!(decoded.height, source.height);
            assert_eq!(decoded.file_name, source.file_name);
            assert_eq!(decoded.version, source.version);
            assert_eq!(decoded.doors, source.doors, "doors {}", path.display());
            assert_eq!(decoded.cells.len(), source.cells.len());
            for (index, (a, b)) in decoded.cells.iter().zip(&source.cells).enumerate() {
                assert_eq!(a, b, "cell {index} of {}", path.display());
            }
            checked += 1;
        }
        assert!(checked >= 1, "至少应有一个真实 .map 样本");
    }
    #[test]
    #[allow(clippy::cast_precision_loss)]
    fn real_maps_report_mnm_size_ratio() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let samples = [
            root.join("resource/map/0100.map"),
            root.join("resource/community/suprcode-crystal-database/cjlaaa/Maps/0100.map"),
            root.join("resource/community/mirfiles-apoc/Apoc/Map/LABY01.map"),
        ];
        for path in samples {
            if !path.exists() {
                continue;
            }
            let source = Map::load_legacy(&path).expect("load legacy");
            let encoded = encode(&source).expect("encode");
            let src = std::fs::metadata(&path).expect("metadata").len();
            let dst = u64::try_from(encoded.len()).unwrap_or(0);
            println!(
                "{}: .map={src} .mnm={dst} ratio={:.3}",
                path.display(),
                dst as f64 / src as f64
            );
            assert!(
                dst < src,
                "{}: .mnm {dst} 不应大于 .map {src}",
                path.display()
            );
        }
    }
    #[test]
    fn map_load_prefers_sibling_mnm_over_legacy_map() {
        let source = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../resource/community/suprcode-crystal-database/cjlaaa/Maps/0100.map");
        if !source.exists() {
            return;
        }
        let dir = std::env::temp_dir().join(format!("mir-mnm-prefer-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("temp");
        let legacy = dir.join("0100.map");
        std::fs::copy(&source, &legacy).expect("copy map");
        let parsed = Map::load_legacy(&legacy).expect("legacy");
        std::fs::write(dir.join("0100.mnm"), encode(&parsed).expect("encode")).expect("write mnm");
        let loaded = Map::load(&legacy).expect("preferred load");
        assert_eq!(loaded.width, parsed.width);
        assert_eq!(loaded.height, parsed.height);
        assert_eq!(loaded.cells, parsed.cells);
        std::fs::remove_dir_all(&dir).ok();
    }
    #[test]
    fn map_load_accepts_mnm_path_directly() {
        let map = sample();
        let dir = std::env::temp_dir().join(format!("mir-mnm-direct-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("temp");
        let path = dir.join("direct.mnm");
        std::fs::write(&path, encode(&map).expect("encode")).expect("write");
        let loaded = Map::load(&path).expect("direct .mnm");
        assert_eq!(loaded.width, map.width);
        assert_eq!(loaded.cells, map.cells);
        std::fs::remove_dir_all(&dir).ok();
    }
    #[test]
    fn mnm_roundtrip_is_lossless() {
        let map = sample();
        let bytes = encode(&map).expect("encode");

        let decoded = decode(&bytes).expect("decode");
        assert_eq!(decoded.width, map.width);
        assert_eq!(decoded.height, map.height);
        assert_eq!(decoded.file_name, map.file_name);
        assert_eq!(decoded.version, map.version);
        assert_eq!(decoded.doors, map.doors);
        assert_eq!(decoded.cells.len(), map.cells.len());
        for (a, b) in decoded.cells.iter().zip(&map.cells) {
            assert_eq!(a, b);
        }
    }
}
