# 传奇-Next

[![License: GPL-3.0-or-later](https://img.shields.io/badge/license-GPL--3.0--or--later-blue.svg)](LICENSE)
![Rust](https://img.shields.io/badge/rust-stable-orange)
![Status](https://img.shields.io/badge/status-playable-orange)

用 Rust 重写的《热血传奇 2》（Legend of Mir 2）服务端、客户端与配套工具链。

## 项目简介

传奇-Next（仓库名 `mir-next`）是《热血传奇 2》的开源重制项目。项目以 LOMCN 社区维护的开源实现 [Crystal](https://github.com/Suprcode/Crystal)（C#）作为玩法与协议的参照系，用 Rust 从零实现服务端、客户端与工具链，目标是在保留原作玩法体验的前提下，获得更好的性能、可维护性与跨平台部署能力。

> **当前阶段：可联机游玩的服务器 + 客户端。** 已经做到：Crystal 的 **432 个数据包全部覆盖**
> （408 个由 C# 源码自动生成、24 个手写），服务端能完成「连接 → 版本校验 → 注册/登录 →
> 建角/删角 → 进入世界 → 转身/走跑/聊天 → 退出」整条链路，行走走的是**真实 `.map` 地形**
> （Crystal 的 9 种地图版本全部支持），账号与角色可存 PostgreSQL，世界里的怪物来自
> **Crystal 自己的 `Server.MirDB` 数据库**（真实怪物属性 + 刷怪点），已能索敌、追击、出手、
> 掉血、掉落、拾取、升级、死亡复活并广播全套战斗与物品包；NPC 商店、任务、组队、行会、
> 玩家仓库、邮件、玩家间交易与 NPC 寄售/拍卖市场均已在服务端落地，其中仓库、邮件与市场挂单
> 可持久化；客户端素材层能解码 `.Lib`/`.wzl` 图库（已用真实素材验证），联机渲染与中文 UI
> 已覆盖状态栏、聊天、背包、纸娃娃、商店、对话、技能、buff、小地图/大地图、邮件、仓库、
> 行会、组队、交易与市场面板。AOI 网格化、视野 16 与门状态机已落地，待刑部全量验收；攻城建筑、
> 部分高级玩法与客户端细节（见 [`docs/client.md`](docs/client.md) §8）尚未完成。

## 项目目标

- **玩法对齐**：地图与行走、战斗与技能、装备与强化、NPC 商店与修理、掉落与经济、任务、组队、行会、攻城、PK 与红名，均以 Crystal 的行为作为基准逐项对齐。
- **协议对齐**：数据包编号、字段顺序与序列化规则与 Crystal 保持一致，使现有 Crystal 客户端可以直接连接本项目服务端。
- **数据对齐**：能够读取 Crystal 生态的地图文件、素材库与数据库结构，降低素材与存档的迁移成本。
- **全中文化**：默认简体中文，文本与地图名走 `config/lang/` 语言包；游戏内 `/language en` 可切换，语言随账号保存（见 [`docs/i18n.md`](docs/i18n.md)）。
- **工程化**：结构化日志、覆盖核心规则的自动化测试、单一可执行文件部署。

## 当前状态

| 模块 | 状态 | 说明 |
| --- | --- | --- |
| 仓库初始化、许可证、README | ✅ | |
| Cargo workspace 与 CI | ✅ | `cargo fmt` / `clippy` / `test` + 协议生成一致性检查 |
| 协议层（`mir-protocol`） | ✅ | **432/432 个包全覆盖**：408 个由 C# 源码自动生成 + 24 个手写；每个包都有 encode→decode 往返测试 |
| 登录网关与会话（`mir-server`） | ✅ | 版本校验、注册、登录、建角/删角、进世界、聊天、语言切换 |
| 地图与行走（`mir-world` / `mir-server`） | 🟡 | `.map` 9 种版本读取 + 可走性校验；动作计时按 Crystal（走/跑 600ms、转身 350ms、跑需先走一步）、`UserLocation`/`ObjectWalk`/`ObjectRun` 广播、世界 tick 100ms；视野已对齐 16 格（`DataRange`）并接入 8×8 桶 AOI 网格、进出视野差分与门状态机（5 秒自动关）；本轮端到端测试见 `tests/aoi.rs`，待刑部全量验收 |
| 账号与角色存档（`mir-store`） | ✅ | PostgreSQL + 迁移；内存实现用于无库试玩 |
| 游戏数据（`mir-db`） | ✅ | Crystal `Server.MirDB` 全量读取（地图/物品/怪物/NPC/任务/技能/商城/攻城/刷怪点），版本 60..117，6 个真实样本 leftover=0 |
| 怪物 AI 与战斗 | ✅ | 索敌/追击/巡逻、两段式攻击（动作包立即、伤害 300ms 后结算）、护甲与闪避、暴击、死亡与尸体回收、经验升级、死亡复活、自然回血，RNG 顺序与 Crystal 一致 |
| PK 与攻击模式 | ✅ | Peace/Group/Guild/RedBrown/All 判定、安全区与 NoFight 禁打、PvP 300ms 结算与暴击、PK 值（击杀 +100、红名 200/黄名 100、12 秒衰减、棕名 60 秒）、名字颜色广播、死亡掉落（红名背包全掉）、守卫只打红名并可回位 |
| 攻城（沙巴克） | 🟡 | 申请校验、时间表开战/停战、四种城主判定（Classic/KingOfHill/ControlPoints/CapturePalace）、夺城广播与城主税收、战区名字色与 PK 保护、攻城传送点城主专属、箭塔只射敌对行会、迁移 0008 持久化；城门/城墙/器械实体与旗子对象未做 |
| 技能（魔法） | 🟡 | 技能数据/用书学习/施法校验/延迟结算/修炼值升级/存档；已实现 15 个伤害/治疗/位移/召唤/中毒技能（火球系列/HellFire/FireBang/治疗/火墙/雷电/瞬息移动/隐身/召唤骷髅/召唤神兽/施毒）与 9 个增益技能（护盾/加速/魔法盾…），其余登记未实现（`docs/world/magic.md` §9） |
| 物品与背包 | ✅ | 掉落、地面物品与过期、拾取/丢弃、46 格背包、14 装备槽、装备加成、药水/卷轴/丢金币、`UserItem[]` 存档；仓库见「玩家仓库与邮件」 |
| Buff | ✅ | 带到期时间的属性修饰层、`AddBuff`/`RemoveBuff` 广播、魔法盾按伤害扣时、到期重算属性并补发血量 |
| NPC 对话与商店 | ✅ | `Envir/NPCs` 脚本解析（`#INSERT`/`#INCLUDE`/条件/动作/按钮）、NPC 刷出与 `ObjectNPC`、`NewMapInfo` 下发、对话跳转、买/卖/修（Crystal 定价公式与价格倍率）；NPC 营业时段可见性未做 |
| 任务 | ✅ | `Envir/Quests` 脚本解析（13 个段）、接取前置（等级/职业/前置任务链/上限）、击杀·收集·标记目标、交付领奖（金币/经验/元宝/固定/任选）、限时超时、放弃、NPC `CHECKQUEST` 与 `[Quests]` 关联、`characters.quests` 存档往返；任务物品栏与每日重置未做（见 `docs/world/quests.md`） |
| 组队 | ✅ | 邀请/接受/拒绝/退队/踢人、队长顺延、组内 `!!` 聊天、16 格内按人数与等级分配经验、组队任务击杀共享、切图通知；PK 与地图准入规则未做 |
| 多地图与传送 | ✅ | 按地图文件惰性加载与缓存、`MovementInfo` 传送点（含 `need_hole`/`need_move`）、`S.MapChanged` 包序、NPC `MOVE`/`MAPMOVE`/`ENTERMAP`；客户端地形仍是启动时加载（见 `docs/client.md` §8） |
| 行会 | ✅ | 创建（NPC `[@CREATEGUILD]`）、邀请/加入、职位与权限位、升降职/踢人、公告、`!~` 行会聊天、行会经验与等级、行会仓库（112 格物品 + 资金）、行会 buff（购买/激活/到期/全员加成）、迁移 0005/0006 持久化；攻城与领地未做 |
| 玩家仓库与邮件 | ✅ | 80 格仓库（`[@STORAGE]` 页，`characters.storage` 持久化）、邮件发送（附件/邮票/邮费/限速）、阅读/锁定/领取/删除、迁移 0007、离线按名投递与 30 天过期 |
| 玩家交易 | ✅ | 面朝格发起/回应、10 格交易栏、金币、双方确认后互换、放物品即清双方确认、取消/走跑转身切图下线死亡中止、背包满时取消改寄邮件（`docs/world/trade.md`） |
| 寄售 / 拍卖市场 | 🟡 | NPC `[@CONSIGN]`/`[@MARKET]` 上架、搜索/翻页/买断/出价/提前成交/取回、5% 佣金与手续费、7 天过期与启动清理、迁移 0009 持久化；核心闭环可用，持久化与原子性已修复并有 PG 实测；到期自动成交未做（`docs/world/market.md`） |
| 商城（GameShop）与元宝 | 🟡 | 市场面板商城页签、`C.GameshopBuy` 真实扣元宝/金币、职业等级与库存校验、背包优先/放不下转商城邮件、库存计数迁移 0011 单事务持久化；独立商城面板与 `S.GameShopInfo`/`S.GameShopStock` 下发未做（`docs/world/gameshop.md`） |
| 攻城与领地 | ⏳ | `ConquestInfo` 数据已可读，城门/城墙/旗子与领地租借待做 |
| Rust 客户端（渲染与 UI） | 🟡 | 等距地图 + 联机 + 实体精灵 + 装备分层外观 + 逐帧动画（站立/走/跑/攻击/受击/死亡）+ 中文 UI（状态栏/聊天/背包/纸娃娃/tooltip/商店/对话/技能/buff/小地图/大地图/任务/邮件/仓库/行会/组队/交易/市场/商城页签/设置面板/服务器与角色选择）+ 技能表现（火墙/雷电/瞬息移动/隐身/召唤）+ 伤害数字 + 头顶血条 + 目标锁定 + 拖拽穿脱 + 点地走路 + 音效与背景音乐（rodio，可无声降级）；坐骑/翅膀精确素材、移动插值、寻路与天气待做（见 [`docs/client.md`](docs/client.md) §8） |
| 工具链（`mir-tools`） | 🟡 | 地图检查、协议统计、MirDB 报告、图库查看/导出；素材库编辑器未做 |
| 素材索引（`mir2_dev`） | ✅ | `tools/asset-index/` 把 `resource/` 的 233 万文件登记进 PostgreSQL，按名字/扩展名/仓库查询 |

### 协议自动生成

`tools/protocol-gen/` 会解析 Crystal 的 `Shared/Enums.cs`、`Shared/ClientPackets.cs`、
`Shared/ServerPackets.cs` 与 `Shared/Data/*.cs`，生成：

- `crates/mir-core/src/enums/generated.rs`：全部枚举（与 C# 一一对应，含底层类型）；
- `crates/mir-protocol/src/packets/{client,server}.rs`：每个包的 `decode` / `encode`。

生成器**同时翻译读侧与写侧**，只有当两边推导出的线格式完全一致时才产出代码；不一致的包会被列进
`tools/protocol-gen/report.md` 并改为手写（例如 `MapInformation` 的位标记、
`UserInformation` 的三组可选背包）。CI 会校验「生成结果与仓库中提交的一致」。

## 仓库结构

```text
mir-next/
├─ crates/
│  ├─ mir-core/       基础类型：坐标、方向、枚举（生成）、时间、随机数、多语言
│  ├─ mir-protocol/   包定义（生成）与编解码、帧格式、gzip、线格式数据类
│  ├─ mir-world/      .map 读取与可走性（9 种版本）
│  ├─ mir-db/         Crystal `Server.MirDB` 读取器（怪物/物品/NPC/刷怪点/技能…）
│  ├─ mir-store/      PostgreSQL 持久化：账号与角色（Crystal 口令算法）
│  ├─ mir-server/     网关、会话状态机、世界、战斗/技能/怪物 AI、商城与市场
│  ├─ mir-client/     客户端：等距渲染、UI 面板、登录/选角、设置面板、技能表现
│  └─ mir-tools/      工具链入口（地图检查、协议统计、MirDB 报告）
├─ tools/protocol-gen/ 从 Crystal C# 源码生成 Rust 协议代码
├─ tools/asset-index/  把 resource/ 登记进 mir2_dev 索引库，并按需查询
├─ tools/i18n/         硬编码中文扫描（i18n 收口清单的复跑工具）
├─ config/lang/       语言包（默认 zh-cn，另有 en；运行时加载，覆盖内置文本）
├─ docs/              协议、存档、地图与世界、战斗、素材索引的实现契约
├─ resource/          素材目录（不提交到版本库）
├─ github/            从 GitHub clone 的参考项目（如 github/Crystal；不提交到版本库）
├─ Cargo.toml         workspace 清单
└─ LICENSE            GPL-3.0-or-later
```

### 游戏数据与素材

怪物、物品、NPC、刷怪点都来自 Crystal 自己的数据库文件 `Server.MirDB`（二进制格式，
读取实现见 `crates/mir-db`，格式文档见 [`docs/store/mirdb-format.md`](docs/store/mirdb-format.md)）。

服务端启动时会按顺序找：

1. `MIR2_DB_PATH` 指定的文件；
2. `crates/mir-server/src/gamedata.rs` 里的候选路径（索引里挑的 3 个真实样本，含 v105/v88/v83）。

找到就用真实刷怪点铺满世界（比奇省 358 个刷怪点 / 2252 只怪），找不到就只刷演示怪，
服务器照常可跑。`MIR2_LOAD_GAME_DATA=0` 可以强制关掉数据库刷怪（测试用）。

素材（地图、客户端资源包、音效、社区仓库归档）统一放在 `resource/`，并由 PostgreSQL
数据库 `mir2_dev` 建立索引：

```bash
# 建立/更新索引（幂等；230 万文件几分钟跑完）
python tools/asset-index/index_resources.py

# 查素材：先查索引，再按路径去 resource/ 取
python tools/asset-index/find_asset.py --name Server.MirDB
python tools/asset-index/find_asset.py --ext .wzl --top data --limit 20
```

细节见 [`docs/assets/index.md`](docs/assets/index.md)。

## 多语言

- **默认全中文**：内置文本与 `config/lang/zh-cn.json` 都是简体中文，缺语言包时也回落中文。
- **游戏内切换**：聊天框输入 `/language en` 切换英文、`/language zh-cn` 切回中文，
  服务端回一条该语言的系统提示，并把选择写回 `accounts.language`（迁移 `0002`）。
- **服务端玩家可见文本全部走 `TextKey`**：聊天/系统提示/邮件/市场回执/攻城公告等
  200 个键都有中英模板；按**收件人自己的语言**渲染（全服公告逐收件人编码），
  `/language en` 后系统提示立刻跟随。
- **内容也跟随语言**：地图标题（`MapInformation.Title`）等文本从语言包的
  `maps` 段取，未收录时用文件名兜底。
- **客户端本地语言**：`cargo run -p mir-client -- --lang en`（设置存本地，断网也能切）。
  客户端固定文案在 `crates/mir-client/src/ui/labels.rs` 的 `Label` 枚举里编译期内置，
  **不进** `config/lang/`（后者只镜像服务端 `TextKey`）。

细节、新增文本流程与覆盖审计见 [`docs/i18n.md`](docs/i18n.md)；硬编码中文可用
`py tools/i18n/scan_hardcoded.py --player-visible --no-test --strict` 复跑（CI 以此作棘轮）。

## 技术选型

| 领域 | 选型 | 说明 |
| --- | --- | --- |
| 语言 | Rust（stable，edition 2024） | 工作区统一 lint（clippy all + pedantic） |
| 异步运行时 | tokio | 每连接一个任务，写队列走 `mpsc` |
| 编解码 | 手写 `PacketReader` / `PacketWriter` | 严格对齐 Crystal 的读写顺序 |
| 数据库 | PostgreSQL + `sqlx` | 迁移内嵌，集成测试需要显式提供测试库 URL |
| 配置 | 环境变量（`MIR2_*`） | `.env` 仅供本地开发 |
| 可观测性 | `tracing` | 按模块划分 target |
| 客户端渲染 | 未定（winit + wgpu 为候选） | 里程碑 M6 |

## 环境与构建

### 前置条件

- Rust 工具链（stable）
- Python 3（仅在重新生成协议代码时需要）
- PostgreSQL 16+（可选；不配置时服务端使用内存存储）
- Crystal 源码检出（仅在重新生成协议代码时需要）：放在仓库根的 `github/Crystal`

### 构建与运行

```bash
# 构建全部 crate
cargo build --release

# 跑测试（不依赖数据库）
cargo test --workspace

# 启动服务端（不配置数据库时用内存存储）
cargo run -p mir-server

# 用 PostgreSQL 存储（会先跑迁移）
DATABASE_URL=postgres://user:pass@host:5432/mir2 cargo run -p mir-server

# 检查一张地图
cargo run -p mir-tools -- map-info resource/map/0.map

# 看一眼游戏数据库里都有什么（怪物/物品/NPC/刷怪点数量 + 抽样记录）
cargo run -p mir-tools -- mirdb-report --path resource/community/suprcode-crystal-database/cjlaaa/Server.MirDB

# 从 Crystal 源码重新生成协议代码（默认读 github/Crystal，可用 --crystal 覆盖）
python tools/protocol-gen/generate.py
```

服务端默认监听 `0.0.0.0:7000`（`MIR2_LISTEN` 可改），会尝试加载
`resource/map/0.map` 作为起始地图；加载失败则退化成只校验边界与占格的兜底世界。

### 本地试玩链路

1. `cargo run -p mir-server`
2. 用 Crystal 客户端（或后续的 `mir-client`）连上来：版本校验通过后会收到 `ClientVersion{Result=1}`；
3. 注册账号（成功码 `8`）→ 登录（收到 `LoginSuccess`）→ 建角色（等级 0，首次进世界补到 1 级）
   → 进世界（`StartGame{Result=4}` + `MapInformation` + `UserInformation`）；
4. 在聊天框输入 `/language en` 可以在游戏内切换语言。

## 文档

| 文档 | 内容 |
| --- | --- |
| [`docs/protocol/framing.md`](docs/protocol/framing.md) | 帧格式、类型映射、字符串编码、gzip、上限，逐字节契约 |
| [`docs/protocol/packet-ids.md`](docs/protocol/packet-ids.md) | 两张完整包编号表（279 + 153） |
| [`docs/protocol/packets-complex.md`](docs/protocol/packets-complex.md) | 需要手写/特殊处理的包逐条说明 |
| [`docs/protocol/login-flow.md`](docs/protocol/login-flow.md) | 连接生命周期、账号与角色流程、状态机、结果码 |
| [`docs/store/schema.md`](docs/store/schema.md) | 账号/角色的持久化字段与存档触发点 |
| [`docs/store/mirdb-format.md`](docs/store/mirdb-format.md) | Crystal 二进制世界库与账号库的容器格式 |
| [`docs/store/drops.md`](docs/store/drops.md) | 掉落表（`Envir/Drops/*.txt`）的格式与掷骰语义 |
| [`docs/world/map-format.md`](docs/world/map-format.md) | `.map` 各版本格式与真实文件实测 |
| [`docs/world/movement.md`](docs/world/movement.md) | 走/跑/转身的判定与计时 |
| [`docs/world/tick-loop.md`](docs/world/tick-loop.md) | 世界循环与计时模型 |
| [`docs/world/combat.md`](docs/world/combat.md) | 战斗公式、RNG 顺序、怪物 AI 与相关数据包 |
| [`docs/world/items.md`](docs/world/items.md) | 物品系统：掉落、地面物品、背包/装备槽、持久化与协议包 |
| [`docs/world/magic.md`](docs/world/magic.md) | 技能系统：数据与存档、用书学习、施法流程、伤害/治疗公式、协议包与支持矩阵 |
| [`docs/world/npc.md`](docs/world/npc.md) | NPC 脚本格式、条件/动作支持矩阵、商店与修理公式 |
| [`docs/world/quests.md`](docs/world/quests.md) | 任务系统：脚本段、接取/交付流程、奖励与存档 |
| [`docs/world/buffs.md`](docs/world/buffs.md) | Buff 容器：到期、属性叠加、药水与增益技能 |
| [`docs/world/maps.md`](docs/world/maps.md) | 多地图与传送：加载缓存、传送点、切图包序 |
| [`docs/world/aoi.md`](docs/world/aoi.md) | AOI 网格、视野半径 16、进出视野差分与门状态机 |
| [`docs/world/group.md`](docs/world/group.md) | 组队：状态模型、包序、经验共享公式 |
| [`docs/world/guild.md`](docs/world/guild.md) | 行会：创建/职位/公告/聊天/经验、仓库与 buff、持久化 |
| [`docs/world/storage.md`](docs/world/storage.md) | 玩家仓库：80 格、存取校验与存档 |
| [`docs/world/mail.md`](docs/world/mail.md) | 邮件：发送/领取/锁定/删除、附件与邮费、过期 |
| [`docs/world/trade.md`](docs/world/trade.md) | 玩家交易：发起/接受/交易格/金币/确认/取消、距离与朝向、包序 |
| [`docs/world/market.md`](docs/world/market.md) | NPC 寄售/拍卖：上架/搜索/翻页/买断/出价/取回/过期、佣金与持久化 |
| [`docs/world/gameshop.md`](docs/world/gameshop.md) | 商城（GameShop）与元宝：商品数据、购买校验、发货与邮件兜底、库存计数与页签包序 |
| [`docs/client.md`](docs/client.md) | 客户端：协议流程、按键/鼠标、面板与字体 |
| [`docs/assets/index.md`](docs/assets/index.md) | 素材索引：目录约定、表结构、查询方式 |
| [`docs/i18n.md`](docs/i18n.md) | 多语言：默认中文、游戏内切换与 i18n 收口清单（逐条 TextKey 表） |
| [`docs/architecture.md`](docs/architecture.md) | 架构总览：crate 分层、数据流、时间与随机、测试策略 |

## 开发约定

- 提交前至少通过 `cargo fmt --all --check`、`cargo clippy --workspace --all-targets -- -D warnings` 与 `cargo test --workspace`。
- 协议代码**不要手改**：改 `tools/protocol-gen/` 后重新生成；手写的包放在 `crates/mir-protocol/src/manual/`。
- 每个 crate 只承担单一职责；规则逻辑与网络、存储解耦，保证可以脱离网络单独测试。
- 库代码中对可恢复错误使用 `Result` 传播，不使用 `unwrap` / `expect`。
- 新增玩法时，先在 `docs/` 写明与 Crystal 的对照行为（带源码行号），再实现与补充测试。
- 玩家可见文本一律走 `mir-core::i18n`，默认简体中文，不在代码里写死文案。

## 许可证

Copyright (C) 2026 diana and contributors.

本项目采用 [GNU General Public License v3.0 or later](LICENSE)，SPDX 标识为 `GPL-3.0-or-later`。

`LICENSE` 中存放的是 GPL-3.0 的完整官方文本；`or later` 的授权范围以本节声明为准。

## 版权与免责声明

《热血传奇 2》/ Legend of Mir 2 的商标、美术、音频与地图数据归 Wemade Entertainment、ActozSoft 等权利方所有。本仓库只包含自行编写的代码与文档，**不附带任何官方游戏素材**。自行准备素材并运行服务器时，请确认你拥有相应授权。

本项目仅供学习、研究与自建私服的技术验证使用，请勿用于商业运营；因使用本项目产生的任何法律与运营风险由使用者自行承担。

## 致谢

- [Crystal](https://github.com/Suprcode/Crystal) 及 LOMCN 社区：提供了完整可参照的传奇 2 服务端与客户端实现。
- ActozSoft 与 Wemade Entertainment：创造了《热血传奇 2》原作。
- Rust 生态中的 tokio、sqlx、tracing、flate2 等项目。
