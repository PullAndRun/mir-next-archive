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
| 视野与广播 | 🟡 | 玩家/怪物/物品/NPC 的进出同步；半径已对齐 Crystal 16 格，接入 8×8 桶 AOI 网格、进出视野双向差分与门状态机（5 秒自动关），端到端用例见 `tests/aoi.rs`；待刑部全量验收 |
| 怪物 / 刷新点 | ✅ | 从 `Server.MirDB` 读 555 个模板，比奇省 359 组刷出 1922 只；死亡后按 `delay+random` 补刷 |
| 战斗 | ✅ | 两段式攻击、护甲/闪避/暴击、RNG 顺序照抄；PK 未做 |
| 经验 / 升级 | ✅ | `ExpList.ini` 真表、高等级衰减、属性重算、包与 Crystal 一致 |
| 死亡 / 复活 / 回血 | ✅ | 10 秒原地复活（Crystal 走客户端 `C.TownRevive`，本作先自动）、10 秒 3% 回血 |
| 物品 | ✅ | 掉落表、地面物品、拾取/丢弃/丢金币、46 格背包、14 装备槽、装备加成、消耗品使用、存档；个人仓库见「玩家仓库与邮件」 |
| NPC | ✅ | 660 份脚本解析、对话条件/动作子集、买卖修（定价公式与价格倍率）、`NewMapInfo` 下发；NPC 营业时段可见性未做 |
| 技能 | ✅ | 用书学习、施法校验、15 个伤害/治疗/位移/召唤/中毒技能 + 9 个增益技能（火墙/雷电/瞬息移动/隐身/召唤骷髅/召唤神兽/施毒；护盾/加速/减伤…）；第 4 轮补了野生怪索敌/反击召唤物（守卫/箭塔例外同 Crystal）；护身符消耗、开关型技能与部分地图级技能未做（见 `docs/world/magic.md`） |
| Buff | ✅ | 带到期时间的属性修饰层、`S.AddBuff`/`S.RemoveBuff`、药水增益（见 `docs/world/buffs.md`） |
| 任务 | ✅ | `Envir/Quests` 13 段脚本解析（真实 154/154 份）、接取前置、击杀/收集/标记目标、交付领奖、限时超时、放弃、NPC `[Quests]` 关联与三个任务动作、`characters.quests` 存档；第 4 轮补任务物品栏（40 格、205/206 包、掉落截胡、迁移 0012）；每日重置未做（见 `docs/world/quests.md`） |
| 组队 | ✅ | 邀请/接受/拒绝/踢人/退队、`!!` 组队聊天、同图同范围经验共享与任务杀怪共享、入队血条与切图地图名同步（见 `docs/world/group.md`） |
| 行会 | 🟡 | 创建/邀请/接受、职位（升/降/踢/职位名/权限）、公告、`!~` 行会聊天、行会经验与升级、行会仓库（物品 + 金币）、行会 buff、退出/转让/解散、五张表持久化（见 `docs/world/guild.md`）；攻城建筑、联盟未做 |
| 玩家仓库 | ✅ | `[@STORAGE]` 页面、`C.StoreItem`/`C.TakeBackItem`、80 格、`S.UserStorage`/`S.NPCStorage`、`characters.storage` 持久化；扩充仓库/仓库密码未做，个人仓库**没有金币功能**（见 `docs/world/storage.md`） |
| 邮件 | ✅ | 发送（金币/物品附件、邮票、邮费、10 秒限速）、阅读、邮局受理、领取附件、锁定/删除、`mail` 表持久化、30 天过期（扩展）、系统邮件（交易退货/市场退款）与商城邮件（背包溢出）；行会邮件、物品找回未做（见 `docs/world/mail.md`） |
| 玩家交易 | ✅ | 面朝格发起、接受/拒绝、10 格交易栏、金币、双方确认、取消与动作中止、背包满寄邮件；28 个端到端用例（见 `docs/world/trade.md`） |
| 寄售 / 拍卖 | ✅ | NPC 托管市场：寄售一口价、拍卖出价、搜索/翻页、提前成交、下架/收款、5% 佣金与 7 天过期、**到期自动成交**（`process_auctions` 每 60s，买家物品邮件 + 卖家金币邮件，同事务/幂等/崩溃重结算）、启动清理未成交过期单、迁移 0009 持久化（见 `docs/world/market.md`） |
| 商城（GameShop）与元宝 | 🟡 | 市场面板商城页签、`C.GameshopBuy` 真实扣元宝/金币、职业等级与库存校验、背包优先/放不下转商城邮件、迁移 0011 库存计数单事务持久化；元宝为**角色级**（Crystal 是账号级）；独立商城面板与 `S.GameShopInfo`/`S.GameShopStock` 未做（见 `docs/world/gameshop.md`） |
| PK 与攻击模式 | ✅ | Peace/Group/Guild/RedBrown/All、安全区禁打、PK 值与红/黄/棕名、死亡掉落、守卫（见 `docs/world/pk.md`） |
| 攻城（沙巴克） | 🟡 | 申请/时间表/四种城主判定/夺城/税收/传送点/箭塔、迁移 0008；城门/城墙/旗子实体与器械未做（见 `docs/world/conquest.md`） |

## 三、客户端

