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
| 视野与广播 | 🟡 | 玩家/怪物/物品/NPC 的进出同步；半径已对齐 Crystal 16 格，接入 8×8 桶 AOI 网格、进出视野双向差分与门状态机（5 秒自动关），端到端用例见 `tests/aoi.rs`；**R54 五路由已补证：4 PASS / 1 PARTIAL，仍不可全量关闭**（回收路由 NPC 子项无生产回收入口；全量门禁未跑），详见 [R54 证据索引](testing/round54/README.md) |
| 怪物 / 刷新点 | ✅ | 从 `Server.MirDB` 读 555 个模板，比奇省 359 组刷出 1922 只；死亡后按 `delay+random` 补刷；第 7 轮补远程攻击（弓手/守卫/箭塔）与目标丢失回位；第 8 轮补怪物魔法（AI 55 + 预留 Sep 215/216/220）与物理 AOE/直线（AI 13/29/14/50/100）；第 15 轮补 TrapRock 子石（AI 47）、场景 AI 3 Tree / 56 Trainer 与 `ai_audit` 巡检（已实现 480/537 模板 = 89.4%），见 [`docs/world/monster-ai.md`](world/monster-ai.md) |
| 战斗 | ✅ | 两段式攻击、护甲/闪避/暴击、RNG 顺序照抄；PK 未做 |
| 经验 / 升级 | ✅ | `ExpList.ini` 真表、高等级衰减、属性重算、包与 Crystal 一致 |
| 死亡 / 复活 / 回血 | ✅ | 10 秒原地复活（Crystal 走客户端 `C.TownRevive`，本作先自动）、10 秒 3% 回血 |
| 物品 | ✅ | 掉落表、地面物品、拾取/丢弃/丢金币、46 格背包、14 装备槽、装备加成、消耗品使用、存档；第 9 轮补新角色出生装（`StartItem` 职业过滤）、书店/技能书与新手提示；个人仓库见「玩家仓库与邮件」 |
| NPC | ✅ | 660 份脚本解析、对话条件/动作子集、买卖修（定价公式与价格倍率）、**耐久损耗/修理**（攻击 1..=4、受击每槽 -1、归零失效 + 提示）、`NewMapInfo` 下发；NPC 营业时段可见性未做 |
| 技能 | ✅ | 用书学习、施法校验、15 个伤害/治疗/位移/召唤/中毒技能 + 9 个增益技能（火墙/雷电/瞬息移动/隐身/召唤骷髅/召唤神兽/施毒；护盾/加速/减伤…）；第 4 轮补野生怪索敌/反击召唤物；第 5 轮补宠物命令 `C.ChangePMode`；第 9 轮补技能成长（`level_magic`）、**升级书**（已会未满 3 级）与**新手免费第一本书**；护身符消耗、开关型技能与部分地图级技能未做（见 `docs/world/magic.md`） |
| Buff | ✅ | 带到期时间的属性修饰层、`S.AddBuff`/`S.RemoveBuff`、药水增益（见 `docs/world/buffs.md`） |
| 任务 | ✅ | `Envir/Quests` 13 段脚本解析（真实 154/154 份）、接取前置、击杀/收集/标记目标、交付领奖、限时超时、放弃、NPC `[Quests]` 关联与三个任务动作、`characters.quests` 存档；第 4 轮补任务物品栏（40 格、205/206 包、掉落截胡、迁移 0012）；第 5 轮补 YouFound（205 → `quest_item_found` → `S.ChangeQuest`）、`Q` 标记掉落（先截胡、无人需要不落地）、每日重置（迁移 0013、按角色 24h 惰性）（见 `docs/world/quests.md`） |
| GM 命令 | ✅ | 21 条：12 条基础 + `@mob/@clearitem/@revive/@setpk/@warp/@mapinfo/@clearbag/@reboot` + `@learn`；权限 admin 或 `MIR2_GM_ACCOUNTS`，英文回复 + 审计（见 `docs/world/gm.md`） |
| 抗刷与连接限制 | ✅ | 单 IP 并发 5、连接速率 10/秒、空闲超时 60s、单会话包速率 200/秒（`MIR2_MAX_CONN_PER_IP` 等，`0`=关闭）；`@reboot` 倒计时优雅停机（见 [`docs/server/limits.md`](server/limits.md)） |
| 可观测性 | ✅ | `/metrics`（`MIR2_METRICS_LISTEN`）：在线/连接/限流拒绝/丢包/tick 分位数/拍卖扫描/邮件队列/DB 耗时、autosave/失败计数 + 30 秒汇总日志（见 [`docs/server/metrics.md`](server/metrics.md)） |
| 数据持久化 | ✅ | 迁移 0001–0016；角色周期 autosave（默认 30s、只写脏行，RPO ≤31s；`MIR2_AUTOSAVE_SECS=0` 退回下线存盘）、行会/攻城 100ms、失败退避 1s→30s 与批次回灌；第 16 轮补**平滑重启**（60/30/10s 广播、强制存盘、90s 重连宽限，见 [`docs/world/persistence.md`](world/persistence.md) §7） |
| 组队 | ✅ | 邀请/接受/拒绝/踢人/退队、`!!` 组队聊天、同图同范围经验共享与任务杀怪共享、入队血条与切图地图名同步（见 `docs/world/group.md`） |
| 行会 | 🟡 | 创建/邀请/接受、职位（升/降/踢/职位名/权限）、公告、`!~` 行会聊天、行会经验与升级、行会仓库（物品 + 金币）、行会 buff、退出/转让/解散、五张表持久化、**行会联盟**（`/ally`、上限 5、盟友 PK 豁免、迁移 0015）+ **联盟页签 UI**（`S.GuildAllies`(303) 权威名单）；行会敌对/行会战、攻城建筑未做（见 `docs/world/guild.md`） |
| 好友与黑名单 | ✅ | 加/删/拉黑/备注、`S.FriendUpdate`(245)、上限 100、`characters.friends` 持久化（迁移 0015）、**F 键好友面板**（320×240 适配）；只支持在线角色（见 [`docs/world/social.md`](world/social.md)） |
| 玩家仓库 | ✅ | `[@STORAGE]` 页面、`C.StoreItem`/`C.TakeBackItem`、80 格、`S.UserStorage`/`S.NPCStorage`、`characters.storage` 持久化；扩充仓库/仓库密码未做，个人仓库**没有金币功能**（见 `docs/world/storage.md`） |
| 邮件 | ✅ | 发送（金币/物品附件、邮票、邮费、10 秒限速）、阅读、邮局受理、领取附件、锁定/删除、`mail` 表持久化、30 天过期（扩展）、系统邮件（交易退货/市场退款）与商城邮件（背包溢出）；行会邮件、物品找回未做（见 `docs/world/mail.md`） |
| 玩家交易 | ✅ | 面朝格发起、接受/拒绝、10 格交易栏、金币、双方确认、取消与动作中止、背包满寄邮件；28 个端到端用例（见 `docs/world/trade.md`） |
| 寄售 / 拍卖 | ✅ | NPC 托管市场：寄售一口价、拍卖出价、搜索/翻页、提前成交、下架/收款、5% 佣金与 **48 小时过期**（Crystal 7 天，本作差异）、**批量上架/取回 ≤10 全成全回滚**、**到期自动成交**（`process_auctions` 每 60s，买家物品邮件 + 卖家金币邮件，同事务/幂等/崩溃重结算）、启动清理未成交过期单、迁移 0009 持久化（见 `docs/world/market.md`） |
| 商城（GameShop）与元宝 | 🟡 已交付，有残余 | R50 独立商城面板、`S.GameShopInfo`(250)/`S.GameShopStock`(251) 真实目录/库存包链、客户端Class/Category筛选、K热键、素材manifest、库存/失败事务均已交付；市场页签、元宝/金币交易与迁移0011沿用，元宝仍为角色级（Crystal账号级）。服务端Category过滤、K改键与像素/release真机仍PARTIAL，见[商城 §12](world/gameshop.md)及[客户端 §5.16.1](client.md)。 |
| PK 与攻击模式 | ✅ | Peace/Group/Guild/RedBrown/All、安全区禁打、PK 值与红/黄/棕名、死亡掉落、守卫（见 `docs/world/pk.md`） |
| 攻城（沙巴克） | 🟡 | 申请/时间表/四种城主判定/夺城/税收/传送点/箭塔、迁移 0008；第 8 轮补器械简化实体（Gate/Wall/Archer + 300–302 扩展包 + **迁移 0016** + `OPENGATE`/`CLOSEGATE` + 箭塔 `ObjectRangeAttack`）；旗子/控制点 NPC、修理未做（见 `docs/world/conquest.md` §8.6） |

