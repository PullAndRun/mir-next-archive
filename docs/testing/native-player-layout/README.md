# R48 A1 · 客户端原生选库 + Crystal 选招证据

本目录只放 **R48 A1** 的证据，不覆盖 `docs/testing/attack-variants/`（R47 冻结证据）。
实现范围：客户端/渲染，不含协议、服务端、数据库或素材格式变更。

## 1. 结论

* `PlayerLayout` 按 Crystal `PlayerObject.cs` 逐帧解析原生玩家库：
  * 战/道/法恒走 C 系（`carmour/cweapon/chair`）；
  * 刺客 `altAnim` 才走 `aarmour/ahair`，否则 C 系；
  * 弓手 `altAnim` 才走 `ararmour/arhair`，否则 C 系；
  * 武器**不随** `altAnim` 换域，按 `Weapon / 100` 单独选域（普通
    `CWeapons`、刺客 `AWeaponsR/L`、弓手 `ARWeapons/ARWeaponsS`）；
  * 身体/头发域随 `altAnim`；翅膀的**库域**也随 `altAnim`，但帧偏移独立。女性偏移
    **不是统一 +512**，由 `NativeLibraryId` 按域/`altAnim` 给：
    C 系身体/头发 808、武器 416、翅膀 840；刺客 alt 512/512/544；弓手 alt
    352/352/352；男性 0/0/0。偏移加在库内物理帧号上，`Start` 不含偏移。
* 选招按 `PlayerObject.cs:1055-1075` 的 **惰性 Next(100) 顺序**：
  * 默认职业：Shift + 无目标时 `Next(100) >= 20` → A1，否则 A3；其余 A1；
  * 刺客：DoubleSlash 优先且 0 抽（生产恒 false）；Shift 时外层
    `Next(100) >= 20`，内层惰性 `>40 ? A1 : A4` / `>10 ? A2 : A3`，恰好 2 抽；
    非 Shift `Next(100) >= 40 ? A1 : A4`，恰好 1 抽；
  * 弓手 0 抽，A1；
  * Attack4 的 `TwinDrakeBlade/FlamingSword` 招式映射由
    `native_attack::select_attack4` 表达；当前协议无 Attack4/Spell 字段，
    生产不调用，属显式 PARTIAL。
* 帧表和时长都来自 `NativeFrameTable`：A1 `136/6`、A2 `184/6`、
  A3 `232/8`、A4 `416/6`，每帧 100ms；女性偏移由 `NativeLibraryId` 按上述
  四域分别加在库内索引上，不写死 600/300，也不是所有域统一 +512。
* `net::entity_views` 把服务端的 `class/gender/weapon_shape` 写进 `EntityView`，
  并只对 `Attacking` 给出 `native_action`；远端只映射已有 A1/A3 变体，A2/A4
  不经过网络。`EntityRenderer` 走原生路径；**身体库键/库不存在时整个玩家
  回退旧扁平路径**，身体库存在但当前动作/方向身体帧缺失时画 28×40 品红占位、
  仍留在原生路径；头发/武器/翅膀单层缺帧才安静跳过，不跨库回退成
  Standing/Attack1。
* 骑乘或没有原生身体库时保留旧扁平路径；这是显式 fallback，不是 native 缺帧回退。
* 原生层序按 Crystal `PlayerObject.cs:4884-4923` 的 8 方向实现：身体前
  主/副手 → 身体 → 头发/翅膀（朝上组先头发、朝下组先翅膀）→ 身体后主/副手。
  弓手 `WeaponLibrary1=null`，AR 弓只在 `WeaponLibrary2`（Off）；Crystal 在所有
  方向的方向分支之后无条件再画一次 `DrawWeapon2`，所以左组末尾出现两次 Off、
  右组身体前一次/末尾一次。`draw_order` 纯函数单测覆盖 8 方向，并用
  `PlayerLayout::for_action` 实际组合断言普通 Attack1(alt=false) 与
  WalkingBow(alt=true)。没有盾牌层；骑乘不进这条原生层序，整体走旧扁平路径
  （见上）。本作未复刻 Crystal 对本地 `User` 跳过翅膀的分支，本地玩家也画翅膀。

## 2. 逐帧真实素材证据

`crates/mir-render/tests/native_player_evidence.rs`（ignored 证据测试）加载
`game-assets` 的 **默认 auto → dev/unpacked** 路径，使用真实地图
`game-assets/maps/0.mnm` 和 `game-assets/gfx/sprites/player-native`，对

* 战 A1–A4；
* 法/道 A1、A3（Crystal 该两职业只定义 A1/A3）；
* 刺客 alt A1–A4；
* 弓手 A/AR 专库人工样本 A1–A3；
* 弓手**生产语义** A1（`PlayerLayout::for_action`，`alt=false`，C 身体 + AR 武器）
  Up/Left/Down/Right 四方向，男女各一组；
* 弓手**生产语义** WalkingBow（`alt=true`，AR 身体 + `ARWeaponsS`）Left/Right。

的男/女每一帧渲染并落 PNG，同时校验每个身体帧 `info.is_empty() == false`
（不是 Standing 占位）。注意：这是渲染默认路径证据，不是网络 live 证据；
网络路径由 client 单测覆盖（见 §3）。

生成：

```bash
TEMP=target/tmp TMP=target/tmp cargo test -p mir-render \
  --test native_player_evidence -- --ignored --nocapture
```

