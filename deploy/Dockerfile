# 传奇-Next 服务端镜像（多阶段构建）
#
# 只构建 `mir-server` 这一个二进制：客户端（winit/softbuffer 需要 X11/Wayland 头文件）
# 不在容器里跑，也就不装那堆系统依赖。
#
# 用法见同目录 README.md；推荐的跑法是 `docker compose up -d`（会一起起 PostgreSQL）。

FROM rust:1-bookworm AS build
WORKDIR /src

# 先只拷清单跑一次依赖解析，源码改动时不必重下依赖（本地缓存层）。
COPY Cargo.toml Cargo.lock rust-toolchain.toml ./
COPY crates/mir-core/Cargo.toml crates/mir-core/Cargo.toml
COPY crates/mir-db/Cargo.toml crates/mir-db/Cargo.toml
COPY crates/mir-protocol/Cargo.toml crates/mir-protocol/Cargo.toml
COPY crates/mir-store/Cargo.toml crates/mir-store/Cargo.toml
COPY crates/mir-world/Cargo.toml crates/mir-world/Cargo.toml
COPY crates/mir-server/Cargo.toml crates/mir-server/Cargo.toml
COPY crates/mir-client/Cargo.toml crates/mir-client/Cargo.toml
COPY crates/mir-render/Cargo.toml crates/mir-render/Cargo.toml
COPY crates/mir-asset/Cargo.toml crates/mir-asset/Cargo.toml
COPY crates/mir-tools/Cargo.toml crates/mir-tools/Cargo.toml

# 源码。
COPY crates ./crates

# `sqlx` 的迁移是编译期内嵌的（`crates/mir-store/migrations/`），构建期不需要数据库。
RUN cargo build --release -p mir-server \
    && strip target/release/mir-server

FROM debian:bookworm-slim AS runtime
# `sqlx` 走 rustls，不需要 OpenSSL；证书只留给以后对外抓取用。
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app
COPY --from=build /src/target/release/mir-server /app/mir-server

# 素材与语言包一律挂载进来（`resource/` 有两百多万个文件，不进镜像）：
#   /app/resource   地图 + 客户端素材 + MirDB 样本
#   /app/config     语言包（config/lang/*.json）
# 路径与 `ServerConfig::default()` 的相对默认值一致（`resource/map`、`config/lang`，
# 相对进程工作目录 `/app`），所以不需要额外环境变量。
ENV MIR2_LISTEN=0.0.0.0:7000 \
    RUST_LOG=info

EXPOSE 7000
ENTRYPOINT ["/app/mir-server"]