## 三、客户端

| 能力 | 状态 | 说明 |
| --- | --- | --- |
| 图库解码 | ✅ | `.Lib`（4 种变体）、`.wzl+.wzx` 与第 6 轮新增 `.wil+.wix`（WEMADE `#ILIB`/`#INDX`）；调色板/565/RGB24/DXT1/gzip RGBA |
| 等距地图渲染 | ✅ | Crystal 几何常量与图层映射，真实 `0.map` 出图；视口内瓦片解码 + 4096 张缓存，release 800×600 ≈0.50 ms/帧、1080p ≈1.45 ms/帧；100 玩家 720p ≈1.42 ms（t104 帧合并 + 特效池化） |
| 真实素材渲染 | ✅ | `resource/data`（`--data-dir` 可指定）；28 个 wzl + 5 个可打开 WIL；玩家/怪物/NPC/物品按真实图库与帧渲染，缺资源占位回退 |
| 联机 | ✅ | 登录/建角/进世界、30+ 包快照、KeepAlive |
| 实体渲染 | ✅ | 玩家/怪物/NPC/物品精灵（`hum`/`monN`/`npc`/`items` 映射表） |
| 操作 | ✅ | 走/跑/转向/攻击/拾取/聊天 |
| UI（状态栏/聊天/背包/商店/技能/任务/任务物品栏/邮件/仓库/行会/组队/交易/市场/商城页签/设置面板/服务器选择） | ✅ | 主要面板已可用；第 2 轮补了设置面板（音效/音乐/语言/分辨率/按键说明）、登录窗口与服务器/角色选择、市场商城页签与技能表现（火墙/雷电/瞬息移动/隐身/召唤）；第 4 轮补任务物品栏（40 格图标 + 数量）与 HUD 天气；第 5 轮补宠物模式 V 键、任务物品滚轮翻页、`@` 命令原样发送与 `S.SendOutputMessage` 三频道；第 7 轮补攻击 300ms 节奏、挥砍/技能命中特效、音效 100ms 节流与 BGM 按地图切换；第 8 轮补好友面板（F）与行会联盟页签、**攻城 UI**（300–302 器械/HUD/结算横幅）；第 9 轮补组队面板（P，成员/HP/职业/地图）与行会成员页（四列 + 刷新）；第 13 轮补聊天折行/避头尾/省略号与字符级 IME 光标；第 15 轮补小地图传送点标记/悬停提示与断线重连（3 次退避 + 横幅）；第 16 轮补设置面板 O（主/音乐/音效滑杆、窗口模式、6 键重绑、`client.toml`）；细节缺口见 [`docs/client.md`](client.md) §8 |
| 汉字字体 | ✅ | `ab_glyph` + 系统 TrueType 字体回退；找不到字体时退回点阵（中文会显示为空白并打印警告） |
| 地图切换 / 小地图 / 音效 / 背景音乐 | ✅ | 小地图/大地图/任务/邮件/仓库/行会/交易面板、rodio 音效与背景音乐 |
| 移动插值 | ✅ | 第 45 轮自机走/跑 6×100ms 整数像素插值 + 相机跟随（照 Crystal `OffSetMove`）；第 46 轮他者玩家、第 49 C 怪物走/跑已按 Crystal 插值（怪物真实证据限 `005/022/184`，见 [`docs/testing/monster-motion/README.md`](testing/monster-motion/README.md)）；四类真实窗口边界未实测、≥600ms/跳变无窗口录屏；坐骑帧表、Push/Jump/Dash 未接（见 [`docs/testing/self-motion/README.md`](testing/self-motion/README.md)） |
| 寻路 / 天气粒子 / 光照 | ⏳ | 大地图点击寻路、天气粒子特效、光照待做；天气类型已随 `S.MapChanged.weather` 显示在 HUD，时间因服务端未下发 `S.TimeOfDay` 不显示 |

