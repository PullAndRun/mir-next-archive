# 数据语言审计（默认全中文能否落地）

> 审计时间：2026-10-22 第 27 轮；**第一步（默认切 cjlaaa）已落地**，落地记录见 §11。
> 目标：判断「默认简体中文」在**数据文本**层能否落地，以及切换数据集的收益/风险。
> 候选：`resource/community/suprcode-crystal-database/{cjlaaa, Jev, Daneo1989, NightWolf}`。

## 0. 结论（TL;DR）

1. **配置层已经是中文**（`accounts.language DEFAULT 'zh-cn'`、`client_config.rs`、`LanguageCatalog::default()`、`server.rs:418` 语言包），缺的只是**数据文本**。
2. `cjlaaa` 是四个候选里**唯一的中文数据集**（物品 85.7% / 怪物 77.2% / 地图 98.2% / 任务 98.1% / 技能 72.9% 为中文），且本作能完整加载（DB v105、Envir/NPC 脚本/任务脚本/掉落表/地图全部可读）。
3. `Jev` 是英文数据集（怪物/NPC/地图/任务 0% 中文；物品里 166 条 CJK 只是 `(金龍)` 这类装饰性汉字）。`Daneo1989` 也是英文；`NightWolf` 版本 34 超出本作支持范围（60..=117），不可用。
4. **已采纳 A：默认数据集已切到 `cjlaaa`**（`gamedata.rs::DB_CANDIDATES` 首位，见 §11）。切换后地图/任务/NPC 对话/物品/怪物/技能大部分变中文；**NPC 名字仍是英文**（309 个里只有 14 个中文），这是仍会“中英混杂”的**唯一大项**，下一步做 **NPC 名字本地化层**（309 条映射，可复用 `mir-core::i18n`）。
5. **B（保留 Jev + 本地化层）不推荐**：要翻 1628 物品 / 555 怪物 / 375 NPC / 463 地图 / 154 任务 + 660 份 NPC 对话脚本，量级远大于换库；C（混合）不可取，两库索引/脚本文件名不对应（见 §6）。

## 1. 候选库总表（`Server.MirDB` 实测）

| 数据集 | 版本 | 物品 | 怪物 | NPC | 地图 | 任务 | 技能 | 本作可加载 | 备注 |
|---|---|---|---|---|---|---|---|---|---|
| `cjlaaa` | 105 | 1349 | 544 | 309 | 389 | 157 | 107 | ✅ | **中文库（当前默认）** |
| `Jev` | 111 | 1628 | 555 | 375 | 463 | 154 | 111 | ✅ | 英文库（兜底 / Jev 数据契约用例） |
| `Daneo1989` | 83 | 1348 | 506 | 293 | 400 | 157 | 105 | ✅ | 英文库 |
| `NightWolf` | 34 | - | - | - | - | - | - | ❌ | 版本不在 60..=117，解析直接失败 |

## 2. 名称语言（CJK 占比，按字符判断）

| 类别 | cjlaaa | Jev | 说明 |
|---|---|---|---|
| 物品 `ItemInfo.Name` | **1156 / 1349 = 85.7%** | 166 / 1628 = 10.2% | Jev 的 CJK 只是 `(金龍)YellowDragonBow` 这类韩服汉字装饰，不是中文化 |
| 怪物 `MonsterInfo.Name` | **420 / 544 = 77.2%** | 0 / 555 | cjlaaa 残留英文：`Clone`、`AssassinClone`、`BabyPig`、`Chick`、`Kitten` 等 |
| NPC `NpcInfo.Name` | **14 / 309 = 4.5%** | 0 / 375 | **两库 NPC 显示名都是英文**：`Assistant_Jane`、`Blacksmith_Smith`、`Merchant_Sarah`… |
| 地图 `MapInfo.Title` | **382 / 389 = 98.2%** | 0 / 463 | cjlaaa 例：`比奇省`、`皇宫`、`书店`；Jev：`BichonProvince` |
| 任务 `QuestInfo.Name` | **154 / 157 = 98.1%** | 0 / 154 | cjlaaa 例：`助理的要求`、`帮屠夫狩猎` |
| 任务分组 `QuestInfo.Group` | **155 / 157 = 98.4%** | 0 / 154 | cjlaaa 全为 `比奇省` 等中文 |
| 技能 `MagicInfo.Name` | **78 / 107 = 72.9%** | 0 / 111 | cjlaaa：`基本剑术`、`攻杀剑术`；Jev：`Fencing` |

