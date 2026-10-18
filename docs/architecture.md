# 架构总览

《传奇-Next》是一个 Rust workspace。按「数据 → 协议 → 世界 → 服务 → 表现」分层，
每一层都可以脱离上层单独测试。

```text
                 ┌──────────────────────────────┐
                 │  resource/（素材，不进 git）  │
                 │  map/*.map  data/*.wzl  ...   │
                 └───────────┬──────────────────┘
                             │ 由 tools/asset-index 建索引（PostgreSQL mir2_dev）
        ┌────────────────────┼─────────────────────┬──────────────────────┐
        ▼                    ▼                     ▼                      ▼
  mir-core            mir-protocol           mir-world              mir-asset
  基础类型/枚举        包编解码/数据结构       .map 地形               .Lib/.wzl 图库
  （由 C# 生成）      （由 C# 生成+手写）     （9 种版本）            （调色板/565/DXT1）
        ▲                    ▲                     ▲                      ▲
        └────────┬───────────┴─────────────────────┘                      │
                 │                                                        │
            mir-store（PostgreSQL：账号/角色/背包/装备/技能/金币）                     │
                 │                                                        │
              mir-db（Server.MirDB：怪物/物品/NPC/刷怪点/技能/掉落表/脚本）  │
                 │                                                        │
                 ▼                                                        │
            mir-server（网关、会话、世界、战斗、怪物 AI、技能、物品、NPC）           │
                 │                                                        │
       ┌─────────┴─────────┐                                             │
       ▼                   ▼                                             │
  mir-client（窗口外壳）  mir-render（纯 CPU 地图渲染）◄────────────────────┘
```

## 各 crate 的职责

| crate | 职责 | 是否依赖外部数据 |
| --- | --- | --- |
| `mir-core` | 坐标、方向、颜色、时间、随机数、多语言；`enums/` 由 `tools/protocol-gen` 从 Crystal 的 `Shared/Enums.cs` 生成（`[Flags]` 枚举生成为位掩码新类型）。 | 否 |
| `mir-protocol` | 帧格式、`PacketReader`/`PacketWriter`、432 个客户端/服务端包的 `decode`/`encode`、线格式数据类（`ItemInfo`/`UserItem`/`ClientMapInfo`…）。 | 否 |
| `mir-world` | 客户端 `.map` 解析（9 种版本）、可走性、门/光照等格数据。 | `resource/map` |
| `mir-asset` | `.Lib` / `.wzl+.wzx` 图库解码为 RGBA8；调色板、RGB565、RGB24、DXT1、gzip RGBA32。 | `resource/data` |
| `mir-db` | Crystal `Server.MirDB` 读取（版本 60..117）、掉落表（`Envir/Drops`）、NPC 脚本（`Envir/NPCs`）。 | `Server.MirDB` + `Envir/` |
| `mir-store` | PostgreSQL 持久化：账号（Crystal 口令算法）、角色、背包/装备/技能/仓库/邮件/行会/攻城状态/市场挂单。 | 需要库 |
| `mir-server` | 网关与会话状态机、世界 tick、怪物 AI 与战斗、技能（魔法）、物品与背包、NPC 与商店、经验/复活/回血。 | 全部依赖 |
| `mir-render` | 纯 CPU 等距渲染：地图格 → 图库 → 一帧 RGBA。 | `resource` |
| `mir-client` | winit 窗口 + softbuffer 呈现，键盘移动镜头。 | `resource` |
| `mir-tools` | CLI：`map-info`、`protocol-report`、`mirdb-report`、`image-info`、`image-dump`。 | 视子命令 |

## 数据流（一次攻击）

```text
客户端 C.Attack
   │
   ▼
session.rs ──► World::attack（动作校验、取目标）
   │                 │  立即广播 S.ObjectAttack
   │                 └─► pending_hits（300ms 后结算）
   ▼
World::tick ──► 结算伤害 ──► S.ObjectStruck / S.HealthChanged / S.DamageIndicator
   │
   └─► 目标死亡 ──► S.Death / S.ObjectDied ──► 掉落（DropTable）──► S.ObjectItem
                                            └─► 经验（ExpTable）──► S.WinExp / S.LevelChanged
```

## 时间与随机

- **时钟**：`World::now()` 是「服务端启动以来的毫秒数」，所有动作计时都是绝对时刻
  （`action_time`、`cell_time`、`attack_time`、`respawn.next_spawn_at`），避免累积误差。
  测试用 `World::advance_clock` 注入时间，不需要 `sleep`。
- **随机**：全服单个 `MirRng`（xorshift128+），抽取顺序与 Crystal 的 `Envir.Random` 一致；
  测试用 `World::seed_rng` 固定序列。这是「同一次操作在不同机器上结果一致」的前提。

## 生成代码的边界

只有两处是「从 Crystal 源码生成」的：`mir-core` 的枚举与 `mir-protocol` 的包。
生成器（`tools/protocol-gen/`）同时翻译 C# 的读侧与写侧，只有两侧推导出的线格式签名
完全一致才产出代码；不一致的包登记到 `tools/protocol-gen/report.md` 并改为手写实现
（`crates/mir-protocol/src/manual/`）。CI 会校验「生成结果与提交的一致」。

## 测试策略

| 层次 | 做法 |
| --- | --- |
| 协议 | 每个包的默认值 `encode_frame` → `decode` 往返；`[Flags]` 组合值的位保真 |
| 数据 | 用 `resource/` 里的真实 `Server.MirDB` / `.wzl` / `.map` 解析，断言 `leftover = 0` 或逐字节重建 |
| 世界 | 固定 seed + 注入时钟，断言坐标、计时、收到的包序列 |
| 服务 | 真实 TCP 客户端跑「注册 → 登录 → 建角 → 进世界 → 走/打/捡 → 下线」 |
| 渲染 | 离屏渲染一帧并回读像素，导出 PNG 供目视 |

> `resource/` 体积太大不进仓库：CI 上依赖真实素材的测试会自行打印「跳过」并返回成功，
> 本地开发时则完整执行。素材是否存在可以用 `python tools/asset-index/find_asset.py` 查。
