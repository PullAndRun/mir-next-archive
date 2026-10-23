# 第 45 轮最终门禁与 D 批次证据冻结

> 本文件记录第 45 轮「自机走/跑插值 + 相机跟随（含 X1-01 修复）」提交前在同一冻结候选上执行的
> 最终 12 项门禁、D 批次证据冻结与候选标识。所有条目均为实际执行结果，未执行项不写入。

## 1. 候选标识

- 父提交：`90b8188c7143bb4800d35abb65bd548a7f6d5f12`（Rust 基线）
- Crystal 参考：`0e315fe327192afe52c3d7357ddd1f5b7e26c5b8`
- 候选清单：`target/final-candidate-sha256.txt`（17 个文件；算法为 **git hash-object = SHA-1 blob**，不是 SHA-256）
- 源码 7 项（门禁前后必须一致）：
  - `crates/mir-client/Cargo.toml` — `09d99669ca0d4d4abf6846122c1bfbdc1278dda6`
  - `crates/mir-client/src/app.rs` — `e6edc7d94a0d0a826c6a82f07a245341f827bb5b`
  - `crates/mir-client/src/lib.rs` — `d9f5c630a1d2d75b10089663bca6dffdd5710164`
  - `crates/mir-client/src/net.rs` — `262a6e3e3aee3e3c66289597befe83c938a93991`
  - `crates/mir-client/tests/live_world.rs` — `b4bdf520e6f67ae5ad022cd2f695f6ffb127a060`
  - `crates/mir-client/src/camera.rs` — `c649138e258afa6287fc8a3dabbbed8a9d97c70c`
  - `crates/mir-render/src/entities.rs` — `19b243a05e1ca482c1e23018be1ddf517d644c08`

## 2. 最终 12 项门禁（`target/final-gate-20261024-002851.log`）

命令（仓库根）：

```powershell
pwsh -NoProfile -File tools/dev/ci.ps1 -KillStale
```

| # | 项 | 结果 | 耗时 |
|---|---|---|---|
| 1 | `cargo fmt --all --check` | OK | 2s |
| 2 | `cargo clippy --workspace --all-targets -- -D warnings` | OK | 4s |
| 3 | `cargo test -p mir-server --test duo_smoke -- --test-threads=1` | OK | 14s |
| 4 | `cargo test --workspace -- --skip duo_ --skip fault_pg` | OK | 117s |
| 5 | 协议生成器 `--check`（`tools/protocol-gen/generate.py --crystal github/Crystal --check`） | OK | 1s |
| 6 | 文档链接与包编号（`tools/docs/check_links.py`） | OK | 0s |
| 7 | TextKey 完整性（`tools/i18n/verify_text_keys.py`） | OK | 0s |
| 8 | 硬编码棘轮 player-visible（`scan_hardcoded.py --player-visible --strict --fail-above 70`） | OK | 0s |
| 9 | 硬编码棘轮 no-test（`--no-test --player-visible --strict --fail-above 15`） | OK | 0s |
| 10 | 运行时不引用老资源格式（`tools/dev/check_legacy_refs.py`） | OK | 0s |
| 11 | 物品图标完整性（`tools/assets/verify_item_icons.py`） | OK | 1s |
| 12 | PostgreSQL 集成测试（`cargo test -p mir-store -- --test-threads=1`） | OK | 2s |

**结论：12/12 全绿，退出码 0。** 门禁期间未修改源码；`cargo test --workspace` 的 live_world 重跑
覆盖了 `build/evidence/self-motion/`，因此按门禁当次产物冻结 D 批次（见 §3）。

## 3. D 批次证据冻结（提交选定）

- 批次：**D**（最终提交批次），来自 2026-10-24 00:29:48 门禁当次 `live_world` 重跑
- 动作链：**Down**，2 次 Walk + 1 次 Run；权威坐标 `(330,270)→(330,271)→(330,272)→(330,274)`；
  18 个时刻样本 / 18 张 PNG；自机锚点恒 `(384,256)`
- manifest：`docs/testing/self-motion/frozen/evidence-manifest.json`
  （sha256 `a3e874a9e33ead3606bcbd8b5c0cd79b911abb70721585f1847d78592dd7b40a`）
- 本地冻结副本：`target/self-motion-frozen-final-20261024T0030/`（19 个文件：manifest + 18 PNG）
- 哈希清单：`docs/testing/self-motion/frozen-evidence.sha256`（20 条 = manifest 的 build 原路径 + docs 入库副本 + 18 PNG；逐条校验 0 mismatch）
- **D 的冻结集只有 manifest + 18 PNG（19 文件），不含独立 log**：D 运行记录以本文件 §2 与本地 `target/final-gate-20261024-002851.log` 为准（该日志为本地产物，不入库、不随 19 文件冻结）；A 的 `live-world-run.log` 是 21:53 独立归档，不与 D 同批。

批次关系（不得混为同一次运行）：

| 批次 | 时间 | 产物 | 状态 |
|---|---|---|---|
| A | 2026-10-23 21:53:57（17.88s） | `docs/testing/self-motion/live-world-run.log` | 独立运行归档；与 D 同参数、不同批次 |
| B | 2026-10-23 21:54:05 | `build/evidence` 临时 DownRight 覆盖 | 已被后续覆盖，仅历史 |
| C | 2026-10-23 22:00 门禁重跑（manifest 22:01:07） | 旧 manifest `c08e2bc2…cf8` | **历史参考**，不作为提交选定 |
| **D** | **2026-10-24 00:29:48 最终门禁** | `frozen/evidence-manifest.json` + 18 PNG | **本轮提交选定** |

## 4. 残余限制（如实保留）

- 四类真实窗口边界（中央移动 / 触边回拉 / 沿边 / 反向离边）**未实测**；`frame-comparison.md` §8 逐项说明 + F2 复现步骤。
- ≥600ms/跳变无残留仅有代码级断言，无窗口录屏佐证。
- `live_world.rs` capture 仍直接 `apply_camera`，**未走 `prepare_frame`**；延迟/打断回归以 camera 12 项单测为准，live 只作生产链路分层证据。
- 本轮**未测 FPS/帧耗时**；`docs/client.md` 中性能数字仅第 13 轮历史数据，不适用本轮。
- 96 行 CSV 是参考对照表，不是 8 方向 × 6 帧 × 2 动作的全组合动态覆盖；live 只抽 Down 单方向链。

## 5. 关联

- 行为说明：`docs/client.md` §4 / §7 / §8
- 证据索引：`docs/testing/self-motion/README.md`
- 逐帧对照：`docs/testing/self-motion/frame-comparison.md` / `.csv`
- 后续体验清单：`docs/client-experience-backlog.md`
