# 本地全量门禁（替代 GitHub Actions）

仓库现在托管在**私有**仓库 `PullAndRun/mir-next`。GitHub Actions 在私有库上需要付费额度，
当前账号的 spend limit / 账单未就绪，**Action 会在分配 runner 前就失败**（报错：
`The job was not started because recent account payments have failed or your spending limit needs to be increased`）。

所以门禁不能依赖远端 CI，改为**本地一条命令**：

```powershell
pwsh -NoProfile -File tools/dev/ci.ps1                # 全跑
pwsh -NoProfile -File tools/dev/ci.ps1 -SkipTests     # 只跑静态检查（快）
pwsh -NoProfile -File tools/dev/ci.ps1 -SkipPostgres  # 没有本地 PG 时
pwsh -NoProfile -File tools/dev/ci.ps1 -NoAssets      # 额外跑「无 resource/」沙箱（慢）
```

## 覆盖的检查（与 `.github/workflows/ci.yml` 的 5 个 job 一一对应）

| # | 检查 | 对应 CI job |
|---|---|---|
| 1 | `cargo fmt --all --check` | Rust |
| 2 | `cargo clippy --workspace --all-targets -- -D warnings` | Rust |
| 3 | `cargo test -p mir-server --test duo_smoke -- --test-threads=1`（串行） | Rust |
| 4 | `cargo test --workspace` | Rust |
| 5 | `py tools/protocol-gen/generate.py --crystal github/Crystal --check` | 协议生成器 |
| 6 | `py tools/docs/check_links.py` | 文档 |
| 7 | `py tools/i18n/verify_text_keys.py` + 两个硬编码棘轮（阈值 70 / 15） | i18n |
| 8 | `cargo test -p mir-store -- --test-threads=1`（PG） | PostgreSQL |
| 9 | `-NoAssets` 时：`tools/dev/test-no-assets.ps1` | （本地补充，模拟没有素材的 CI 环境） |

## 注意

- **`python` vs `py`**：Windows 上 `python` 常是 Microsoft Store 的占位 stub，脚本优先用 `py`。
- **PowerShell 7**：统一用 `pwsh`（PS 7）。脚本仍存为 UTF-8 with BOM，兼容还在用 5.1 的人。
- **远端 CI 恢复条件**：账号恢复 billing / 提高 spend limit 后，push 会自动重跑
  `.github/workflows/ci.yml`；两者检查项一致，不需要改代码。
- **私有库的可见性**：`gh api /repos/PullAndRun/mir-next` 显示所有者 `admin=true`，
  可自行改可见性、设置、删除。