cjlaaa 里仍为英文的名称（供后续本地化清单）：
- 物品 193 条：`RedJadeBelt`、`CheongboHelmet(ARCH)`、`TheRipper` …（多为韩服/后期内容，中文条目已覆盖经典 1.76 主线）。
- 怪物 124 条：`Clone`、`AssassinClone`、`CharmedSnake`、`BabyPig`、`Chick`、`Kitten`、`BabySkeleton`、`Baekdon`、`Wimaen`、`OlympicFlame` …
- 技能 29 条：与 Jev 相同的英文技能名（cjlaaa 未翻的部分）。
- NPC 295 条英文名：`Assistant_Jane`、`Blacksmith_Smith`、`Merchant_Sarah`、`Teleport_Gilbert` …（见 §5）。

## 3. `Envir/` 覆盖度

| 目录 | cjlaaa | Jev | Daneo1989 | 本作是否依赖 |
|---|---|---|---|---|
| `Drops/` | **583 个文件**（扁平，中文文件名） | 1641 个文件（`Misc/`、`Provinces/` 等子目录） | 517 | ✅ 掉落 |
| `NPCs/` | **326 个脚本** | 660 | 313 | ✅ NPC 对话/商店/任务 |
| `Quests/` | **157 个脚本** | 161 | 157 | ✅ 任务 |
| `Maps/` | **1817 个 `.map`，1.19 GB**（扁平） | 1705 个，365 MB（扁平 497 + 子目录） | 1607 | ✅ 地形（但生产默认 `map_root=resource/map`） |
| `Goods/`（回购 .msd） | 缺失 | 53 | 2 | ❌ 本作未实现回购 |
| `Events/` | 缺失 | 18 | 缺失 | ❌ 未使用 |
| `Values/` | 缺失 | 5 | 缺失 | ❌ 未使用 |
| `Configs/` | 30 个（含 `ExpList.ini`/`GuildSettings.ini`） | 31 个 | 有 | ✅（且 `exp.rs`/`guild.rs` **本来就优先 cjlaaa**） |

> cjlaaa 缺的 `Goods/Events/Values` 本作代码均不读取（回购未实现，grep 无引用），不影响可玩性。
> **Envir 引用完整性（实测）**：cjlaaa 的 309 个 NPC 脚本、157 个任务脚本、544 只怪的掉落文件 **0 缺失**；Jev 同样 0 缺失。两个 variant 都可以“整套直接用”。
> `cjlaaa/Configs/ExpList.ini` 与 Jev **逐行相同**（500 级经验曲线一致）；`Language.ini`/`GoodsSystem.ini` 也相同；`GuildSettings.ini` 有差异（Compare-Object 计数 162 行），但 `guild.rs` 候选顺序已优先 cjlaaa，所以现状不变。

## 4. NPC 脚本语言（3 个抽样）

| NPC（DB 显示名） | cjlaaa 脚本 | `[@Main-1]` 原文（中文） | Jev 脚本 | 原文（英文） |
|---|---|---|---|---|
| `Assistant_Jane` | `BichonProvince/BorderVillage/0Helper-0` | `你好 <$USERNAME>, 这是你的首次冒险吗? 请按 ('H') 来打开帮助菜单.` | `BichonProvince/BorderVillage/Jane` | `Hello <$USERNAME>, Are you a beginner in this game? …` |
| `Blacksmith_Smith`（cjlaaa 名 `Merchant_Smith`） | `BichonProvince/BorderVillage/Blacksmith-0` | `你好，请问你需要些什么？ <浏览/@BuySell> 商店. <修理/@Repair> 武器.` | `BichonProvince/BorderVillage/Blacksmith` | `Welcome. Thanks for dropping in. … <View/@BuySell> Store.` |
| `Merchant_Sarah` | `BichonProvince/BichonWall/Butcher-0` | `欢迎，我能为你做些什么? <卖/@Sell> 肉. <询问/@肉help> 如何获得肉.` | `BichonProvince/BichonWall/Butcher1` | `Welcome, what can I do for you? <Sell/@Sell> Meat. …` |

