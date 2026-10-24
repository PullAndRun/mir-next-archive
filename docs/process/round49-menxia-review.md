# 第49轮门下省评审

- 评审任务：`JJC-20261025-001`
- 评审角色：门下省（Menxia）
- 评审对象：`docs/process/round49-plan.md`
- 评审日期：2026-10-25
- 结论：**PASS-WITH-CHANGES（有条件通过，先重规划再执行）**

## 一、结论与放行边界

方案的 Crystal 事实基础和 Rust 协议判断基本正确，可以作为重规划的基础；但当前方案不能按原样同时启动怪物 WIP 收口和独立商城实现。工作区已有 45 个跟踪文件改动、未跟踪帧表/测试/证据目录，且旧任务 `JJC-20261024-004` 已在 Doing 并持有工部子任务。商城跨服务端、客户端、协议消费、素材和持久化验收，挤入同一正式 R49 会让写入范围、门禁证据和提交边界不可审计。

本评审的放行条件如下：

1. **正式 R49 主线采用 `JJC-20261024-004`**，只收口当前怪物原生帧表与走/跑插值 WIP，保留其正在执行的工部子任务；不得另开一条并行怪物实现线。
2. **`JJC-20261025-001` 改写为 R49 的门下评审、WIP 冻结、证据一致性和拆分治理任务**。本任务不在 R49 派发商城代码；R49-03/R49-04 的独立商城实现转为 R50 关联计划，协议结构事实和服务端事务风险可以保留为 R50 前置审计材料。
3. R49 必须按当前门禁脚本生成的实际数量验收。方案写的“12/12”不能继续作为固定门槛；本轮已知实测为 12/13，唯一失败是文档链接与包编号，且 `docs/testing/monster-motion/README.md` 引用的 `self_moving.png`、`landed.png` 尚未生成。旧 `target/gate-r49-d.log` 已因后续变更过期，不能当绿证据。
4. 商城 R50 只有在素材来源/授权、真实包链和事务失败证据冻结后才能开工；R49 不得以现有市场页签冒充独立商城包链。

## 二、事实核对：Crystal 引用与 Rust 现状

### 2.1 怪物帧与移动引用真实有效

- `github/Crystal/Client/MirObjects/Frames.cs:22-32` 确实是 `DefaultMonster`：Standing `0/4/0/500`、Walking `32/6/0/100`、Attack1 `80/6/0/100`、Struck `128/2/0/200`、Die `144/10/0/100`、Dead `153/1/9/1000`、Revive 反向播放。
- `github/Crystal/Client/MirObjects/MonsterObject.cs:274-276` 确实按 `BodyLibrary.Frames ?? FrameSet.DefaultMonster` 选择帧表；`:345-359` 计算绘制帧。
- `github/Crystal/Client/MirObjects/MonsterObject.cs:364-424` 确实按 Walking/Running 等动作计算 `Movement`、`Frame.Count`、八方向 `OffSetMove`，并执行 `x % 2 + x` 的 C# 余数语义；`:433-437` 先减实体自身 `OffSetMove` 再叠加玩家偏移。
- `github/Crystal/Client/MirObjects/MonsterObject.cs:522-545` 确实在动作切换时使用 `Frame.Interval`，并为 Walking/Running 以服务端落点前一格/两格建立移动起点。方案引用没有把行号或语义写错。

事实清单中御史台 X1 提出的四项 high（负 Skip/stride=0、真实 Count×Interval 生命周期、每怪帧表进 session、`OffSetMove` 符号）已由 Lead 抽查确认在现有 WIP 中修复；但修复是否进入正式提交仍须由 R49 主线重新冻结和验收。

### 2.2 商城包线与登录/库存语义真实有效

