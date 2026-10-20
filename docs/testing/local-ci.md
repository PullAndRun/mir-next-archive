# 本地开发、测试与门禁（**完全没有远端 CI**）

本项目**不使用任何远端 CI**：所有编译、测试、压测、验证都在本地跑。
仓库是私有的 `PullAndRun/mir-next`，`.github/workflows` 已删除，push 不会触发任何自动化。

## 唯一入口

```powershell
pwsh -NoProfile -File tools/dev/ci.ps1                # 全量门禁（推荐每轮收口前跑）
pwsh -NoProfile -File tools/dev/ci.ps1 -SkipTests     # 只跑静态检查（~10 秒，改代码时常跑）
pwsh -NoProfile -File tools/dev/ci.ps1 -SkipPostgres  # 没有本地 PG 时
pwsh -NoProfile -File tools/dev/ci.ps1 -NoAssets      # 额外跑「无 resource/」沙箱（慢）
```

退出码 `0` = 全绿。

## 覆盖的检查（共 10 项）

| # | 检查 | 说明 |
|---|---|---|
| 1 | `cargo fmt --all --check` | 格式 |
| 2 | `cargo clippy --workspace --all-targets -- -D warnings` | 静态检查（0 warning） |
| 3 | `cargo test -p mir-server --test duo_smoke -- --test-threads=1` | **串行**跑双人联机冒烟（两个客户端抢同一战斗槽，并行会 flake） |
| 4 | `cargo test --workspace` | 全量测试 |
| 5 | `py tools/protocol-gen/generate.py --crystal github/Crystal --check` | 协议生成结果与仓库一致（`github/Crystal` 不存在时跳过并提示） |
| 6 | `py tools/docs/check_links.py` | 文档链接与包编号 |
| 7 | `py tools/i18n/verify_text_keys.py` | TextKey 中英完整 |
| 8 | 硬编码棘轮两条 | `--player-visible --fail-above 70`、`--no-test --fail-above 15` |
| 9 | `cargo test -p mir-store -- --test-threads=1` | PostgreSQL 集成测试（需要本地 PG） |

`-NoAssets` 额外调用 `tools/dev/test-no-assets.ps1`：把测试二进制放进**没有 `resource/`** 的沙箱执行，
确保「缺素材 → 显式 skip，而不是红」，见 [`ci-assets.md`](ci-assets.md)。

## 巡检例程（发版前 `-Full`，日常 `-Quick`）

`ci.ps1` 是**每次收口都要跑**的快门禁；另有三条慢检查不适合塞进门禁，单独做成一条命令：

```powershell
pwsh -NoProfile -File tools/dev/check-routine.ps1 -Full      # 发版前 / 冻结前：迁移演练 + 无素材沙箱 + 关键 PG 用例
pwsh -NoProfile -File tools/dev/check-routine.ps1 -Quick     # 日常：跳过迁移演练（summary 标 SKIPPED）
pwsh -NoProfile -File tools/dev/check-routine.ps1 -SelfTest  # 自证会红：注入真实失败 + 随后 -Quick 恢复
```

- 证据落在 `tools/dev/evidence/check-routine/<时间戳>/`（`summary.md` + 逐步 `.log`，已 gitignore）；
- 退出码：`0` 全过、`1` 有步骤失败、`2` 参数/环境错误（缺 `MIR2_DRILL_DATABASE_URL` 等）；
- **不并入** `ci.ps1`：无素材沙箱要另建 target 跑全套二进制（分钟级），迁移演练要真 PG。
- 细节见 [`check-routine.md`](check-routine.md)。

## 约定

- **PowerShell 7**：一律 `pwsh -NoProfile -File ...`。脚本仍存 UTF-8 with BOM（兼容 PS 5.1 的环境）。
- **Python**：脚本优先用 `py`（Windows 上 `python` 常是 Microsoft Store 的占位 stub）。
- **不依赖远端**：不要以「推上去看 CI」作为验收依据；**交付前自己跑过并贴输出**。
- **机器空闲才压测**：`tools/stress/stress.py` 的 p99 对机器负载极敏感（负载 60% 时能放大 3 倍），
  压测前先确认没有 `cargo`/`rustc`/`mir-server` 在跑。
