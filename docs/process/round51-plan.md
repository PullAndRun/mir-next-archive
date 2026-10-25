# R51 中书省方案：技能首批补全与商城文档同步

> 主任务：`JJC-20261025-003`。本轮只做 Crystal 已有语义的 Rust 移植和 P12 文档纠偏，不新增玩法。

## 一、事实依据

### Crystal 上游锚点

| 事实 | Crystal 锚点 | 判定 |
|---|---|---|
| 技能默认资料含 Blink | `github/Crystal/Server/MirEnvir/Envir.cs:931-934` | 已实现 |
| 技能默认资料含 StormEscape | `github/Crystal/Server/MirEnvir/Envir.cs:935-942` | 已实现 |
| 技能默认资料含 ShoulderDash | `github/Crystal/Server/MirEnvir/Envir.cs:333-339` | 已实现 |
| FrostCrunch 默认资料 | `github/Crystal/Server/MirEnvir/Envir.cs:711-718` | 已实现 |
| ThunderStorm 默认资料 | `github/Crystal/Server/MirEnvir/Envir.cs:729-736` | 已实现 |
| PoisonCloud 默认资料 | `github/Crystal/Server/MirEnvir/Envir.cs:1247-1254` | 已实现 |
| TrapHexagon 默认资料 | `github/Crystal/Server/MirEnvir/Envir.cs:1097-1104` | 已实现 |
| 人物施法分派七技能 | `github/Crystal/Server/MirObjects/HumanObject.cs:3577-3657` | 已实现 |
| 雷系效果与 StormEscape | `github/Crystal/Server/MirObjects/HumanObject.cs:4264-4340`、`:6079-6114` | 已实现 |
| PoisonCloud/TrapHexagon 效果 | `github/Crystal/Server/MirObjects/HumanObject.cs:4543-4635`、`:4596-4665` | 已实现 |
| 地图效果含 ThunderStorm/StormEscape | `github/Crystal/Server/MirEnvir/Map.cs:1301-1432` | 已实现 |
| 地图 PoisonCloud/TrapHexagon | `github/Crystal/Server/MirEnvir/Map.cs:1434-1482`、`:1752-1819` | 已实现 |
| 宠物保存开关与经验 | `github/Crystal/Server/Settings.cs:151,433,720`；`github/Crystal/Server/MirObjects/MonsterObject.cs:3502-3515` | 上游行为，拆出本轮 |
| 客户端技能表现 | `github/Crystal/Client/MirObjects/PlayerObject.cs:1730-1968`；`github/Crystal/Client/MirScenes/GameScene.cs:11799-11945` | 已实现 |
| 格间插值 | `github/Crystal/Client/MirObjects/PlayerObject.cs:781-853`、`MonsterObject.cs:375-437` | 上游参考，非本轮全量 |

### Rust 当前锚点

| 事实 | Rust 锚点 | 判定 |
|---|---|---|
| 未实现技能统一返回 UnsupportedSpell | `crates/mir-server/src/magic.rs:188-190,1137-1140` | 缺失，R51-A |
| Blink 仅有资料查找调用 | `crates/mir-server/src/world.rs:6046-6050` | 需修正，R51-A |
| Rust 客户端已有技能枚举/文字映射 | `crates/mir-client/src/net.rs:7063-7095`；`crates/mir-client/src/ui/labels.rs:1740-1748` | 已实现基础接线 |
| Rust 客户端已有技能特效分组 | `crates/mir-client/src/ui/spellfx.rs:43-93` | 已实现基础接线 |
| ObjectWalk/ObjectRun 已按 Crystal 编号注册 | `crates/mir-protocol/src/packets/server.rs:822-879,7774-7780` | 已实现 |
| GameShopInfo/GameShopStock 已注册 | `crates/mir-protocol/src/packets/server.rs:5959-5972,7955-7956` | 已实现 |
| GameShopInfo 编解码与 Crystal 注释 | `crates/mir-protocol/src/manual/server.rs:1089-1118` | 已实现 |
| 宠物下线目前明确简化且不做 PetSave/重召 | `crates/mir-server/src/world.rs:1927-1929` | 缺失，列后续/风险 |
| Rust 协议已有 ClientMovementInfo | `crates/mir-protocol/src/data/client_movement_info.rs:21-36` | 已实现 |
| Rust 技能测试仅断言 UnsupportedSpell | `crates/mir-server/src/magic.rs:6808-6812` | 需扩展为行为证据，R51-A |
| P12 文档仍有旧 HEAD/返工中描述 | `docs/world/gameshop.md:39,384`；`docs/client.md:909` | 需修正，R51-B |

结论：R51-A 以服务端技能语义和包/效果接线为主；R51-B 只同步已交付商城事实；R51-C 做协议与客户端表现的证据闭环。AOI 全量、攻城、寻路/天气/光照、任务 NPC、行会和宠物持久化不在本轮。

## 二、可判定目标