- `github/Crystal/Shared/ServerPackets.cs:5348-5391` 的 `S.GameShopInfo` 先写包路径 `GameShopItem.Save(writer, true)`，再写 `Int32 StockLevel`；`S.GameShopStock` 写 `GIndex`、`StockLevel` 两个 `Int32`。
- `github/Crystal/Server/MirObjects/PlayerObject.cs:1203-1206` 登录流程调用 `GetGameShop()`；`:13913-13944` 遍历 `Envir.GameShopList`，按个人/全服已购计数计算库存并逐条发 `S.GameShopInfo`。
- `github/Crystal/Server/MirObjects/PlayerObject.cs:13703-13721`、`:13821-13862` 对应库存刷新、扣款、计数推进和成功购买后的有限库存刷新；`:13725-13814` 是数量、商品、库存、支付方式和余额校验。库存不足和事务失败的包时机必须分别验证，不能混为成功。
- `github/Crystal/Client/MirScenes/GameScene.cs:2042-2046` 消费两个包；`:6679-6697` 按 GIndex 添加、移除或更新目录项；客户端购买字段见 `github/Crystal/Shared/ClientPackets.cs:2434-2454`。
- Rust 侧协议已存在且线格式正确：`crates/mir-protocol/src/manual/server.rs:1089-1127` 的 `GameShopInfo` 使用包路径 `GameShopItem::decode/encode` 与 i32 库存；`crates/mir-protocol/src/packets/server.rs:5959-5986` 的 `GameShopStock` 使用两个 i32，索引为 250/251。方案称“协议结构已存在”属实，不应重写 432 包协议。
- 现状仍只有市场页签：`crates/mir-server/src/market.rs:62-99` 明确本作不发 `S.GameShopInfo`/`S.GameShopStock`。既有购买事务在 `crates/mir-server/src/session.rs:3030-3205`、`crates/mir-server/src/market.rs:2433-2589,2832-2918` 已有锁、计数、扣款、发货、落盘和 DB 失败回滚路径；R50 需在其上补独立包下发和失败包序证据。

### 2.3 文档和优先级事实

- `docs/roadmap.md:47` 仍写明商城只有市场页签，独立 `S.GameShopInfo`/`S.GameShopStock` 未做；`:101-116` 的优先级把 i18n 收口和商城收尾排在技能、AOI 之前，客户端插值在其后。
- `docs/client-experience-backlog.md:10-15` 将他人/怪物移动插值列为 P1、怪物逐图像帧表列为 P2，支持当前 WIP 先收口的必要性。
- `docs/i18n.md:247-275` 的四个基线是 `189/22/69/15`，CI 口径 `--player-visible --strict` 上限为 70，文档口径 `--no-test --strict` 当前为 15；`docs/i18n.md:281-309` 说明这 15 处是开发者/数据文本，不能写成“玩家可见全部归零”以外的事实。
- `docs/client.md:1315-1330` 已把 R49 B/C 写成已接入怪物原生帧表和走跑插值；但 `docs/client.md:1761-1822` §8 仍列 `!Mon.dat` 精确帧表和独立商城面板为未完成，商城现状也与 `docs/world/gameshop.md:33-52` 的“市场页签、独立面板未做”一致。两处必须在 R49 文档收口时明确“已做/仍未做/本轮移出”的边界。

## 三、必须修改的事项（不超过五项）

### 1. 高：解除 R49 双主线撞车并收窄正式范围

**证据：**方案 `docs/process/round49-plan.md:3,8-13,34-36,60-138` 同时覆盖 45 文件 WIP 和服务端/客户端商城；看板实际显示 `JJC-20261024-004` 为 Doing、3/4，子任务 `JJC-20261024-004-sub-f9748d4c` 为 Doing；`JJC-20261025-001` 为 Menxia、0/0。

**必须改：**将 `JJC-20261024-004` 作为 R49 正式实现主线，只收口怪物 WIP；将 `JJC-20261025-001` 改为冻结、审计和拆分治理任务；R49-03/R49-04 移至 R50 关联计划，不得在两个任务中重复写 `crates/mir-client/**`、怪物渲染文件或同一证据目录。若未来保留 R50 客户端任务，必须重新声明其与 R49-02 的文件边界和串行依赖。

### 2. 高：把固定“12/12”改为当前门禁产物驱动的 fail-closed 判据

**证据：**方案 `docs/process/round49-plan.md:134-138` 固定写 12/12；当前事实清单实测 `tools/dev/ci.ps1` 为 12/13，唯一失败为文档链接与包编号，`docs/testing/monster-motion/README.md` 引用的 `self_moving.png`、`landed.png` 缺失，证据测试因此中断；旧 `target/gate-r49-d.log` 是变更前的 12/12。

