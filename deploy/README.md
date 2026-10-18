# 部署（Docker）

《传奇-Next》的服务端是**单个可执行文件**：`cargo build --release -p mir-server` 产出
`target/release/mir-server`，它只依赖 `resource/`（地图与素材）和 `config/`（语言包），
存储走 PostgreSQL（不配 `DATABASE_URL` 时用内存存储，重启丢档，仅供试玩）。

这个目录给的是两种跑法：**本机单进程**和 **compose 起服务端 + PostgreSQL**。

## 一、compose（推荐）

```bash
cd deploy
cp mir2.env.example mir2.env        # 改口令/端口
docker compose --env-file mir2.env up -d --build
docker compose logs -f server
```

- 服务端监听 `7000`（`MIR2_PORT` 可改），客户端连 `127.0.0.1:7000`。
- `postgres` 首次启动建库 `mir2`；服务端启动时 `sqlx` 自动跑内嵌迁移（`0001` 起）。
- `resource/` 与 `config/` 从仓库根目录**只读挂载**到 `/app/resource`、`/app/config`，
  与 `ServerConfig::default()` 的相对路径一致（`resource/map`、`config/lang`），所以不需要
  额外环境变量。
- 没放 `Server.MirDB` 时服务端会退化成「只刷演示怪」的兜底世界，照样能登录走路打怪。

数据卷：`pgdata`（删掉 `docker compose down -v` 会连库一起删）。

## 二、本机单进程

```bash
cargo build --release -p mir-server
DATABASE_URL=postgres://user:pass@host:5432/mir2 ./target/release/mir-server
```

启动日志里出现「传奇-Next 网关已启动」即就绪。

## 三、环境变量

| 变量 | 作用 | 默认 |
| --- | --- | --- |
| `MIR2_LISTEN` | 监听地址 | `0.0.0.0:7000` |
| `DATABASE_URL` | PostgreSQL 连接串；空则用内存存储 | 空 |
| `MIR2_DB_PATH` | Crystal `Server.MirDB` 路径 | 探测候选路径，找不到降级 |
| `MIR2_LOAD_GAME_DATA` | `0` 强制不加载 MirDB（只刷演示怪） | 加载 |
| `MIR2_CHECK_VERSION` | 是否校验客户端版本哈希 | 见 `config.rs` |
| `MIR2_ALLOW_START_GAME` | 是否允许进世界 | 见 `config.rs` |
| `MIR2_START_MAP` | 新角色起始地图文件名 | `0` |
| `MIR2_DEMO_MONSTERS` | 出生点附近演示怪数量 | 见 `config.rs` |
| `RUST_LOG` | 日志级别（`tracing`） | `info` |

`map_root`、`language_dir` 目前是编译期默认值（`resource/map`、`config/lang`），
**相对进程工作目录**解析；容器里工作目录是 `/app`，所以挂载点必须是 `/app/resource` 与
`/app/config`。

## 四、常见问题

- **端口占用**：改 `MIR2_PORT`（compose）或 `MIR2_LISTEN`（直接跑）。
- **地图加载失败**：日志会打「地图加载失败，退化成兜底世界」；确认 `resource/map/0.map`
  存在。素材不进仓库，需自行准备（见仓库根 `docs/assets/index.md`）。
- **数据库连不上**：先用 `docker compose ps` 看 `postgres` 是否 healthy；服务端启动时会
  跑迁移，权限不足会在日志里明确报错。
- **想清档重来**：`docker compose down -v && docker compose up -d`。