- **R51-A 技能语义**：七技能不再落入 `UnsupportedSpell`；效果、消耗、目标筛选、位置/持续时间按上述 Crystal 锚点镜像。验收：`cargo test -p mir-server magic -- --nocapture`；证据 `crates/mir-server/src/magic.rs`、`world.rs` 测试输出与 Crystal 对照表。
- **R51-B P12 文档**：修正商城协议、服务端包链、独立面板、库存/库存刷新与当前 HEAD 状态；删除“返工中/旧 HEAD”陈述。验收：`node scripts/validate-docs.mjs`（若存在）或文档锚点扫描命令；证据 `docs/world/gameshop.md`、`docs/client.md` diff 与扫描日志。
- **R51-C 客户端/协议接线**：七技能效果包与已有 `ObjectWalk/ObjectRun`、商城包注册保持编号/编解码一致，补最小回环或快照测试，不改变包号。验收：`cargo test -p mir-protocol` 与针对 `mir-client` 的现有测试命令；证据 `crates/mir-protocol/src/packets/server.rs`、`manual/server.rs`、`crates/mir-client/src/net.rs`。
- **R51-D 门禁与可追溯性**：两次提交分别为文档批、功能批；队长保留全量门禁原始 exit code、起止时间、日志 SHA。验收：`git log -2 --oneline`、全量既有 validate/CI 命令；证据写入主任务 memo 和审计记录。

## 三、子任务 DAG

1. **礼部 / libu（gpt）**：先行文档批。写范围：`docs/world/gameshop.md`、`docs/client.md`、必要 `docs/process/*`；不得改 crates。依赖：无。交付：P12 锚点同步、验证命令和文档批提交。
2. **兵部 / bingbu（deepseek）**：技能服务端与必要协议接线。写范围：`crates/mir-server/src/magic.rs`、`world.rs` 及对应测试；协议/客户端仅限已确认的包与效果接线文件。依赖：读取 Crystal 对照与礼部文档批，不改文档。交付：功能批、定向测试、行为对照表。
3. **刑部 / xingbu（gpt）**：独立审计。写范围：只写 `docs/audit/round51*` 或主任务审计附件，不改实现。依赖：R51-A/B/C 完成。交付：逐项验收、Crystal/Rust 差异、残余风险。
4. **尚书省 / shangshu**：登记委派、串行合并两批、运行队长全量门禁；写范围为提交与主任务材料，不改六部专属文件。

依赖图：`libu → 文档批 → bingbu → xingbu → shangshu 全量门禁`；若功能不依赖文档实际内容，可由尚书确认后并行，但提交顺序仍固定为文档批、功能批。

## 四、提交与门禁策略

- 提交 1（文档批）：仅文档与文档验证证据；提交前 `git diff --check`，记录原始 exit、开始/结束时间、日志 SHA。
- 提交 2（功能批）：技能/协议/客户端接线与测试；不得混入旧 P02-P08/P10/P12 以外改动；记录同样门禁元数据。
- 队长唯一执行全量门禁；六部只执行定向测试。不得跑未授权的 cargo/CI 于中书阶段。
- Review 前尚书逐个读取子任务 `show`、todo 与 `delegation_result`；刑部独立核验后方可请求 `Review → Done`，由门下按 `pending_confirm.confirm_by` confirm。

## 五、风险与 PARTIAL 预案

- 技能协议字段若 Crystal 与现有 Rust 包边界不一致，保留现有包号/编码，记录为 PARTIAL，不猜字段。
- 客户端缺少可视化素材或运行时入口时，先交服务器行为与包回环证据，列客户端表现残余，不伪造 PASS。
- 护身符消耗、PetSave/PetExp、AOI 全量验收、攻城及其控制点/城门修理、寻路天气光照、坐骑翅膀、小地图 NPC、CallDefaultNPC、行会、attack 像素证据、K 键重绑均保持未完成状态并回填 R52。
- 任一门禁失败：保留失败原始日志与 SHA，退回对应子任务；不以文档通过替代功能通过。

## 六、明确不做清单

本轮不做：护身符消耗；PetSave/PetExp；AOI 全量验收；攻城旗子/控制点/城门修理/领地；他人/怪物格间插值剩余、寻路、天气/光照、坐骑翅膀、小地图 NPC；任务 `CallDefaultNPC`；行会补全；R49 P05 attack 3/25 帧像素差；R49 P08 K 键重绑；新玩法、改包号、改 Crystal 未有的数值或规则。

## 七、待门下裁决项

1. Crystal 的技能效果若缺少 Rust 现有实体/包字段，是否准许按现有协议最小保真降级并标 PARTIAL？
2. 客户端视觉缺素材时，是否以协议回环与效果枚举测试作为 R51-C 最低证据？
3. 文档批是否允许在功能批前合并并作为唯一事实基线？
4. 全量门禁是否沿用 R50 队长命令集，还是由尚书在功能批后补充本轮命令清单？
5. PetSave/PetExp 是否明确登记为 R52 主单，避免被误报为 R51 缺陷？
