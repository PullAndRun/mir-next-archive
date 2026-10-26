# R63 570 严格时钟与跨代缓存顺序

结论：**PASS-WITH-PARTIAL，范围仅限 G1+G2**。本轮完成 570 命中特效严格 NextFrame 状态时钟（G1）与跨代首批位置缓存刷新顺序（G2）；生产 `SkipFrames`/ActionFeed、命中后物理回收、570 素材等仍为 PARTIAL，R63 整体不因本文档写成完整合格。终审（刑部 D4 之后）、尚书验收与门下确认**尚未进行**，也不代替父任务结论。

## 提交与门禁

| 字段 | 已核实事实 |
|---|---|
| 功能 HEAD | `1fcf4fccaf4177475af971dc50db69ffc3a3d1d9`（`1fcf4fc`），父 `f80724e` |
| 功能批 | 5 文件 +874/−66：`projectile.rs` +333/−34，4 个测试文件；无 net/app/协议改动 |
| 门禁 attempt-1 | `pwsh -NoProfile -File tools/dev/ci.ps1 -KillStale`，**EXIT=0，12/12 OK** |
| 原始日志 | [attempt-1.log](../../../target/round63/gate/attempt-1.log)，190,207 B，SHA256 `15adfa307592ef1ab3ea4cec4df248a852c62c814974c8d5043c8f52e2ac1656` |
| UTC 起止 | `2026-10-26T15:40:38Z`–`15:44:34Z`；meta 见 [attempt-1.meta.txt](../../../target/round63/gate/attempt-1.meta.txt) |
| 发布 | 未 commit 文档、未 push；push 待 Lead 终审后统一执行 |
## 子单与角色映射

| 子单 | 角色 | 交付 |
|---|---|---|
| `JJC-20261026-010` | Lead | 规划/派发、唯一门禁、提交、收口 |
| `-sub-823eaf5f` | 兵部 A（bingbu-hit-a） | G1/G2 实现；lib 红探针 0/2 → 绿 24/0，clippy/fmt 0 |
| `-sub-1f51f226` | 工部 B（gongbu-wire-b） | 新 wire 3 tests + 历史 elapsed 断言改写；基线真红 1p/2f exit101 → 绿 3/0 |
| `-sub-da9402eb` | 刑部 C（xingbu-r63-c） | 独立复跑一致、真红复现、禁称/ignore 核对；登记 D1–D4 |
| `-sub-5d0de186` | 礼部 D（libu-r63-d） | 本文档、manifest、脚本、client/roadmap 更新 |

## 测试矩阵与红绿证据

| 项 | 结果 |
|---|---|
| `cargo test -p mir-client --lib ui::projectile::tests` | 24 passed / 0 failed |
| `--test round63_projectile_wire` | 3 passed（新增，红绿同 SHA `6438cce5…`） |
| `--test round59_projectile_render` | 16 passed |
| `--test round61_projectile_queue` | 11 passed |
| `--test round62_projectile_wire` | 5 passed |
| `cargo clippy -p mir-client --all-targets -D warnings` | exit 0，0 诊断 |
| `cargo fmt --all -- --check` | exit 0，无 diff |

- 基线红（`f80724e` 纯净树 + 新测试同 SHA）：**exit 101，1 passed / 2 failed**——`round63_projectile_wire.rs:182` left 571 / right 570（等号不进未生效）、`:290` left 1 / right 2（首批位置被跨代 reset 清掉）。证据 [baseline-red-final.log](../../../target/round63/b/attempt-1/baseline-red-final.log)。
- 功能树绿：同一测试文件 SHA `6438cce5f43358e5ede1195ae357c9041b2645662bc49df1b05b307ccf1c0ded`，3/0/0 exit0；测试文件 SHA 见 [test-files.final.sha256](../../../target/round63/b/attempt-1/test-files.final.sha256)（红/绿同 SHA）。
- A 侧 lib 红探针（实现前）：0 passed/2 failed exit101；C 独立 clean 重跑五个 target 计数与 A/B 自报逐项一致。原始日志：[A](../../../target/round63/a/attempt-1/)、[B](../../../target/round63/b/attempt-1/)、C 报告原件 `target/round63/c/round63-c-evidence.md`。

