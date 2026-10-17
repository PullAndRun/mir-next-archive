# 路线与进度

以 Crystal（C#）为参照系的功能清单与当前状态。表里的「已实现」都对应仓库里带源码行号注释的
代码与自动化测试；「未做」不是「不打算做」，而是还没轮到。

图例：✅ 完成　🟡 部分　⏳ 未开始　— Crystal 没有这个功能

## 一、基础设施

| 能力 | 状态 | 说明 |
| --- | --- | --- |
| 协议编解码（432 个包） | ✅ | 408 个由 C# 源码自动生成、24 个手写；每个包默认值 `encode_frame → decode` 往返 |
| 枚举与位掩码 | ✅ | `[Flags]` 枚举生成为位掩码新类型（会长 255、`Warrior\|Wizard` 组合值不丢位） |
| 帧格式 / gzip | ✅ | 长度前缀 + `int16` 包号；只有 `NPCGoods` 压缩 |
| 多语言 | ✅ | 默认简体中文，`/language en` 切换并随账号保存 |
| 存档 | ✅ | PostgreSQL（账号/角色/背包/装备/技能/金币），口令算法与 Crystal 一致 |
| 素材索引 | ✅ | `resource/` 233 万文件进 `mir2_dev`，`tools/asset-index` 查询 |
| 协议生成器自检 | ✅ | CI 重新克隆 Crystal 并校验生成结果一致 |

## 二、世界与玩法

| 能力 | 状态 | 说明 |
| --- | --- | --- |
| `.map` 地形（9 个版本） | ✅ | 可走性、门/光照/矿区字段解析 |
| 行走 / 跑步 / 转身 | ✅ | Crystal 计时（600/600/350ms）、先走后跑、`CellTime` 宽限 |
| 视野与广播 | 🟡 | 玩家/怪物/物品/NPC 的进出同步；半径仍为 10（Crystal 16），AOI 是全量扫描 |
| 怪物 / 刷新点 | ✅ | 从 `Server.MirDB` 读 555 个模板，比奇省 359 组刷出 1922 只；死亡后按 `delay+random` 补刷 |
| 战斗 | ✅ | 两段式攻击、护甲/闪避/暴击、RNG 顺序照抄；PK 未做 |
| 经验 / 升级 | ✅ | `ExpList.ini` 真表、高等级衰减、属性重算、包与 Crystal 一致 |
| 死亡 / 复活 / 回血 | ✅ | 10 秒原地复活（Crystal 走客户端 `C.TownRevive`，本作先自动）、10 秒 3% 回血 |
| 物品 | ✅ | 掉落表、地面物品、拾取/丢弃/丢金币、46 格背包、14 装备槽、装备加成、消耗品使用、存档 |
| NPC | ✅ | 660 份脚本解析、对话条件/动作子集、买卖修（定价公式与价格倍率）、`NewMapInfo` 下发 |
| 技能 | ✅ | 用书学习、施法校验、8 个伤害/治疗技能 + 9 个增益技能（护盾/加速/减伤…） |
| Buff | ✅ | 带到期时间的属性修饰层、`S.AddBuff`/`S.RemoveBuff`、药水增益（见 `docs/world/buffs.md`） |
| 任务 | ⏳ | `Envir/Quests` 与 `QuestInfo` 头已能读 |
| 行会 / 组队 / 攻城 | ⏳ | 协议与数据类已就绪 |
| 仓库 / 邮件 / 拍卖 | ⏳ | 协议与数据结构已就绪 |

## 三、客户端

| 能力 | 状态 | 说明 |
| --- | --- | --- |
| 图库解码 | ✅ | `.Lib`（4 种变体）与 `.wzl+.wzx`；调色板/565/RGB24/DXT1/gzip RGBA |
| 等距地图渲染 | ✅ | Crystal 几何常量与图层映射，真实 `0.map` 出图 |
| 联机 | ✅ | 登录/建角/进世界、30+ 包快照、KeepAlive |
| 实体渲染 | ✅ | 玩家/怪物/NPC/物品精灵（`hum`/`monN`/`npc`/`items` 映射表） |
| 操作 | ✅ | 走/跑/转向/攻击/拾取/聊天 |
| UI（状态栏/聊天/背包/商店/技能栏） | ⏳ | 本次迭代目标 |
| 汉字字体 | ⏳ | 本次迭代目标（`ab_glyph` + 系统字体回退） |
| 地图切换 / 小地图 / 音效 / 天气 | ⏳ | |

## 四、与 Crystal 的已知偏差

逐条记录在各自文档里，汇总入口：

- [`docs/world/movement.md`](world/movement.md) — 移动与动作计时
- [`docs/world/combat.md`](world/combat.md) — 战斗公式、RNG 顺序、18 条 Crystal 异常
- [`docs/world/items.md`](world/items.md) — 背包/装备/掉落/使用物品/丢金币/持久化差异
- [`docs/world/buffs.md`](world/buffs.md) — 增益容器、到期语义、9 个增益技能与 5 条异常
- [`docs/world/npc.md`](world/npc.md) — 脚本支持矩阵与 12 条异常
- [`docs/world/magic.md`](world/magic.md) — 技能支持矩阵与 11 条异常
- [`docs/client.md`](client.md) — 客户端协议流程与 7 条怪癖
- [`docs/store/mirdb-format.md`](store/mirdb-format.md) — 数据库格式与版本门
- [`docs/store/drops.md`](store/drops.md) — 掉落表语义

## 五、下一步优先级

1. **客户端 UI**：状态栏、聊天、背包、商店、技能栏与汉字字体。
2. **任务系统**：`Envir/Quests` 解析 + `QuestInfo` 状态机 + 任务奖励。
4. **组队 / 行会**：协议与数据结构已就绪，主要工作是状态机与广播。
5. **地图切换与传送**：`MovementInfo` + NPC 脚本的 `MAPMOVE`。
6. **视野与 AOI**：把广播半径与 Crystal 对齐，并做网格化索引。
