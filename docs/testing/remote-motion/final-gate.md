# 第 46 轮最终门禁与 D2 批次证据冻结

> 本文件记录第 46 轮「其他玩家 Walk/Run 移动插值」提交前在同一冻结候选上执行的最终 12 项门禁、
> D2 批次证据冻结与候选标识。所有条目均为实际执行结果，未执行项不写入。

## 1. 候选标识

- 父提交：`85bfc21befae1725d73982afedabefeaca09f110`（第 45 轮提交）
- Crystal 参考：`0e315fe327192afe52c3d7357ddd1f5b7e26c5b8`
- 候选清单：`target/r46-candidate-sha256.txt`（17 个文件；算法 = **git hash-object SHA-1 blob**，不是 SHA-256）
- 源码/测试（门禁前后一致）：
  - `crates/mir-client/src/app.rs` — `0b7b13f872048789386c4c7008b76645b07c1ea9`
  - `crates/mir-client/src/camera.rs` — `cc568936ecccec0243c38cc4a3089653397da404`
  - `crates/mir-client/src/net.rs` — `fb23a1549aa891a70b5d82713cb131a21ef1e7bf`
  - `crates/mir-render/src/entities.rs` — `946c03dc0a1833227b596f11943d4f4982f530fd`
  - `crates/mir-client/tests/remote_motion.rs` — `179ce140152c82b3bc555d5c2a6fd149c7d0437d`
  - `crates/mir-client/tests/remote_motion_live.rs` — `81786b11197b6731802c0b5a5b506d710b3cd7d9`

## 2. 最终 12 项门禁（`target/r46-final-gate-20261024-025242.log`）

命令（仓库根）：

```powershell
pwsh -NoProfile -File tools/dev/ci.ps1 -KillStale
```

| # | 项 | 结果 | 耗时 |
|---|---|---|---|
| 1 | `cargo fmt --all --check` | OK | 6s |
| 2 | `cargo clippy --workspace --all-targets -- -D warnings` | OK | 11s |
| 3 | `cargo test -p mir-server --test duo_smoke -- --test-threads=1` | OK | 19s |
| 4 | `cargo test --workspace -- --skip duo_ --skip fault_pg` | OK | 236s |
| 5 | 协议生成器 `--check` | OK | 2s |
| 6 | 文档链接与包编号 | OK | 1s |
| 7 | TextKey 完整性 | OK | 0s |
| 8 | 硬编码棘轮 player-visible（70） | OK | 0s |
| 9 | 硬编码棘轮 no-test（15） | OK | 0s |
| 10 | 运行时不引用老资源格式 | OK | 1s |
| 11 | 物品图标完整性 | OK | 1s |
| 12 | PostgreSQL 集成测试（mir-store 串行） | OK | 6s |

**结论：12/12 全绿，退出码 0。** 门禁期间未修改源码；`cargo test --workspace` 的
`remote_motion_live` 重跑覆盖了 `build/evidence/remote-motion/`，因此按门禁当次产物冻结 D2。

## 3. D2 批次证据冻结（提交选定）

- 批次：**D2**（提交批次），来自 2026-10-24 02:52 最终门禁当次 `remote_motion_live` 重跑
- 内容：`manifest.json` + **22 张被 manifest 引用的 PNG**（walk step1 Down 7 帧、walk step2 DownRight 7 帧、run step1 DownRight 7 帧、concurrent 1）；manifest 记录 **22 个样本**；
- 实际步链（以 manifest 为准）：**Walk1 Down `(330,270)→(330,271)`；Walk2 DownRight `(330,271)→(331,272)`；Run DownRight `(331,272)→(333,274)`**；并发 observer `330,270→331,270` 且 A 仍活动（remote from `331,272`、pixel `64,42`、screen `464,362`）；
- manifest（入库）：`docs/testing/remote-motion/frozen/manifest.json`
  （sha256 `331a775105e26dc81fb1367e6aa4cbdd5e65349dda550729698a74b0451b10aa`）
- 本地冻结副本：`target/remote-motion-frozen-final-20261024T0300/`（**23 文件 = manifest + 22 PNG**）
- 哈希清单：`docs/testing/remote-motion/frozen-evidence.sha256`（**24 条** = manifest 的 build 原路径 +
  docs 入库副本 + 22 张被引用 PNG；逐条校验 0 mismatch）
- 排除：`observer-moving-frame0.png` 未被 D2 manifest 引用（旧批次残留、历史未关联），已移至
  `target/remote-motion-observer-moving-historical.png`，**不列入 D2 冻结与哈希**。
- 同批门禁日志：`target/r46-final-gate-20261024-025242.log` → 本地冻结副本
  `target/r46-final-gate-frozen.log`，sha256 `b7be7d4b00e6be13940862a8bdc124fa75c13b3f23894c18b0c4614d40d5fcd0`
  （见 `final-gate-log.sha256`；日志本地保留、不入库）。
- 历史对照：`live-manifest.json`（22 样本，早期批次）仅作历史，不作为 D2 提交证据。

## 4. 残余限制（如实保留）

- 只做非自机 Player 的普通 Walk/Run；怪物/NPC、攻击 2/3/4、坐骑/Sprint/Pushed/Jump/Dash、
  预测/补偿队列、寻路/锁定、光照/UI/音效均未做。
- live 覆盖 **Down 与 DownRight 两段链**（Walk1 Down、Walk2/Run DownRight）、单观察者；其他方向与多观察者未实测。
- 未知对象按 R46-X1-04 选择 A：忽略、不创建实体/不建 motion，依赖发现包先建立。
- **窗口四类与 FPS/录像未测**，保留为残余风险。
- 性能只测新增插值/前处理 CPU 开销（1/50/200 远端 ×1000 帧，中位/p95），不称 FPS；
  微秒级噪声明显，阈值只防退化，不把噪声写成收益。

## 5. 关联

- 行为说明：`docs/client.md` §4 / §7 / §8
- 证据索引：`docs/testing/remote-motion/README.md`
- 逐帧对照：`docs/testing/remote-motion/frame-comparison.csv`
- 后续体验清单：`docs/client-experience-backlog.md`