## Crystal 依据

- `github/Crystal/Client/MirObjects/Effect.cs:38-48,81-96`：`NextFrame=Start+60`，`now<=NextFrame` 不进；skip 到期先 +1 再普通 +1；`frame>=10` 删除。
- `github/Crystal/Client/MirScenes/GameScene.cs:4701-4710`：570 建立条件与 Owner=命中目标；绘制只读 frame，不推进时钟。

## 改动模块边界与 supersede

- 只改 `crates/mir-client/src/ui/projectile.rs` 与 4 个测试文件；未改 wire/total/游标/发号；`owner_skip_frames` 生产仍恒 false，未扩 ActionFeed/LRU。
- R59/R61/R62 的 elapsed 断言按「**R63 G1 supersede**」改为严格 NextFrame 逐边界；测试名与数量 16/11/5 未减，历史冻结 R59/R61/R62 证据不覆写；R63 新增 wire 3 tests。

## PARTIAL 清单（不得冒充完成）

1. **生产 SkipFrames/ActionFeed 未接**：`player_skip_frames` 恒 false；Crystal `PlayerObject.cs:750,966-967`、`MonsterObject.cs:346,528-529` 的 FIFO 来源仍缺。
2. **命中后物理回收/事件表减容未做**：无服务端回收协议；append-only 继续。
3. **`known_positions` 无容量上限/LRU**：G3 只交契约（见下），不写随意常数、不宣称已限容。
4. **570 素材不入库**：素材属 `game-assets/`，本仓库无 570 资源；音效/渲染素材边界继续 PARTIAL。
5. **范围限定**：严格 NextFrame 仅本模块 570；不由此推导飞行/渲染/音频全链已证；R61 12 项 PARTIAL 按原编号承接。
6. **禁称**：不得写「生产 SkipFrames 已接」「缓存已限容」「125 全链已证」「真实可听」「R63 完整」。

## G3 容量/回收契约（仅文档）

Crystal 无全局位置缓存 LRU、125 无回收 ack 字段，故不发明协议：只保护仍被活跃引用/本代次需要的位置，仅淘汰无引用历史项；miss 不造坐标、不补播，并披露可能少生成。容量数值、LRU 更新时机、换图/ID 复用失效、保护项超额策略待后续实证；物理裁剪须先定义同代绝对序号区间、所有消费者最低确认水位、慢消费者/克隆/重连/total 饱和语义，当前维持 append-only/PARTIAL。

## 偏离登记 D1–D4

- **D1**（证据登记）：A `summary.md` 头部写 `projectile.rs` SHA `013c475…`，为 stale 字段；canonical 为 `8c062cb4…`（工作树和提交 blob 一致，结论不受影响）。
- **D2**：B `round63_test.sha256` 为中间稿；canonical 改引 `test-files.final.sha256`（`6438cce…`，与红/绿两侧一致）。
- **D3**（方法论）：共享 `CARGO_TARGET_DIR` 跨 workspace 会复用陈旧集成测试二进制（假绿/假红）；门禁复跑须先 `cargo clean -p mir-client` 或隔离 target。
- **D4**（C 副作用，已披露）：C 为复现/清理执行 `cargo clean -p mir-client`，移除 93,520 files / 45.8 GiB 可再生成产物，随后 clean 重建全绿，源码/证据未受影响。

## 复跑与验收待办

```bash
bash docs/testing/round63/run-checks.sh --list
bash docs/testing/round63/run-checks.sh
py -3 -c "import json;json.load(open('docs/testing/round63/final-manifest.json',encoding='utf-8'))"
bash docs/testing/round63/gen-tree-hashes.sh
```

- D 脚本实跑记录：`target/round63/d/attempt-1/summary.tsv`（16 项，计数见该清单）。待办：① 终审（刑部 D4 之后）；② 尚书验收；③ 门下确认；④ Lead 决定 push。以上未完成前不得预写整体结论。