**必须改：**R49-06 读取本次生成的 gate manifest/exit code，记录实际测试总数和通过数；缺任何必需 PNG、ignored 真实素材证据、链接检查或 PARTIAL 声明时维持 `PARTIAL`，不得以旧日志、`SKIP` 或改数字变绿。全量门禁仍只能由一人运行；当前正在重跑的 ignored 证据结束后必须再跑一次完整门禁。

### 3. 中高：统一 R49 文档、i18n 和“已完成”口径

**证据：**`docs/client.md:1315-1330` 已写 R49 B/C 完成，但 `docs/client.md:1783-1808` 仍写怪物精确 `!Mon.dat` 和独立商城未完成；`docs/world/gameshop.md:33-52` 也明确当前只有市场页签；`docs/i18n.md:267-275` 的 no-test strict 仍为 15。

**必须改：**R49-05 只把已被本轮真实证据覆盖的怪物 WIP 标为完成，保留 `!Mon.dat` 精确映射、坐骑/Push/Jump/Dash 和独立商城为未完成/移至 R50；更新 §8 使它与 §6、`docs/roadmap.md`、`docs/world/gameshop.md` 一致。商城新增文字若留在 R50，不能提前计入 R49 的 i18n 通过；所有新增玩家可见文字仍必须走 `TextKey`/`Label` 参数化 API。

### 4. 中高：冻结商城 UI 素材来源、授权和缺图回退

**证据：**方案只在 `docs/process/round49-plan.md:49,108,112-113` 写“缺图标占位”和截图路径，没有素材清单、哈希、来源或授权门槛。当前资源检索能找到的商城图像主要位于 `resource/community/lomcn-chinese-knockoff-assets/...`、`resource/community/etorth-mir-like-res-8-directions/...` 等社区/仿制资源，不能直接当作 Crystal 原作素材。

**必须改：**R50 计划新增资产 manifest（路径、来源、哈希、授权/可用范围、目标尺寸）和稳定占位策略；未确认来源的社区或仿制图标不得进入产品或真实截图。缺素材时可以用统一占位完成协议/UI逻辑证据，但截图和报告必须标注 `PARTIAL`，不能声称 Crystal 视觉对齐。

### 5. 高：把商城包序、库存和 DB 失败契约写成可证伪测试

**证据：**方案 `docs/process/round49-plan.md:44-50,92-113,148-155` 已提出包序、个人/全服库存和失败回滚，但未把每个结果的发送边界写成独立断言；Crystal `PlayerObject.cs:13703-13721,13821-13862` 区分库存不足/成功刷新，Rust 当前 `market.rs:96-103` 仍明确不发两个独立包。

**必须改：**R50-03/R50-04 验收必须逐项断言：登录每个可用 GIndex 只发一次 `S.GameShopInfo`；有限库存按个人/全服计数计算，库存为零按 Crystal 语义处理；成功购买只在事务提交后按条件发 `S.GameShopStock`；库存不足按 Crystal 负例刷新；DB/邮件/角色落盘失败不发虚假库存，且内存扣款/发货回滚；客户端按 GIndex 幂等消费迟到/重复包。测试必须走真实 `S.GameShopInfo`/`S.GameShopStock` 链路，禁止静态目录、抓包改包和伪造库存。

## 四、R49 子任务调整

| 原任务 | R49 处理 | 调整后的 owner / 写入范围 / 依赖 |
|---|---|---|
| R49-01 | 保留 | gpt-6.1-sol 负责 WIP 冻结与 Crystal 审计；只写 `docs/testing/round49/**` 的冻结产物，不改 `crates/**`；作为 004 WIP 收口前置 |
| R49-02 | 保留并归入 004 | deepseek/工部编码；沿用 `crates/mir-asset/**`、`crates/mir-render/**`、`crates/mir-client/src/{net.rs,camera.rs,app.rs,main.rs}` 及对应测试；保留 X1 修复和真实素材 PARTIAL 语义 |
| R49-03 | 移到 R50 | deepseek/兵部；未来只写服务端实现/测试，协议仅在线格式确有差异才改；前置依赖真实 GameData/DB 计数和 R49 冻结报告 |
| R49-04 | 移到 R50 | deepseek/工部；未来只写客户端商城 UI/网络消费和测试；不能与 R49-02 并行改同一 `mir-client/src/**` 文件 |
| R49-05 | 收窄 | gpt-6.1-sol 负责 WIP 文档、i18n 与证据一致性；需修正 `docs/client.md` §8 和 R49 manifest；与 R49-01 的 `docs/testing/round49/**` 写入串行 |
| R49-06 | 收窄 | gpt-6.1-sol/刑部只读终审；只写 `target/round49/**`、最终 manifest；按本评审的当前 gate manifest 规则验收，不能固定 12/12 |