## 四、与 Crystal 的已知偏差

逐条记录在各自文档里，汇总入口：

- [`docs/world/movement.md`](world/movement.md) — 移动与动作计时
- [`docs/world/combat.md`](world/combat.md) — 战斗公式、RNG 顺序、18 条 Crystal 异常
- [`docs/world/monster-ai.md`](world/monster-ai.md) — 怪物远程攻击/回位、掉落组概率语义
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
- [`docs/world/gm.md`](world/gm.md) — GM 命令：权限、21 条命令、错误口径与审计
- [`docs/server/limits.md`](server/limits.md) — 抗刷与连接限制：单 IP 并发/连接速率、心跳超时、包速率
- [`docs/server/metrics.md`](server/metrics.md) — `/metrics` 字段、Prometheus 抓取与 30 秒汇总日志
- [`docs/world/social.md`](world/social.md) — 好友与黑名单：数据、包号、上限与持久化
- [`docs/world/persistence.md`](world/persistence.md) — 存档与崩溃一致性：autosave、RPO、失败退避与指标
- [`docs/testing/duo-smoke.md`](testing/duo-smoke.md) — 双人联机冒烟：场景矩阵与运行方式
- [`docs/testing/playtest.md`](testing/playtest.md) — 协议级 playtest：Python 直连真服务端与限制
- [`docs/testing/stress.md`](testing/stress.md) — 压测：100/300 连接基线、tick/CPU/RSS/DB 口径
- [`docs/testing/ci-assets.md`](testing/ci-assets.md) — CI 素材依赖审计：30 条用例清单与无 resource/ 复现
- [`docs/world/magic.md`](world/magic.md) — 技能支持矩阵、15 个伤害/位移/召唤技能与 20 条异常
- [`docs/client.md`](client.md) — 客户端协议流程与 7 条怪癖
- [`docs/store/mirdb-format.md`](store/mirdb-format.md) — 数据库格式与版本门
- [`docs/store/drops.md`](store/drops.md) — 掉落表语义

