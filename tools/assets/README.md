# tools/assets：原生玩家库导入

本目录的 `make_native_player_plan.py` 生成 Crystal classic 原生玩家库计划，覆盖
C（战/道/法）、A（刺客）、AR（弓手）三域的 `armour + hair + weapon`。翅膀库
（`CHumEffect/AHumEffect/ARHumEffect`）本轮明确后置，不导入。

## 计划

```bash
py tools/assets/make_native_player_plan.py \
  --out target/native-layout/native-player-plan.json
```

脚本按“域前缀 + 规范化 stem”生成唯一键，例如：

- `CArmour/00.Lib` → `carmour-00`
- `AWeapon/00 L.Lib` → `aweapon-00-l`
- `ARWeapon/00 S.Lib` → `arweapon-00-s`

同一批 262 个 `.Lib` 只用这份计划，不再靠 `file_stem` 猜目录。`assets unpack --plan`
也保留缺省回退：没有 `library_key` 的旧计划仍使用规范化 `file_stem`，但同 key 指向
两个不同库时会直接报错。

## 导入

先构建 release，避免 debug 解包 25 万帧耗时过长：

```bash
cargo build -p mir-tools --release
target/release/mir-tools.exe assets unpack \
  --plan target/native-layout/native-player-plan.json \
  --out-root game-assets --format png
```

输出是 `game-assets/gfx/sprites/player-native/<library_key>/index.json + *.png`。根目录
`game-assets/library-map.json` 会同时把原生键映射到这些目录；`index.json` 记录
`library_key`、源 `.Lib` 文件名和每帧全局索引。

## manifest 与 pack

`build_manifest.py` 需要带 `outputs[]` 的合并计划（现有资产 + 262 条原生 output），
可在 `target/native-layout/` 里把原生条目并入 `target/used-assets.json` 后执行：

```bash
py tools/assets/build_manifest.py --plan target/native-layout/manifest-plan.json \
  --game-assets game-assets --verify-json ''
target/release/mir-tools.exe assets pack \
  --input game-assets --out game-assets/packs --chunk-mb 16
```

`game-assets/packs/library-map.json` 与根 `library-map.json` 使用同一键；原生库不带
`dirs`，只带 `prefix`，运行期按 key 走 pack。

## 布局证据

`crates/mir-render/src/player_layout.rs` 是帧表/偏移/选库域；`NativeFrameTable` 的
常量逐项注释了 `github/Crystal/Client/MirObjects/Frames.cs` 行号。Attack2 的经典
hum 物理位置证据由 `mir-tools assets unpack --lib resource/data/hum.wzl --indices
200-391` 生成，记录在 `target/native-layout/attack2-evidence.json`，不写入 docs。