> 结论：**cjlaaa 的 NPC 对话是中文**；但同一 NPC 在两个库里的**脚本文件名不同**（`0Helper-0` vs `Jane`、`Blacksmith-0` vs `Blacksmith`、`Butcher-0` vs `Butcher1`），所以 **DB 与 Envir 必须整套来自同一个 variant**，不能只换 `Server.MirDB`。
> 另外 cjlaaa 脚本里有中文按钮 key（如 `@肉help`），本作 `normalize_key` 用 Unicode `to_uppercase`，可正常匹配；但客户端回包需按原样 UTF-8 字符串，切换后应纳入一次真机点击验证。

## 5. 掉落表路径

| 项 | cjlaaa | Jev |
|---|---|---|
| `MonsterInfo.DropPath` | **全部为空（0/544）** | 449/555 非空，且 449 条全是 Windows 反斜杠 |
| 解析方式 | 空 → 回退怪物名 → `<中文名>.txt` | `Misc\ArcherGuard`、`Provinces\Hen` → 子目录 |
| 目录结构 | `Drops/` **扁平**，583 个中文名文件 | `Drops/` 含 `Misc/`、`Provinces/`、`AncientCaves/` 等子目录 |
| 反斜杠问题 | **不存在**（无 DropPath） | 已修（`gamedata.rs::normalize_drop_path`） |
| 实测可加载 | 544 只怪的文件全部存在；其中 **401 只**有非空规则，`鸡`/`鹿`/`稻草人` 均中文规则、含 `Gold` 行 | 555 只怪文件全部存在；其中 **377 只**有非空规则 |
| 样例 | `鸡.txt`：`1/5 鸡肉`、`1/15 鸡肉`、`1/20 Feather`；`稻草人.txt`：`1/10 Gold 50` | `Provinces/Hen.txt`：`1/1 Chicken`、`1/20 Feather` |

> Jev `Drops/` 根目录的 `Hen.txt`/`Deer.txt`/`Scarecrow.txt` 仍是**未被引用的空壳**（真实规则在 `Provinces/`），排查时别拿错；cjlaaa 没有这个陷阱。

## 6. 可加载性验证（实测）

- `GameData::load(cjlaaa/Server.MirDB)` 成功：map0 标题 = `比奇省`；经验表来源 cjlaaa（100/200/300…，与 Jev 相同）。
- 怪物 `鸡`、`鹿`、`稻草人` 的掉落表均能加载（401 只怪非空）；中文物品名 `鸡肉` 可查。
- NPC 脚本 `BichonProvince/BorderVillage/0Helper-0` 可加载；任务 1（`助理的要求`，文件 `1`）脚本可加载且正文为中文。
- 地图抽查：`0.map` 700×700（walkable 326,596）、`0103` 27×29、`0104` 13×16、`D001A` 400×400、`bonguk1` 300×300 全部可加载。
- 三份 `0.map`（`resource/map`、cjlaaa/Maps、Jev/Maps）都是 700×700，可走格数量几乎一致（327,180 / 326,596 / 326,625）→ **地形本身不是切换风险**，差异在 DB 的传送点/NPC/刷怪点数据。

## 7. 方案 A（已落地：默认切 cjlaaa）

### 7.1 切法（不改代码也能先验证）
- 环境变量：`MIR2_DB_PATH=resource/community/suprcode-crystal-database/cjlaaa/Server.MirDB`
  → `GameData::load` 会从**同目录**推导 `Envir/`（NPC 脚本/任务/掉落都来自 cjlaaa），与 DB 不混套。
- 代码默认：把 `crates/mir-server/src/gamedata.rs` 的 `DB_CANDIDATES` 首位从 Jev 换成 cjlaaa（下轮改；本轮不动）。
- `map_root` 默认仍是 `resource/map`（见风险 §8.3）。