最新所有者指令要求 gpt-6.1-sol 承担全部非编码工作，因此方案中“libu/deepseek 负责文档/素材”和旧制度中“deepseek 执行层”的冲突不作为本评审依据；后续派工应按当前指令路由：deepseek 只编码，gpt 负责计划、审计、证据、文档、风险和决定。

## 五、R49 验收标准

### 5.1 谁验

- 工部（deepseek）完成怪物 WIP 代码、单测和真实素材 ignored 证据；不改变验收标准。
- gpt-6.1-sol 负责本轮非编码冻结、Crystal 对照、文档/i18n 审计、证据审计和最终 PASS/PARTIAL 判定。
- 刑部执行只读门禁核对并保存命令输出；任何失败、缺产物或路径/哈希不一致都回报 PARTIAL，不代修。
- 尚书省只在全部证据可复查后验收 004；门下省对 R49 结果独立复核，未通过不得提交。

### 5.2 必跑命令与证据

R49 WIP 收口至少必须有以下命令的原始退出码和摘要：

```text
cargo test -p mir-asset --test real_framesets
cargo test -p mir-render --test monster_native -- --ignored --nocapture
cargo test -p mir-render --test monster_native_evidence -- --ignored --nocapture
cargo test -p mir-client --lib camera::monster_motion_tests
cargo test -p mir-client --test monster_motion -- --ignored --nocapture
cargo test -p mir-tools --test monster_native_import -- --ignored --nocapture
py tools/i18n/scan_hardcoded.py --player-visible --strict --fail-above 70
py tools/i18n/scan_hardcoded.py --player-visible --no-test --strict
py tools/docs/check_links.py
git diff --check
pwsh -NoProfile -File tools/dev/ci.ps1 -KillStale
```

证据必须落在：

- `docs/testing/round49/wip-freeze.json`
- `docs/testing/round49/monster-wip.json`
- `docs/testing/round49/i18n.log`
- `docs/testing/round49/manifest.json`
- `target/round49/final-gate.log`
- `docs/testing/round49/final-manifest.json`
- 既有 `docs/testing/monster-native/`、`docs/testing/monster-motion/`、`docs/testing/native-monster/` 目录中的真实素材证据和 fallback 说明。

### 5.3 什么算 PARTIAL，什么才算绿

以下任一项存在时只能标记 `PARTIAL`，不能标绿或提交：

- ignored 真实素材测试未运行、因缺素材中断，或所需 PNG（当前已知 `self_moving.png`、`landed.png`）不存在；
- `tools/dev/ci.ps1` 任一门禁失败，尤其链接/包编号检查失败；
- 只存在旧日志，缺少本次工作树对应的命令退出码、哈希和 manifest；
- 文档仍同时声称 R49 B/C 已完成和未完成，无法区分本轮已收口与 `!Mon.dat`/坐骑等边界；
- 证据使用静态商城 fixture、抓包改包、GM/改 DB、伪造库存，或任何捷径黑名单操作；
- 任何一个 WIP 文件、未跟踪资产或回退语义未被冻结清单覆盖。

只有所有必跑命令使用同一冻结工作树、退出码全为 0、证据路径/哈希可复查、PARTIAL 边界与文档一致，并经尚书验收和门下复核，R49 才算绿。商城独立面板不属于 R49 绿的必要条件；它必须在 R50 单独通过真实包链和事务验收。

## 六、风险与回滚

