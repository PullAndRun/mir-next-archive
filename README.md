# 传奇-Next

[![License: GPL-3.0-or-later](https://img.shields.io/badge/license-GPL--3.0--or--later-blue.svg)](LICENSE)
![Rust](https://img.shields.io/badge/rust-stable-orange)
![Status](https://img.shields.io/badge/status-initializing-lightgrey)

用 Rust 重写的《热血传奇 2》（Legend of Mir 2）服务端、客户端与配套工具链。

## 项目简介

传奇-Next（仓库名 `mir-next`）是《热血传奇 2》的开源重制项目。项目以 LOMCN 社区维护的开源实现 [Crystal](https://github.com/Suprcode/Crystal)（C#）作为玩法与协议的参照系，用 Rust 从零实现服务端、客户端与工具链，目标是在保留原作玩法体验的前提下，获得更好的性能、可维护性与跨平台部署能力。

> **当前阶段：初始化。** 仓库目前只包含工程文档与许可证，尚无任何源码。下文中的架构、技术选型与命令描述的都是目标状态，会随实现推进同步修正。

## 项目目标

- **玩法对齐**：地图与行走、战斗与技能、装备与强化、NPC 商店与修理、掉落与经济、任务、组队、行会、攻城、PK 与红名，均以 Crystal 的行为作为基准逐项对齐。
- **协议对齐**：数据包编号、字段顺序与序列化规则尽量与 Crystal 保持一致，使现有 Crystal 客户端可以直接连接本项目服务端。
- **数据对齐**：能够读取 Crystal 生态的地图文件、素材库与数据库结构，降低素材与存档的迁移成本。
- **工程化**：单一可执行文件部署、配置驱动、结构化日志与指标、覆盖核心规则的自动化测试。

## 当前状态

| 模块 | 状态 |
| --- | --- |
| 仓库初始化、许可证、README | ✅ 完成 |
| Cargo workspace 脚手架与 CI | ⏳ 规划中 |
| 协议定义、登录网关与会话 | ⏳ 规划中 |
| 世界环境、地图与视野广播 | ⏳ 规划中 |
| 战斗、怪物 AI、技能与状态效果 | ⏳ 规划中 |
| 角色存档、物品、商店与掉落 | ⏳ 规划中 |
| 行会、任务、组队与攻城 | ⏳ 规划中 |
| Rust 客户端（渲染与 UI） | ⏳ 规划中 |
| 工具链（素材库编辑器、地图查看器、补丁器） | ⏳ 规划中 |

## 与 Crystal 的对应关系

| Crystal 模块 | 职责 | 传奇-Next 对应 | 说明 |
| --- | --- | --- | --- |
| `Shared` | 数据包、枚举、公共结构 | `mir-protocol` | 包编号与字段顺序对齐 Crystal，便于复用客户端 |
| `Server.MirEnvir` | 世界环境、地图、刷怪与全局状态 | `mir-world` | 按地图分区组织，避免单一「大世界」锁 |
| `Server.MirObjects` | 玩家、怪物、NPC、行会与技能实现 | `mir-world` | 对象系统按行为拆分，便于单独测试 |
| `Server.MirDatabase` | 存档模型与数据库访问 | `mir-store` | PostgreSQL，结构对照 Crystal 的表设计 |
| `Server.MirNetwork` | 网关、连接与封包分发 | `mir-server` | tokio 异步，网络层与规则层解耦 |
| `Client` | 渲染、界面与输入 | `mir-client` | winit + wgpu 重写 |
| `LibraryEditor` / `LibraryViewer` | 素材库编辑与预览 | `mir-tools` | 合并为一套命令行/图形工具 |
| `AutoPatcher` | 客户端补丁分发 | `mir-tools` | 规划中 |

本项目以 Crystal 的行为为参照，代码为独立实现，不直接复制其 C# 源码。

## 仓库结构（规划）

```text
mir-next/
├─ crates/
│  ├─ mir-core/       基础类型：坐标、方向、时间、随机数、枚举
│  ├─ mir-protocol/   数据包定义、编解码与版本握手
│  ├─ mir-world/      地图、视野、对象、战斗、AI、技能与规则
│  ├─ mir-store/      存档模型、数据库迁移与仓储
│  ├─ mir-server/     网关与游戏服务端可执行程序
│  ├─ mir-client/     客户端可执行程序（渲染、UI、输入）
│  └─ mir-tools/      素材库、地图与补丁工具
├─ config/            服务端配置模板
├─ docs/              架构说明与协议文档
├─ resource/          素材目录（不提交到版本库）
├─ Cargo.toml         workspace 清单
└─ LICENSE            GPL-3.0-or-later
```

目录名会按实际实现调整，以 `Cargo.toml` 的 workspace 成员为准。

## 技术选型（计划采用）

| 领域 | 选型 | 说明 |
| --- | --- | --- |
| 语言 | Rust（stable） | edition 2024，MSRV 随实现确定 |
| 异步运行时 | tokio | 网络 I/O、定时器与后台任务 |
| 编解码 | `bytes` + 手写 packet trait | 严格对齐 Crystal 的读写顺序 |
| 数据库 | PostgreSQL + `sqlx` | 账号、角色、行会、日志与后台数据 |
| 缓存（可选） | Redis | 会话与跨服状态共享 |
| 配置 | `serde` + TOML | 服务端配置分层覆盖 |
| 可观测性 | `tracing` | 结构化日志，按模块划分 target |
| 客户端渲染 | `winit` + `wgpu` | 解码素材库后交由 GPU 渲染 |

## 环境与构建

### 前置条件

- Rust 工具链（通过 [rustup](https://rustup.rs/) 安装 stable 版本）
- PostgreSQL 16 及以上（服务端存档）
- Redis（可选，仅在启用会话共享时需要）
- Windows 与 Linux 为主要目标平台；macOS 尚未验证

### 构建与运行

脚手架落地后，工作区将提供以下命令：

```bash
# 构建全部 crate
cargo build --release

# 运行服务端（后续会拆分为网关与游戏服）
cargo run -p mir-server

# 运行全部测试
cargo test --workspace
```

> 当前仓库还没有 `Cargo.toml`，以上命令暂不可用。

## 素材与本地资源

游戏素材（地图、素材库、音频等）不随仓库分发。`resource/` 目录已在 [`.gitignore`](.gitignore) 中忽略，其中的内容按来源分类存放（例如 `community`、`data`、`ld`、`map`、`wav`），由使用者自行准备。

数据库连接串、下载器令牌等本地开发配置统一放在根目录的 `.env` 文件，该文件同样不会进入版本库。克隆后请自行创建，不要提交任何凭据。

## 版权与免责声明

《热血传奇 2》/ Legend of Mir 2 的商标、美术、音频与地图数据归 Wemade Entertainment、ActozSoft 等权利方所有。本仓库只包含自行编写的代码与文档，**不附带任何官方游戏素材**。自行准备素材并运行服务器时，请确认你拥有相应授权。

本项目仅供学习、研究与自建私服的技术验证使用，请勿用于商业运营；因使用本项目产生的任何法律与运营风险由使用者自行承担。

## 路线图

| 里程碑 | 内容 | 状态 |
| --- | --- | --- |
| M0 | 仓库初始化、许可证与文档 | ✅ |
| M1 | Cargo workspace、协议定义、登录网关，客户端可登录并选角 | ⏳ |
| M2 | 地图读取、视野广播、行走与传送 | ⏳ |
| M3 | 战斗、怪物 AI、技能与状态效果 | ⏳ |
| M4 | 物品、背包装备、NPC 商店、修理与掉落 | ⏳ |
| M5 | 行会、任务、组队、攻城与 PK 规则 | ⏳ |
| M6 | Rust 客户端：素材库解码、地图渲染与界面 | ⏳ |
| M7 | 工具链（素材库编辑器、地图查看器、补丁器）与发布 | ⏳ |

## 开发约定

- 提交前至少通过 `cargo fmt --all --check`、`cargo clippy --workspace --all-targets -- -D warnings` 与 `cargo test --workspace`。
- 每个 crate 只承担单一职责；规则逻辑与网络、存储解耦，保证可以脱离网络单独测试。
- 库代码中对可恢复错误使用 `Result` 传播，不使用 `unwrap` / `expect`。
- 新增玩法时，优先在 issue 或文档中写明与 Crystal 的对照行为，再实现与补充测试。
- 面向玩家的文案保持原作口径，中英文版本后续统一维护。

## 许可证

本项目采用 [GNU General Public License v3.0 or later](LICENSE)，SPDX 标识为 `GPL-3.0-or-later`。

## 致谢

- [Crystal](https://github.com/Suprcode/Crystal) 及 LOMCN 社区：提供了完整可参照的传奇 2 服务端与客户端实现。
- ActozSoft 与 Wemade Entertainment：创造了《热血传奇 2》原作。
- Rust 生态中的 tokio、sqlx、tracing、wgpu 等项目。