### 7.2 切换后预期
- ✅ 地图标题、任务名/任务正文、NPC 对话、物品名（约 86%）、怪物名（约 77%）、技能名（约 73%）变中文；经典主线内容基本全中文。
- ⚠️ **NPC 名字仍是英文**（`Assistant_Jane` 等 295 条）——视觉上还会中英混杂，建议下一步做 309 条 NPC 名字映射（可放 `mir-core::i18n` 或独立 `NameLists`）。
- ⚠️ 193 条物品 / 124 条怪物 / 29 条技能仍是英文（多为后期/韩服内容）。

## 8. 切换风险清单（按影响排序）

1. **测试/工具写死 Jev 数据（最大）**
   - `GameData::discover()` 共 **28 处调用 / 19 个文件**（12 个测试 + 4 个 examples + 3 个 src）；把默认库换成 cjlaaa 后这些用例读到的物品名/索引/任务编号/地图标题都会变。
   - 仓库里 `Jev` 字样 **101 处 / 38 个文件**（不含本审计文档）；其中 **4 个文件、5 处提到 Jev DB 绝对路径**（`gamedata.rs:53` 候选表、`examples/quest_levels.rs:8`、`tests/common/mod.rs:76` 注释、`tests/gameshop.rs:1243/1275`），其余为文档引用。
   - 本单新增的 `tests/gold_drop.rs`（Hen/Chicken）与 `tests/npc_shop.rs`（Merchant_Sarah 仍在 cjlaaa，但 Chicken/Hen 不在）在切库后需要中文名兼容（`鸡`/`鸡肉`）或显式钉住 Jev。
   - 建议：把“真实数据用例”统一改成 `GameData::load(<显式 variant 路径>)`，默认库只用于生产；否则每次换库都要改测试。
2. **地图覆盖**：cjlaaa DB 声明 389 张图，`resource/map`（默认 map_root）只有 **298 张**（缺 91：`D001A`、`bonguk*` 等副本）。现状 Jev 是 295/463（缺 168），所以 A 反而**减少**缺失；要 100% 覆盖需把 `map_root` 指向 `cjlaaa/Maps`（389/389，1.19 GB，比 `resource/map` 的 712 MB 多约 480 MB），抽查可加载。
3. **传送点/走位坐标变了**：map0 传送点 cjlaaa 70 条 vs Jev 72 条；去 0104 的入口 cjlaaa `(323,247)` vs Jev `(322,247)`；去 0122/0108 的目的地也不同。`tools/playtest/gates.json`、`docs/world/quest-nav.md`、`newbie_flow.rs` 的锚点必须按 cjlaaa 重新生成/核对。
4. **内容量差异**（cjlaaa v105 vs Jev v111）：物品 -279（1349 vs 1628）、怪物 -11、NPC -66、地图 -74、刷怪点 -741（5600 vs 6341）、传送点 -140、商城 -23（82 vs 105）；任务 +3（157 vs 154）、技能 -4（107 vs 111）。后期内容（韩服新图/新装备）会缺。
5. **任务/新号流程**：cjlaaa 任务名/编号/脚本文件名与 Jev 不同（如任务 1 = `助理的要求`）；`quests.rs`/`quest-bind-audit`/`newbie_flow` 的 Jev 锚点需重验；NPC 位置也有 1 格级差异（Smith `(297,612)` vs `(296,613)`，Sarah 相同）。
6. **DB 与 Envir 必须整套切换**：两库 NPC 脚本文件名不同（§4）；只换 DB 或只换 Envir 会大量 `NoScript`/掉落缺失。
7. **中文文件名**：cjlaaa 的 `Drops/鸡.txt`、`NPCs/.../月饼采购员-0.txt` 在 Windows 正常；Linux/Docker 需确认归档解压后的 UTF-8 文件名未被转码（容器内 `MIR2_DB_PATH` 指向 cjlaaa 时一并验证）。
8. **商城内容变化**：cjlaaa `GameShop` 82 条 vs Jev 105 条（hubu 的真实 TCP 商城用例显式指向 Jev，测试不受影响；生产商店会少 23 件）。
9. **配置层**：`ExpList.ini`/`Language.ini` 一致，`GuildSettings.ini` 不同但本作已优先 cjlaaa，切库不改现状；`Setup.ini` 有 330 行差异，与游戏逻辑无关。
10. **过时注释**：`gamedata.rs` 的 `DB_CANDIDATES` 注释目前写「cjlaaa v105 只有数据库文件、缺 Envir」「Jev 最完整」——这与当前 `resource/` 不一致（cjlaaa 已带完整 `Envir/`，Jev 的 NPC/掉落文件反而更旧），切换时一并更新注释与 `docs/store/mirdb-format.md` 的样本描述。