本次 manifest：`manifest.json`（batch `1792827856831`），272 个 sample：15 个
职业/动作人工分支（战 A1–A4、法/道 A1+A3、刺客 A1–A4、弓手 A/AR A1–A3）× 男/女
逐帧，加弓手生产 Attack1 四方向 × 2 性别、WalkingBow 两方向 × 2 性别。**样本构造**：
人工 A/AR 样本用 `PlayerLayout::new(...)` 指定 `alt_anim`，用于单独证明专库帧表；
生产样本必须走 `PlayerLayout::for_action`：普通 `Attacking → Attack1` 得
`alt=false`、C 身体/头发 + AR 武器（Off、无 `-s`），`Walking → WalkingBow` 得
`alt=true`、AR 身体/头发 + `ARWeaponsS`。manifest 对每个样本记录
`production`、`direction`、`alt_anim`、`body_library`/`body_source`、
`hair_library`、`weapon_library`/`weapon_hand`、`draw_order`。已知源素材缺帧用
`weapon_frame_missing` 显式记录；生产普通弓手样本断言 `body_library` 以
`carmour` 开头、`weapon_library` 以 `arweapon` 开头且 `weapon_hand="off"`。
本次 `unique_png_sha256=216`、`unique_character_crop_sha256=216`
（216 > 200；跨职业同 body/武器的帧允许重复，同 label 相邻帧已被断言哈希不同）。
测试同时断言头发层非空，武器层仅在已知女性 C 系 A4 越界时为
`weapon_frame_missing=true`，其余动作/职业必须非空。本目录共 274 个文件：
`README.md` + `manifest.json` + 272 张 PNG；PNG 的 SHA-256 已按 manifest 逐张重算。

真实素材限制（源素材本身，不是导入缺陷）：

* 女性 C 系 `cweapon-00`：库 832 帧，武器偏移 416，A4 的 6 帧（416..421 加偏移后
  为 832..837）越界；渲染安静跳过武器层，身体仍正常，不回退其他武器/Standing。
* 弓手不使用 A4；A1–A3 身体/弓库完整。
* 刺客 alt 专库 `aarmour-00` 1024 帧、`aweapon-00-r/l` 1024 帧完整。
* 普通（非 alt）刺客/弓手走 `carmour-00`（1616 帧）/`cweapon-00`（832 帧），
  `aarmour/ararmour` 的 1024/704 帧不会被误用。

## 3. 测试覆盖

| 测试 | 覆盖 |
|---|---|
| `mir-render --lib player_layout`（10） | 域切换、库键、offset、身体/effect 帧表、`altAnim`、翅膀库号/WingOffset、8 方向层序 |
| `mir-render --lib entities`（新增 1） | 原生身体库存在但精确帧缺失仍留原生路径、画占位、不回落 flat |
| `mir-render --lib native_attack`（9） | 默认 Shift A1/A3（40 次固定采样双覆盖）、刺客 Shift 链 A2/A4、非 Shift 1 抽、弓手 0 抽、Attack4 spell 映射、惰性抽取顺序 |
| `mir-render --test native_player_layout`（1 常规 + 4 ignored） | 稳定库键；A1–A4 真实身体帧非空、A/AR/C offset、库键无等级后缀、release pack 解码 |
| `mir-render --test native_player_evidence`（1 ignored） | 272 帧默认路径 PNG + manifest，身体/头发/武器层逐帧断言 |
| `mir-client --lib native_attack_keeps_class_gender_and_frame_through_entity_views` | `class/gender/native_action` 穿过 `entity_views`，A4 416/6、600ms |
| `mir-client --test attack_variant`（12 + 1 ignored） | R47 旧 A1/A3 不回归、普通 50ms/300ms 路径 |
| `mir-client --test anim`（6） | 旧扁平动画/R47 时序不回归 |

`DoubleSlash` 生产恒 false：协议没有技能/状态字段；`skills` 枚举拿不到。
`Attack4` 双持/技能映射：测试可达，生产协议无 Attack4/Spell，标 PARTIAL。
**未做窗口启动 live**：本轮证据是默认 `game-assets` 路径的渲染/选择证据；
未声称真实 TCP 战斗或窗口 40 次采样已跑，留给审核决定是否补。

## 4. 验证命令

```bash
TEMP=target/tmp TMP=target/tmp cargo test -p mir-render --lib player_layout
TEMP=target/tmp TMP=target/tmp cargo test -p mir-render --lib native_attack
TEMP=target/tmp TMP=target/tmp cargo test -p mir-render --test native_player_layout -- --ignored --nocapture
TEMP=target/tmp TMP=target/tmp cargo test -p mir-client --test attack_variant
TEMP=target/tmp TMP=target/tmp cargo test -p mir-client --lib native_attack_keeps_class_gender_and_frame_through_entity_views
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
```

不要运行会写 `docs/testing/attack-variants/` 的 R47 live 测试来生成本目录证据；
本目录的测试路径独立，`git status` 不应出现 `docs/testing/attack-variants/` 改动。

## 5. 本次修复后的实测结果

* `player_layout` 10 passed；`native_attack` 9 passed；`native_player_layout`
  4 个 ignored 用例显式运行通过；`attack_variant` 12 passed + 1 ignored；
  native→entity_views client 单测 1 passed。
* `cargo test -p mir-client -p mir-render`：**558 passed / 0 failed / 7 ignored**。
  该命令中的 R47 live 用例会写 `docs/testing/attack-variants/`，跑完已用
  `git checkout -- docs/testing/attack-variants` 恢复，收尾工作区该目录 0 改动。
* `cargo fmt --all -- --check`、`cargo clippy --workspace --all-targets -- -D warnings`
  和 `py tools/docs/check_links.py` 均通过（见下方交付回执的实际数字）。
* 证据 batch `1792827856831`：272 samples、`unique_png_sha256=216`、
  `unique_character_crop_sha256=216`；272 张 PNG 的 SHA-256 已逐张复核一致。
* 本次重跑了 `native_player_evidence`（33.58s）和 `native_player_layout` 的
  4 个 ignored 用例；上面的测试计数来自修复后的完整套件。