## 五、下一步优先级（R52 A，2026-10-25）

R50 独立商城已交付；R51 已 **Done / PASS-WITH-PARTIAL**，已交付 Blink、StormEscape、FrostCrunch、ThunderStorm、ShoulderDash 五技能。R52 B1–B4/C 已交付护身符耗材原语、PoisonCloud、TrapHexagon、R51 定向回归与客户端 wire，当前交付仍含 **PARTIAL**，T 的 G7 **BLOCK** 待 H 收口复核；125 无服务端生产者、真 socket、原生 dash 表现、Frost 玩家目标、guild/嵌套槽与旧技能耗材等边界不得写成完整闭环。R51 [终审 v4 §D](testing/round51/round51-final-audit-v4.md)的十一项仍按下表承接；P03/P10 closed 是历史结论，不因补证计划重新打开或扩大。

1. **R52 主线（已交付，含 PARTIAL）**：B1 护身符耗材原语、B2 PoisonCloud、B3 TrapHexagon、B4 R51 定向补证与 C 客户端真实 wire 均已交付；PoisonCloud 使用 5 毒 + 5 符，Trap 使用 1 符，服务端/客户端证据与 attempt2 门禁 **12/12 PASS（exit 0）** 已归档。G1/G6 的已证子项不等于所有存档、125、socket 或表现边界闭合，详见[门下 D1–D5、M01–M08](process/round52-menxia-review.md)和[派发契约](process/round52-shangshu-dispatch.md)。
2. **R51 定向补证（B4 已交付，含 PARTIAL）**：已从默认数据加载链记录七技能资料（默认数据共 107 条）及 MP/冷却边界，并完成 Slow/Frozen、StormEscape、来源/RNG 等本轮回归；Frost 玩家目标、guild/嵌套槽及其它未覆盖边界仍为 PARTIAL，现有分项绿灯不替代未覆盖的真实 socket 或原生表现证据。
3. **文档与 i18n（G7）**：保留 R49/R50/R51 原日志和历史结论；本轮收口同步当前 TextKey 与扫描计数，实测值见 [i18n §7.9](i18n.md)。R50 参数化 TextKey 与 no-test 收口已交付，不再按待实施派活。
4. **商城已交付 / 残余顺延**：R50 的独立面板、250/251 真实包链、库存/失败事务及素材 manifest 见 [商城 §12](world/gameshop.md#12-r50-交付索引与残余原r50-前置条件已完成保留结论)。服务端 Category 过滤、K 改键、像素基线与 release 真机往返仍按 [客户端 §5.16.1/§8](client.md)保留，不扩入 R52。
5. **其它独立系统顺延**：PetSave/PetExp、AOI 全量验收、攻城旗子/控制点/修门/领地、坐骑/天气/光照/攻击变招、大地图寻路、CallDefaultNPC、行会职位/投票 UI。市场拍卖到期自动成交、任务物品栏/YouFound/Q 标记/每日重置、行会联盟页签与 303 权威名单已交付。

| R51 §D 项 | R52 处置（已交付项与顺延边界） |
|---|---|
| 1 两技能/耗材；Frost 玩家 PvP | B1/B2/B3/C 已交付 G1–G3/G6 子项；Frost 玩家真实目标仍 PARTIAL，旧技能耗材顺延 |
| 2 默认逐技能 MP/冷却 | B4/G4 已交付；默认记录及来源 SHA、ShoulderDash 2500ms、PoisonCloud L2/L3 边界已记录；未覆盖边界点名 PARTIAL |
| 3 非空 Slow、概率两侧、玩家/怪物 | B4/G5 已交付定向回归；玩家 Frost 真实施法仍 PARTIAL |
| 4 StormEscape 包序/跨图 | B4/G5 已交付；真实升级、五位置包序与跨图结果已记录 |
| 5 玩家 Frozen 实际拒攻 | B4/G5 已交付拒攻证据；玩家真实 Frost 命中仍未冒称覆盖 |
| 6 152/153 敏感回纠 | C/G6 已交付；错误预测格 + 已有 motion/队列两分支均有证据 |
| 7 火墙逐对象 ProcessSpell/InTrapRock | 顺延 R53 候选，P04 保留；TrapHexagon 不等于 TrapRock |
| 8 原生 dash 帧/视觉/声音/时序 | 顺延，P08 保留；Walking/Walk 借帧仍为 PARTIAL |
| 9 guild Freezing / 真 socket | 顺延；World→encode→decode→快照不等于 socket 端到端 |
| 10 多根槽有效装备 / 1965–1966 插槽 | 多根槽由 B4/G5 补证；嵌套插槽属性顺延 PARTIAL |
| 11 零 RNG/坏装备+buff/0 耐久+added | B4/G5 已交付状态序列对照，Freezing7 上界收紧至 11；未覆盖边界仍 PARTIAL |

实施顺序为 A → B1 → B2 → B3 → B4 → C → Lead 唯一全量门禁 → T → 尚书 → 指定门下确认；各批串行独立评审且仅 Lead 提交。G1–G3/G6 已有交付子项但仍保留 125、socket、存档及表现边界，G4/G5 逐子项判定；D3 离线/异图安全清场是已批准的生命周期偏差，须单列 PARTIAL。T 当前 BLOCK 仅指 G7 文档收口，H 收口证据见 [R52 索引](testing/round52/README.md)。

## 六、第 49 轮收口状态（2026-10-25）

本轮只做“有真实证据的收口”，不把采样结论外推成全量；gate=12/12 PASS；日志 target/round49/final-gate.log，182,941 B，sha256 5d475f4f…，exit 0，commit d5707c6；R49 verdict=PASS-WITH-PARTIAL（P01–P08 未清）。

| 项 | 状态 | 说明 |
|---|---|---|
| 怪物原生帧表（R49 B） | 🟡 已收口（限 118 个 used 怪物库、目标客户端路径） | `.Lib` v3 的 `frames.json` 接入默认 `game-assets`；4 个 v2、约 800 个源缺 `.Lib` 的 appearance 未导入；逐帧真实渲染只覆盖 `005/022/039/023`；Blend（117）/Reverse/Effect/Sorting 未移植。证据 [`docs/testing/native-monster/README.md`](testing/native-monster/README.md) |
| 怪物走/跑插值（R49 C） | 🟡 已收口（限 `005/022/184` 真实证据） | 照 Crystal `MonsterObject.cs:345-433`；坐骑帧表、Push/Jump/Dash 未接。证据 [`docs/testing/monster-motion/README.md`](testing/monster-motion/README.md) |
| 文档与 i18n 收口（R49-05） | 🟡 PARTIAL | i18n CI 口径 66（≤70 通过）、文档口径 15（未达 0）；链接检查 `[OK] 670`（最终复跑）；`git diff --check` 0；证据 [`docs/testing/round49/README.md`](testing/round49/README.md) |
| 独立商城面板 | ⏳ 未完成，移至 R50 | 不把市场页签当独立面板；前置条件见 [`docs/world/gameshop.md`](world/gameshop.md) §12 |

> 第 49 轮冻结的 WIP 状态（33 条 tracked/untracked 差异、**109 个 WIP 文件**（19 tracked + 90
> untracked 展开）、4 个证据目录、两份 44/45 MB 大 JSON 只登记不入库）见
> [`docs/testing/round49/wip-freeze.json`](testing/round49/wip-freeze.json)；刑部审计
> 结论与 BLOCK 项见 [`docs/testing/round49/round49-xingbu-wip-audit.md`](testing/round49/round49-xingbu-wip-audit.md)。

> 第 55 轮（R55）maps 基线脚注：`cargo test -p mir-server --test maps` 共 **20 例**，其中 **16 例有效**、**4 例需 `MIR2_DEV_DB_PATH`**（`real_database_*`，缺省路径早退 skip，不计入有效数）；**口径：条件 PASS（需 `MIR2_DEV_DB_PATH`）**；该脚注只登记现状，判定以刑部 T 签发为准。

---

## 七、第 57 轮（R57）状态（在途）

| 项 | 状态 | 说明 |
|---|---|---|
| FireBounce 服务端生产者 | 代码批在途 | G1/G2 由 A 批实现，证据 `target/round57/firebounce/attempt-N/`；未过门禁前不作判定 |
| 文档承接（R56 遗留） | 已落盘，待 T 抽核 | 支持矩阵、aoi 接线时点、R56 证据路径勘误、R52 行口径、R56 受测 HEAD 双标注；见 [R57 索引](testing/round57/README.md) |
| 客户端 125 渲染 | PARTIAL | 服务端生产者不等于客户端闭环；R59 起客户端为**最小闭环 + PARTIAL**（`magic.wzl` 410–565 飞行 + 570–579 命中 + `M34-1/-2`；无落点即不生成 D-R59-1），见 §八，不得表述为闭环 |

---

## 八、第 59 轮（R59）状态（已落盘待 T/门禁抽核）

本轮只做客户端 125（`S.ObjectProjectile` / `FireBounce`）**表现链最小闭环 + PARTIAL**；不建 `Missile`/`Effect`
类体系，只按最小闭环 + PARTIAL 表述，也不预写门禁/终审结论。D2–D4 已落盘（代码、16+6 测试、`run-checks.sh`
9 项）；**门禁与 T 终审未跑，不写整体通过结论**；音效真实可听仍 PARTIAL（素材已导入、通道未接）。

| 项 | 状态 | 说明 |
|---|---|---|
| 客户端 125 投射物飞行（G1/G2） | 🟡 已落盘待 T 抽核 | `crates/mir-client/src/ui/projectile.rs`（游标消费 + `duration/count/index/offset/dir16` 按 Crystal 公式）；接线 `ui/mod.rs:1562-1564/4278-4279`；`round59_projectile_render` 16 passed / exit 0；证据 `target/round59/d1/attempt-1/`、`target/round59/d4/attempt-1/` |
| 命中 570 与音效（G3/G4） | 🟡 已落盘待 T 抽核（音效可听 PARTIAL） | 目标存在且未 `dead` → `magic.wzl[570..=579]`×60ms=600ms（`projectile.rs:79-85/505-537`）；音效基准 `GreatFireBall=34` → `M34-1.wav`/`M34-2.wav`（`sound.rs:234-281`，`mir-render sound` 单测 9 passed）；`round59_projectile_render` 16 passed（含已死目标不播命中/`+2`、每投射物各一次）；素材已导入（`audio verify files=106`），但 `take_sound_cues()` 未接 `app.rs` → **真实可听 PARTIAL**；证据 `target/round59/d2/attempt-1/`、`target/round59/d4/attempt-1/` |
| 素材包扩展（G5） | 🟡 已落盘待 T 抽核 | `tools/assets/analyze_usage.py:324-327` 显式 106 帧段；`round59_projectile_assets` 6 passed / exit 0；本地 106 PNG、`missing=[]`、`skipped_empty=0`；全计划 `used_frames` 186,294→186,390；证据 `target/round59/d3/attempt-1/`、`target/round59/d4/attempt-1/` |
| 测试与门禁脚本（G1–G4 证据） | 🟡 已落盘待 T/门禁抽核 | `round59_projectile_render.rs`（621 行/16 测试）、`round59_projectile_assets.rs`（585 行/6 测试）、`docs/testing/round59/run-checks.sh`（420 行/9 项）；Lead 复跑 9/9 条 exit 0、总 EXIT=0（脚本自报，`target/round59/lead-verify/10-run-checks.log`）；**T/CI 门禁未跑** |
| 文档入树（G6） | 🟡 已落盘待 T 抽核 | [client.md](client.md) §5.26、[magic.md](world/magic.md) FireBounce 行、[R58 索引](testing/round58/README.md):3 HEAD 更正、[asset-pipeline.md](assets/asset-pipeline.md):112、[R59 索引](testing/round59/README.md)；D5b 已按 D2–D4 落盘事实回填 |

| R61 后续状态 | 🟡 已落盘待终审 | 125 append-only 双层消费、动态 duration/count、逐 cue 离线混音已由代码提交① `ef317b0` 落盘；长期回收、完整时钟、真实设备可听与 Live 仍 PARTIAL。见 [R61 索引](testing/round61/README.md)，不得预写 PASS。 |


事件无 ID；`duration/count` 生成时冻结（Crystal `Effect.cs:174-181` 每帧重算）；draw offset 不含 wzl 逐帧
offset；音效真实可听 PARTIAL（`M34-1/-2` 素材已由 `audio import` 产出、`audio verify files=106`，但
`take_sound_cues()` 未接 `app.rs`；`game-assets/` 不入库）；无 Hero、无真 socket。台账见
[R59 索引 §3](testing/round59/README.md)。

**R60 承接**：P-2「事件无 ID / 可能漏事件」已由 R60 修复（绝对序号游标，第 17+ 事件不再被吞；ring 16 仍丢最旧）；
P-5「音效通道未接」已由 R60 接通（`app.rs` 同帧 drain），可听性仍 PARTIAL；P-3/P-4 与生成入脚本转 R61。

## 九、第 60 轮（R60）状态（🟡 已落盘待终审；门禁 attempt-1 已跑）

本轮只做两件事：**125 音效真实通道**（G1）与 **投射物事件去重**（G2），外加文档/脚本/树哈希收口（G3）与唯一门禁（G4）。
口径：**通道打通 + 去重 + PARTIAL 诚实登记**；不写「125 音频完整」「无重复」「真实可听已证」。

- 代码提交① `bb53321`「feat(client): R60 125 音效真实通道与投射物事件去重」，4 文件 +593/−12，已 push `origin/main`。
- 唯一全量门禁 attempt-1（在 `bb53321` HEAD 上）：`pwsh -NoProfile -File tools/dev/ci.ps1 -KillStale` →
  **EXIT=0、12/12 全 OK（末行「全部通过（12 项）」）**；日志 `target/round60/gate/attempt-1.log`
  （205,317 B，sha256 `193b6fe4f8a0a46397a0026c2e8a2d66f0e041c9fa33f0ad6ca3a17d1f3678dd`，UTC 10:29:53–10:32:26Z）。

| 项 | 状态 | 说明 |
|---|---|---|
| G1 125 音效真实通道 | 🟡 已落盘（可听性 PARTIAL） | `ui/mod.rs:1275` `take_projectile_sound_cues` → `app.rs:2757` 同帧 drain → `app.rs:201` `play_projectile_cues` → `audio.rs:592` `play`；`--bin mir-client` 21 passed、`--lib audio` 19 passed；M34-1/2 真 `.mna` 解码成功（无 SKIP）。**只证路径，不证可听** |
| G2 125 事件去重（绝对序号游标） | 🟡 已落盘 | `net.rs:2772` `projectile_events_total` + `:7265` 自增；`ui/projectile.rs:461` `consume` 改绝对游标；**F6 更正**：真实缺陷＝第 17+ 事件永久丢失（非 ring 重放）；`ui::projectile` 12 passed、`net::` 30 passed、`round59_projectile_render` 16、`round58_projectile_wire` 7、`mir-render sound` 9 |
| G3 文档/脚本/树哈希 | 🟡 已落盘待终审 | [R60 索引](testing/round60/README.md)、[run-checks.sh](testing/round60/run-checks.sh)（11 项）、`commit-tree-hashes.txt`、[client.md](client.md) §5.27；R59 P-2 状态已回填 |
| G4 唯一门禁不回归 | ✅ attempt-1 exit 0 / 12 项 | 证据 `target/round60/gate/attempt-1.log`；**终审由 D4 复核，不在此预写结论** |

**已知 PARTIAL**：可听性（headless 无音频设备 + `apply_snapshot` 无 Live 窗口级集成测试）；
`from_repo_root` 找不到 M34 素材（既有根路径行为，未改代码）；ring 16 仍丢最旧（R4a 不修）；
同文件 100 ms 频控 → 同帧多 cue 只响一次（≠ Crystal 每发都响）。台账见

| R61 状态 | 🟡 已落盘待终审 | 代码提交① `ef317b0`、门禁 attempt-1 EXIT=0/12 项、D1/D2 定向证据已落盘；D3 文档/脚本证据待 D4、尚书与独立门下抽核。不得预写整体 PASS。 |

## 十、第 61 轮（R61）承接

R61 目标为 125 积压不丢、动态飞行参数与逐 cue 离线混音。已证范围是测试与离线 mixer 样本；append-only 无回收、完整 NextFrame/SkipFrames、TargetPoint、像素偏移增量、真实设备可听/Live、33 叠加削波、105 个 sfx 未逐一及 `HiddenSpanSource` 覆盖面仍是 PARTIAL。详见 [R61 索引](testing/round61/README.md)。

## 十一、第 62 轮（R62）完成：PASS-WITH-PARTIAL

本轮限定交付完成：严格 NextFrame 飞行时钟、目标缓存兜底、运动 offset 一次叠加、会话代号与恰一次复位。代码 `a7362fc`（6 文件 +1502/−248），Lead 门禁 attempt-1 EXIT=0、12/12 OK；C 独立核验 C1–C5 PASS，44 targets / 558 passed / 0 failed / 7 既有 ignored。文档登记六 SHA 与 R59/R61 改写测试 supersede，保留历史冻结证据。push 待 Lead 终审后统一处理。详见 [R62 索引](testing/round62/README.md)。此状态只指限定交付，不代替父任务独立终审。

## 十二、第 63 轮（R63）完成：PASS-WITH-PARTIAL（限定 G1+G2+首批缓存顺序）

R62 P3 四项候选：事件表物理回收无服务端协议；known_positions 无容量上限/LRU；owner_skip_frames 生产固定 false（Crystal PlayerObject.cs:750/MonsterObject.cs:346 ActionFeed 来源未接）；570 残影仍 elapsed 公式。R62 P1 ignored 已按 7 更正，P2 六 SHA/supersede 已登记待终审。

R61 12 项 PARTIAL 按原编号承接，已证子项仅作进展，不整项清零（来源：[R61 独立台账](testing/round61/round61-final-audit.md#L43)）：

| 原项 | R63 承接与 R62 进展 |
|---|---|
| P01 append-only 回收/clone/容量 | 消费游标与代号契约已有测试；服务端确认、安全裁剪、物理回收与长期容量仍待办 |
| P02 NextFrame/SkipFrames | 严格 NextFrame 已证；生产 SkipFrames/ActionFeed 来源继续 PARTIAL |
| P03 TargetPoint | 当前位置优先与最后可信缓存兜底已证；缓存容量/LRU 继续 PARTIAL |
| P04 像素偏移 | 运动 offset 一次叠加已证；ImageInfo.offset 不在本轮修复范围 |
| P05 设备/Live 可听 | 未新增真实声卡或 Live 窗口证据 |
| P06 33 路削波 | peak=22.844788 的真实输出削波风险仍在 |
| P07 105 sfx | 未逐一验证；既有样本集中 M34-1/2 |
| P08 HiddenSpanSource | 仅本 crate decoder_for 路径，未全局化 |
| P09 CWD/素材来源 | 根路径与素材解析边界继续保留 |
| P10 会话代号歧义 | 本地统一发号/首批非空恰复位已证；未来新代 len=0 且 total>0 边界与物理回收协议继续跟踪 |
| P11 缺素材 early-return | SKIP 仍按 PARTIAL，不能计 PASS；复查脚本对 SKIP 总体失败 |
| P12 R60 P-1..P-6 | 原台账不自动清零，不由局部时钟/队列测试推导渲染、效果或音频完整 |

本轮完成（限定交付）：G1 570 命中特效严格 NextFrame 时钟（对照 Crystal `Effect.cs:38-48,81-96`）；G2 `consume` 顺序 reset→cache→spawn，跨代首批 payload 位置先进缓存再 spawn。功能提交 `1fcf4fc`（5 文件 +874/−66），Lead 门禁 attempt-1 EXIT=0（12 项 OK / 0 FAIL，日志 `target/round63/gate/attempt-1.log`，sha256 `15adfa30…ac1656`）。新增 wire 测试 3 例，R59/R61/R62 的 elapsed 断言同步 supersede 为严格 NextFrame；红绿同一测试文件 SHA `6438cce…`（基线 `f80724e` exit 101：`round63_projectile_wire.rs:182` 571≠570、`:290` 1≠2），功能树 lib 24 / wire 3 / R59 16 / R61 11 / R62 5 与 clippy/fmt 全绿，C 独立复跑一致。PARTIAL 继续承接：生产 `SkipFrames`/ActionFeed 来源、命中后物理回收与事件表减容、`known_positions` 容量/LRU、570 素材（`game-assets/` 不入库）。刑部终审已完成（PASS-WITH-PARTIAL，报告 `docs/testing/round63/round63-final-audit.md`）；尚书验收进行中（S1 证据一致性已修正待复验）；门下确认尚未完成。本状态只指限定交付，不代替父任务独立结论；push 待 Lead 统一处理。详见 [R63 索引](testing/round63/README.md)。

## 十三、第 64 轮（R64）限定交付：门禁通过，终审未完成

功能提交 `063c6f6`（5 文件 +905/−34）已提交并推送：投射物事件表本地确认水位与安全前缀裁剪。`AckProjectileEvents` 是进程内契约，wire 零改动，仅 app.rs 单消费者；水位按已确认条数计，0-based 下标到 1-based 事件序号的换算见 [client §5.31](client.md#531-投射物事件表本地确认水位与安全前缀裁剪r64)。

Lead 唯一门禁 attempt-1 在 `063c6f609e2ee61fc4d3cd8c5d322dcc32c49ce2` 上 EXIT=0、12/12 OK；C 独立复跑绿。阶段：**门禁通过；独立复跑绿；终审待刑部/尚书/门下，未 PASS**。F1/F3 CLOSED-with-comment-only，F2 证据补录，F4 由文档统一；交付、哈希和复跑见 [R64 索引](testing/round64/README.md)。本阶段不替代父任务独立终审。

PARTIAL：多消费者/克隆快照全局最低水位未实现；事件表未限容、未确认 backlog 可增长；known_positions 容量/LRU 仅契约；服务端回收协议未做；定向绿不替代 Lead 全量门禁；R61/R62/R63 PARTIAL 按原编号承接，不整项清零。

## 十四、第 65 轮（R65）候选

1. 多消费者/克隆快照全局最低水位与消费生命周期契约。
2. 事件表限容策略与未确认积压处理；known_positions 容量/LRU 另取依据，不发明 Crystal 常数。
3. 服务端回收协议独立设计、评审与兼容验证。
4. 生产 SkipFrames/ActionFeed 来源、570/550 动作与特效、目标切换及飞行参数更新。
5. 掉落表 1..22 / 24-26 / 36-40、event_add_type 与护身符 4/13/14/17。
6. 继续承接设备/Live 可听、素材与 R61 P01–P12 历史边界；各轮终审由对应责任链收口。