## 9. 方案对比

| 方案 | 收益 | 成本/风险 | 结论 |
|---|---|---|---|
| **A 默认切 cjlaaa** | 地图/任务/NPC 对话/物品/怪物/技能大部分中文；Envir 完整；无代码改动可先用 `MIR2_DB_PATH` 验证 | 测试锚点/坐标需重验；NPC 名字仍英文；后期内容少 | **推荐** |
| A+（切 cjlaaa + NPC 名字映射 + 补 193 物品/124 怪物/29 技能） | 真正「全中文」 | 309 条 NPC 映射 + 346 条名称翻译 | A 的后续步骤 |
| B 保留 Jev + 名称本地化层 | 不动数据/坐标/测试 | 需翻 1628+555+375+463+154 名称 + 660 份 NPC 脚本对话，量级最大；任务/NPC 文本无法只靠名称层 | 不推荐 |
| C 混合（cjlaaa DB + Jev Envir 或反之） | — | 脚本文件名/索引不对应，`NoScript`/掉落/任务错配 | 不推荐 |

## 10. 建议的落地顺序

1. 用 `MIR2_DB_PATH=.../cjlaaa/Server.MirDB` 起一台真服，跑 `newbie_flow`/商店/传送点，确认中文效果与坐标差异（不改默认）。
2. 把真实数据用例改为“显式指定 variant”，避免默认库变化打断测试。
3. 改 `DB_CANDIDATES` 首位 → cjlaaa；`map_root` 视情况改 `cjlaaa/Maps` 或补 91 张缺失图。
4. 做 309 条 NPC 名字映射（+ 可选补 193/124/29 条名称），把“中英混杂”收敛到 0。

## 11. 第一步落地记录（第 27 轮）

### 11.1 代码与默认值
- `crates/mir-server/src/gamedata.rs::DB_CANDIDATES` 顺序改为 **cjlaaa → Jev → Daneo1989**；过时注释（“cjlaaa 缺 Envir / Jev 最完整”）已更新为当前事实。
- 新增单测 `default_dataset_is_the_chinese_cjlaaa`：默认库路径含 `cjlaaa`、map0 标题中文、物品/怪物名过半中文、`Assistant_Jane` 脚本对话含中文。
- `windows_style_drop_path_loads_the_jev_table` 显式钉 Jev（验证反斜杠 DropPath 契约）。
- `map_root` 未改，仍是 `resource/map`（cjlaaa 389 张图覆盖 298 张）。

### 11.2 测试分组（Jev 契约 vs 默认 cjlaaa）
- 新 helper：`crates/mir-server/tests/common/mod.rs` 的 `dataset_db_path()/load_dataset()/default_data()/jev_data()`。
- **显式钉 Jev**（验证 Jev 数据契约，不随默认数据集漂移）：
  - `tests/gold_drop.rs`（Hen/Chicken/Scarecrow0）、`tests/npc_shop.rs`（Chicken/Butcher1）；
  - `tests/quests.rs` 4 条 `real_*`（ForestYeti/Smith/Luke/任务 5/154）；
  - `tests/npc.rs` 2 条 `real_*`（比奇真实 NPC/商店）；
  - `tests/conquest.rs::real_database_exposes_a_complete_conquest_record`（Sabuk Wall）；
  - `gamedata.rs::windows_style_drop_path_loads_the_jev_table`。
- **走默认 cjlaaa（数据驱动）**：其余全部用例；`tests/maps.rs::player_can_walk_from_the_plaza_into_a_natural_teleport_point` 的 BFS 改为排除 NPC/存活怪物占格（cjlaaa 的 NPC 布局会挡路），仍用 `resource/map` 地形。
- 结果：`cargo test --workspace --no-fail-fast` **全绿**（PG 用例需 `MIR2_TEST_DATABASE_URL`，用 `mir2_test` 验证）；`clippy --workspace --all-targets -D warnings` 0；`cargo fmt --all --check` 0；i18n 棘轮 69/70。