- 先保存 `git status --porcelain`、`git diff --stat`、tracked/untracked 哈希和证据目录清单；任何回滚只反应用于 004 WIP 的 scoped patch，禁止 `git reset --hard`、`git checkout --` 或删除他人未提交文件。
- R49 不改数据库迁移、不以 fixture 商品或 GM 命令制造成功；R50 商城必须沿用已有 `lock_game_shop`、`gameshop_buy`、`flush_gameshop_update` 和 `rollback_gameshop_purchase` 事务边界。
- 全量门禁运行时只允许一个执行者；若已有门禁锁或正在重跑 ignored 证据，不能并行 `-KillStale`。当前 `target/gate-r49-d.log` 只作历史参考。
- 原生素材缺口、fallback、负 Skip/stride=0、动作生命周期和偏移符号必须保留在 manifest 与 README 中；任何“全量 native”表述都封驳。
- 商城素材若没有可复查授权和哈希，使用稳定占位并标 PARTIAL；禁止把社区仿制资源包装成 Crystal 原作素材。

## 七、看板决策与操作记录

本评审决定：

- `JJC-20261024-004`：保留 Doing，作为 R49 正式怪物 WIP 主线；不取消、不与新任务重复实施。现有 `JJC-20261024-004-sub-f9748d4c` 继续执行，完成后按其正常 Doing → Review → PendingConfirm → Done 路径收口。
- `JJC-20261025-001`：门下评审通过但附条件，改写为 R49 冻结/审计/拆分治理任务，转 Assigned；商城实现移出 R49，后续由尚书登记 R50 关联任务。该任务不进入 Cancelled，因此不触发高风险取消确认。
- 评审文件路径和结论写入 `JJC-20261025-001`；并在 004 任务上留下撞车处置 memo。

实际看板操作记录（CLI 唯一写入口）：

1. 首次按用户简写执行的两条 `memo`（`JJC-20261025-001`、`JJC-20261024-004`）因缺少 CLI 要求的角色位置参数而返回用法错误；随后用只读 `show` 确认两项任务没有产生 memo 或状态副作用。
2. 成功执行：
   - `node "C:/Users/diana/.dsh/skills/edict/scripts/edict.mjs" memo JJC-20261025-001 menxia "门下评审 PASS-WITH-CHANGES；评审文件 docs/process/round49-menxia-review.md；R49正式主线采用004怪物WIP；商城移至R50；12/12改为当前门禁manifest" "当前证据仍PARTIAL，须先补PNG和重跑完整门禁" --as menxia`
   - `node "C:/Users/diana/.dsh/skills/edict/scripts/edict.mjs" memo JJC-20261024-004 menxia "门下裁决：004保留为R49正式怪物WIP主线；001改作冻结审计拆分治理；不取消004；商城转R50" "禁止与001重复实施怪物或并行写同一客户端文件，仍须完成证据和门禁" --as menxia`
   - `node "C:/Users/diana/.dsh/skills/edict/scripts/edict.mjs" state JJC-20261025-001 Assigned "PASS-WITH-CHANGES：已落盘评审；004为R49怪物WIP主线，001转冻结审计拆分治理，商城移至R50" --as menxia`
   - `node "C:/Users/diana/.dsh/skills/edict/scripts/edict.mjs" delegate JJC-20261025-001 menxia shangshu "按门下评审重规划R49：将001改为WIP冻结、Crystal审计、文档/i18n一致性与门禁治理；移出R49-03/R49-04商城实现，登记R50关联计划；与004的怪物WIP及工部子任务不得重叠；保留PARTIAL边界和回滚规则" "交付修订后的round49方案/拆分说明、R50商城关联登记、无重叠owner/write scope/depends_on/验收命令，并让001可进入正常执行收口；证据引用docs/process/round49-menxia-review.md及最终show状态" --as menxia`
3. 委派返回子任务：`JJC-20261025-001-sub-03e00608`，状态 `Doing`，承接方尚书省（责任 `shangshu`），用于重规划，不实施代码。
4. 最终只读核对：`show JJC-20261025-001` 为 `Assigned`，归属尚书省，当前摘要为 PASS-WITH-CHANGES；`show JJC-20261024-004` 仍为 `Doing`、进度 3/4，子任务 `JJC-20261024-004-sub-f9748d4c` 仍为 `Doing`。因此 004 没有被取消，正式 R49 主线唯一保留；001 已合法转入治理/重规划路径。
5. `node "C:/Users/diana/.dsh/skills/edict/scripts/edict.mjs" validate` 返回 `✅ 看板校验通过：87 条任务符合 schema v3.1`。