| 能力 | 状态 | 说明 |
| --- | --- | --- |
| 图库解码 | ✅ | `.Lib`（4 种变体）与 `.wzl+.wzx`；调色板/565/RGB24/DXT1/gzip RGBA |
| 等距地图渲染 | ✅ | Crystal 几何常量与图层映射，真实 `0.map` 出图 |
| 联机 | ✅ | 登录/建角/进世界、30+ 包快照、KeepAlive |
| 实体渲染 | ✅ | 玩家/怪物/NPC/物品精灵（`hum`/`monN`/`npc`/`items` 映射表） |
| 操作 | ✅ | 走/跑/转向/攻击/拾取/聊天 |
| UI（状态栏/聊天/背包/商店/技能/任务/任务物品栏/邮件/仓库/行会/组队/交易/市场/商城页签/设置面板/服务器选择） | ✅ | 主要面板已可用；第 2 轮补了设置面板（音效/音乐/语言/分辨率/按键说明）、登录窗口与服务器/角色选择、市场商城页签与技能表现（火墙/雷电/瞬息移动/隐身/召唤）；第 4 轮补任务物品栏（40 格图标 + 数量）与 HUD 天气；细节缺口见 [`docs/client.md`](client.md) §8 |
| 汉字字体 | ✅ | `ab_glyph` + 系统 TrueType 字体回退；找不到字体时退回点阵（中文会显示为空白并打印警告） |
| 地图切换 / 小地图 / 音效 / 背景音乐 | ✅ | 小地图/大地图/任务/邮件/仓库/行会/交易面板、rodio 音效与背景音乐 |
| 移动插值 / 寻路 / 天气粒子 / 光照 | ⏳ | 格间平滑、大地图点击寻路、天气粒子特效、光照待做；天气类型已随 `S.MapChanged.weather` 显示在 HUD，时间因服务端未下发 `S.TimeOfDay` 不显示 |

## 四、与 Crystal 的已知偏差

逐条记录在各自文档里，汇总入口：

- [`docs/world/movement.md`](world/movement.md) — 移动与动作计时
- [`docs/world/combat.md`](world/combat.md) — 战斗公式、RNG 顺序、18 条 Crystal 异常
- [`docs/world/items.md`](world/items.md) — 背包/装备/掉落/使用物品/丢金币/持久化差异
- [`docs/world/buffs.md`](world/buffs.md) — 增益容器、到期语义、9 个增益技能与 5 条异常
- [`docs/world/npc.md`](world/npc.md) — 脚本支持矩阵与 12 条异常
- [`docs/world/quests.md`](world/quests.md) — 任务脚本格式、接取/交付流程与 17 条 Crystal 异常
- [`docs/world/group.md`](world/group.md) — 组队状态机、包序、经验共享公式与 Crystal 异常
- [`docs/world/guild.md`](world/guild.md) — 行会状态机、职位权限、经验表、持久化与 Crystal 异常
- [`docs/world/storage.md`](world/storage.md) — 个人仓库存取、容量、包序与 Crystal 异常
- [`docs/world/mail.md`](world/mail.md) — 邮件发送/领取/过期、邮费公式与 Crystal 异常
- [`docs/world/maps.md`](world/maps.md) — 多地图、传送点与切图包序
- [`docs/world/aoi.md`](world/aoi.md) — AOI 网格、视野 16、进出视野差分与门状态机
- [`docs/world/trade.md`](world/trade.md) — 玩家交易状态机、包序与 Crystal 异常
- [`docs/world/market.md`](world/market.md) — 寄售/拍卖流程、佣金与手续费、过期与持久化
- [`docs/world/gameshop.md`](world/gameshop.md) — 商城与元宝：商品数据、购买校验、发货/邮件、库存计数与页签包序
- [`docs/world/magic.md`](world/magic.md) — 技能支持矩阵、15 个伤害/位移/召唤技能与 20 条异常
- [`docs/client.md`](client.md) — 客户端协议流程与 7 条怪癖
- [`docs/store/mirdb-format.md`](store/mirdb-format.md) — 数据库格式与版本门
- [`docs/store/drops.md`](store/drops.md) — 掉落表语义

## 五、下一步优先级

1. **i18n 收口（照单执行）**：`docs/i18n.md` §7 已给出逐条清单（第 1 类玩家可见 170+ 处，
   含 `文件:行`、`TextKey` 建议名与参数占位）。先做 §7.5 的参数化 `TextKey` API，
   再按表迁移；`tools/i18n/scan_hardcoded.py` 可复跑核对。
2. **商城收尾**：独立商城面板（`S.GameShopInfo`/`S.GameShopStock`）与客户端 `Class`/`Category`
   过滤、商城邮件附件领取的全链路端到端用例（见 `docs/world/gameshop.md` §7.3、§10）。
3. **技能补全**：`Blink`/`ShoulderDash`/`StormEscape` 等位移、`PoisonCloud`/`FrostCrunch`
   持续伤害、`ThunderStorm`/`TrapHexagon` 地图级技能、护身符消耗系统、`PetSave`/`PetExp`
   （见 `docs/world/magic.md` §11）。
4. **AOI 验收收尾**：8×8 桶网格、视野 16、进出视野差分与门状态机已实现，补跑
   `cargo test -p mir-server --test aoi` 与既有包序回归后翻 ✅。
5. **市场收尾**：✅ 已完成——拍卖到期自动成交（与 Crystal `ProcessAuction` 对齐，见 `docs/world/market.md` §9）。
6. **攻城与领地**：城门/城墙/旗子/器械实体、领地租借与联盟。
7. **客户端表现**：格间移动插值、大地图点击寻路、天气/光照、坐骑/翅膀精确素材映射、
   小地图 NPC 任务标记。
8. **任务补全**：✅ 任务物品栏已完成；剩余每日任务重置、`CallDefaultNPC`。
9. **行会补全**：新建职位/权限编辑/邀请入会界面、行会投票、联盟。