### 11.3 端到端自查（真服，默认数据）
启动 `target/debug/mir-server.exe`（**不设** `MIR2_DB_PATH`，走默认候选）：
- 日志：`已加载 Server.MirDB path=.../cjlaaa/Server.MirDB monsters=544 items=1349`；`出生地图已初始化 map=0 npcs=45 groups=358 spawned=2252`。
- 新角色进图：`S.NewMapInfo` title=`比奇省`；系统聊天=`欢迎来到传奇-Next。` + 新手提示；视野怪物=`皇城弓箭护卫`/`皇城卫士`（中文）；走到最近 NPC 对话正文为中文。
- `Assistant_Jane` 的中文对话由 `default_dataset_is_the_chinese_cjlaaa` 单测覆盖（脚本 `0Helper-0`）。

### 11.4 受影响的走位/锚点清单（只列，未改脚本）
- `tools/playtest/gates.json`（94 条）：map0 72 条里 **30 条源坐标在 cjlaaa 不存在**（含 `(322,247)->0104@4,10`——cjlaaa 实际是 `(323,247)`；`(347,189)->0122`；`(286,296)->0101`；`(315,613)->0121`；`(414,243)->0137`；`(324..326,40..43)->1`；`(667,88)->2`；`(322/323,473/474)->01041`；`(147/148,33/34)->D001`；`HELL00` 六连等），另有 **18 条目的地坐标漂移**；其余 22 条（0101..0108 回程）也需按 cjlaaa 重新生成。
- `tools/playtest/playtest.py`：`(322,247)` 常量（`:371`）、书商目标（`:2076-2078`）、`self_check` 期望串 `(326,288)->0108(8,12)`（`:4010`，cjlaaa 为 `(10,12)`）、回程表（`:1923-1925`）、锚点（`:1949`）。
- `crates/mir-server/tests/newbie_flow.rs`：直接走 `0104.map` + `Librarian_Steven(5,8)`（cjlaaa 同位置，已验证；不依赖 Jev movement）。
- `docs/world/quests.md:337-354`：quest-nav 的 pickup/handin 坐标表来自 Jev，需按 cjlaaa NPC 位置重算。
- `docs/world/quest-nav.md` 只有格式说明，无坐标。

### 11.5 blocker 修复：cjlaaa 缺 bigmap flags → `S.NewMapInfo` 按图回退

- **数据事实**：cjlaaa v105 全库 `NpcInfo.ShowOnBigMap=false`（309/309）、
  `MovementInfo.ShowOnBigMap=false`（1859/1859）、`BigMapIcon=0`；Jev v111 则有
  65/375 个 bigmap NPC、450/1999 条 bigmap movement。
- **影响**：Crystal `CheckMapInfo`（`PlayerObject.cs:1030/1047`）只下发 bigmap 标记项，
  所以 cjlaaa 下 `S.NewMapInfo` 的 NPC/movements 都是 0 → 客户端大地图、远端 NPC 列表、
  任务导航目标不可见（视野内 `S.ObjectNPC` 仍正常，世界里有 45 个 NPC）。
- **修复**（`npc.rs::map_entry_info`）：**整张图一条 bigmap 标记都没有时**，回退为该图
  全部 NPC / 全部 movements；只要图里有任意标记（Jev），行为与 Crystal 完全一致
  （Jev map0 仍是 40 NPC / 6 movements）。这是「数据缺 flags 的兼容回退」，不改 `mir-db` 解析。
- **单测**：`crates/mir-server/tests/npc_map_info.rs`
  - `new_map_info_falls_back_to_all_when_bigmap_flags_missing`（全 false → 2 NPC / 2 movements）；
  - `new_map_info_keeps_crystal_filter_when_flags_present`（Jev 真实数据 → 46 刷出 / 40 NPC / 6 movements）。

### 11.6 下一步
按队长计划做 **NPC 名映射**（309 个显示名，295 个英文）：数据源优先 `NpcInfo.Name` + 客户端字符串表/`Envir/NameLists`，产出 `config/lang/*.json` 的 `npc_names` 段，服务端 `S.ObjectNPC` 按会话语言渲染。
