# 第 40 轮核心闭环独立审计

## 总结论与证据不足（置前）

**verdict: partial；核心闭环部分成立，不能宣称完整玩家闭环终验通过。**

审计日期：2026-10-22（Asia/Shanghai）；动态 live 日志观察截止 18:11。审计者：御史台。范围：已有日志、当前工作树源码、关键测试和 packs 冒烟；不改产品代码、不启动服务端、不运行 git。本文与 `target/audit-round40-*` 是本次审计产物。

最高优先缺口：

1. **high / PS-1 判据降级**：修后真服已有 [ps1-fix](../../target/ps1-fix.log#L12-L25) 的穿装后重登成功样本，不能再写“完全未复跑”。但当前 [比较器](../../tools/playtest/playtest.py#L4171-L4197) 合并装备/背包 UID，且耐久不同只打印、不失败；槽位错乱、同 UID 模板变化及耐久回滚可漏判。该样本实际打印的前后值相同，可以证明这一次两件装备保留，不能证明旧组合失败及完整逐字段门禁已经修复。
2. **high / 原组合仍为失败证据**：[core1c](../../target/core1c.log#L77-L91)、[core1d](../../target/core1d.log#L80-L94) 均有任务奖励物品消失及耐久差异，前者另有装备回背包；修后组合日志 [core1e](../../target/core1e.log) 截取时尚无终态。独立 PS-1 样本金币 0、已完成任务/技能为空，不能覆盖这些非空进度。后续追加日志不得倒改本次证据时间窗。
3. **high / DR-1、TP-1 与 EQ-1 仅分层部分覆盖**：DR-1 有 live 自然死亡与 60s 兜底复活，但没有客户端弹窗确认回城的 live 闭环；TP-1 只走第一个 movement（皇宫），未证明到达任务交付目标或客户端 UI 点击；EQ-1 已证实磨损与付费修理，未在该 live 日志证明自然耗尽至 0、属性失效、修后再次战斗。live 升级仍为 **SKIP**，不换算 PASS。

其他证据边界：

- 受“不跑 git”限制，使用 [HEAD 指针](../../.git/HEAD#L1) → [main ref](../../.git/refs/heads/main#L1) 读取 `633a1700567da61b1ede82b4454ebe149fd62f90`；[reflog](../../.git/logs/HEAD#L196) 的主题为“登录读角色加 per-character 写锁…”。没有运行 `git log -1`，没有证明工作树 clean。测试针对当前工作树，不等同于 clean commit 的构建证明。
- 老 live 日志没有内嵌可核验的构建哈希，不强行绑定到当前 HEAD。`ps1-fix` 使用 7123，旧 `core1d` 使用 7122，属于不同 run。
- 当前 launcher 日志已进入第 41 轮窗口，不能用“当前检索 0 行”倒证第 40 轮启动无旧资源回退，更不能证明素材版权自有。
- 本轮已见 12 项门禁终态日志，且独立复跑关键子集；不宣称独立复跑全套。门禁绿不等于全部测试无跳过，日志有 2 条 ignored。

## 证据方法与复跑命令

所有行号均来自逐行读取原文件；下列是本次实际命令，查询只读取本地文件。

```powershell
cargo test -p mir-server --lib -- --nocapture *> target/audit-round40-lib.log
cargo test -p mir-server --test npc_shop --test persistence_equipment -- --nocapture *> target/audit-round40-shop-persistence.log
pwsh -NoProfile -File tools/dev/release-smoke.ps1 *> target/audit-round40-release.log
cargo test -p mir-server --lib level_up *> target/audit-round40-level.log
cargo fmt --all --check *> target/audit-round40-fmt.log
py tools/dev/check_legacy_refs.py *> target/audit-round40-legacy.log
```

日志核对方法（可复现命令，不重新运行 live gate）：

```powershell
Select-String -Path target/core1d.log -Pattern 'NPC对话|quest1|PS-1|SKIP|结果'
Select-String -Path target/reg-tp1c.log -Pattern 'TP-1|S.Death|S.Revived'
Select-String -Path target/sell-gate14.log -Pattern '850|金币|修理|FAIL|结果'
Select-String -Path target/buy1b.log,target/core1d.log -Pattern 'buy1|S.GainedItem'
Select-String -Path target/magic1b.log -Pattern 'NewMagic|ObjectMagic|MP'
Select-String -Path target/gate-resume2-175309.log -Pattern 'ignored|全部通过|== 汇总'
Select-String -Path target/launcher-server.log,target/launcher-server.err.log -Pattern 'resource/|Server\.MirDB|\.map'
```

## 逐条核验

### 1. quest1：pass（限定任务协议链）

查询：上列 `core1d` 命令。原文 [31–42 行](../../target/core1d.log#L31-L42)：

> `[NPC对话] 首行='你好 PT447490, 这是你的首次冒险吗?'`
> `[OK] quest1 接取 —— Jane @ (284, 606)`
> `[OK] quest1 交付/奖励 —— completed_quests 含 1；经验 0->50（+50，cjlaaa），金币 0->0`

接取、中文页、任务物品 #866x5、完成标记和经验 +50 均可见；不是 +60 金币/+48 经验的旧任务验收，不能移用旧数据判据。这里证明协议脚本观察，不证明真人 UI 的自主理解；组合最终仍 FAIL。

### 2. TP-1：partial

查询：上列 `reg-tp1c` 命令。[13–16 行](../../target/reg-tp1c.log#L13-L16) 有 `前往 比奇省(0)`，选门 `皇宫 pos=(347, 188)`；[109–112 行](../../target/reg-tp1c.log#L109-L112) 原文：

> `[地图] 切换地图 0@(346, 189) -> 0122@(11, 41)`
> `[OK] TP-1 到达 —— 0 -> 0122 @ (11, 41)（gate=皇宫）`

换图事实 PASS，但目标文字是比奇省(0)，脚本 [4052–4080 行](../../tools/playtest/playtest.py#L4052-L4080) 只断言导航文字后取 `self.movements[0]` 并检查换图，未验证任务目标关联、交互完成或客户端 UI 点击。不得将末尾“0 跳过”当完整 TP-1 阶梯 PASS。

### 3. PS-1：partial；完整修复尚未验证

查询：`Select-String -Path target/ps1-round40.log,target/core1c.log,target/core1d.log,target/ps1-fix.log -Pattern '重登前|重登后|逐字段|FAIL|结果'`。

- [ps1-round40:12–25](../../target/ps1-round40.log#L12-L25)：装备空、任务空、技能空、金币 0 的原始出生包重登通过，仅是基线。
- [core1c:77–83](../../target/core1c.log#L77-L83)：物品 `...8042` 从装备变成背包，任务奖励 `...8044` 丢失；衣服 5000→4996；`[FAIL] PS-1 逐字段比对`。
- [core1d:80–86](../../target/core1d.log#L80-L86)：奖励 `...8052` 消失，耐久 5000→4993；`[FAIL] PS-1 逐字段比对`。原日志此前已有耐久包，不足以仅凭快照差异断定真实耐久回滚，需区分观察器缓存与落盘。
- [pg_store:669–683](../../crates/mir-server/src/pg_store.rs#L669-L683) 已存在 `character_write_lock(character_id).await` 与 `lock.lock().await`；这证明修复代码存在，不自动证明并发正确性。
- 修后 [ps1-fix:12–25](../../target/ps1-fix.log#L12-L25) 有两件装备、相同五件 UID 的正常退出重登样本，逐行前后装备/耐久相同。这是正向证据，不是“未复跑”。但 [比较器:4171–4193](../../tools/playtest/playtest.py#L4171-L4193) 只将装备和背包合并成 UID 集合，`耐久差异（不判失败）`；其 PASS 不能覆盖装备槽及耐久恒等。completed/spells 仍为空，金币仍 0；“可战”仅是发 `C.Attack`，不是观察攻击结算。
- [persistence_equipment:91–114](../../crates/mir-server/tests/persistence_equipment.rs#L91-L114) 实际使用 `MemoryStore::new()`，不是 PG 读写锁的竞争测试；当前复跑 [17–23 行](../../target/audit-round40-shop-persistence.log#L17-L23) 为 2 passed。不能报告“PG 锁回归已由这两条证明”。

### 4. EQ-1：partial（磨损、卖肉、付费修理子链 pass）

查询：上列 `sell-gate14` 命令。[67–76 行](../../target/sell-gate14.log#L67-L76) 有掉落 `850 x1`、鸡肉 UID、`S.SellItem ... success=True` 和 `[金币] +40`；[70 行](../../target/sell-gate14.log#L70) 武器 current=3978；[91–95 行](../../target/sell-gate14.log#L91-L95)：

> `[金币] -2（累计 +38）`
> `[修理] unique_id=1879729405078536207 max=4000 current=4000（S.ItemRepaired 114）`
> `[OK] EQ-1 修理/扣款 ... 金币 40->40（扣 0）`

以包级 -2 为准，可推得 40−2=38；末尾 40→40 是观察汇总不一致，不算扣款 0。`@Sell/@Repair` 是 NPC 页键，不是玩家 GM 命令。另 [48 行](../../target/sell-gate14.log#L48) 先打印 `[FAIL] 捡起掉落`，后续重试成功而 [97 行](../../target/sell-gate14.log#L97) 说全部通过；因此证明最终子链成功，但不能称整份日志无 FAIL。该日志没有耐久到 0、损坏属性失效或修后再战证据，完整 EQ-1 仍 partial。

### 5. buy1：pass（限定免费书 live + 有价商店确定性测试）

查询：上列 buy1 命令。原指定 [buy1b:12–21](../../target/buy1b.log#L12-L21) 证明 0104 书店、货架 UID=201、免费购买汇总，但**没有** `S.GainedItem` 行；声明把该包归给 buy1b 不准确。补充正确来源 [core1d:65–74](../../target/core1d.log#L65-L74)：

> `[buy1] S.GainedItem item_index=973 uid=1879740154387628053 count=1`

本次 `npc_shop` 复跑 [6–15 行](../../target/audit-round40-shop-persistence.log#L6-L15) 为 7 passed，含所称四条 librarian 测试。源码 [434–560 行](../../crates/mir-server/tests/npc_shop.rs#L434-L560) 分别验证 500 金币及 LoseGold→GainedItem 包序、金币不足不发货、满包不扣钱、1级无技能免费书。它们是无 TCP 的世界层测试且依赖 Jev；本次 `--nocapture` 无 skip 输出，不能据此把免费 live 样本说成有价购买 live。

### 6. 升级：partial（确定性 pass；live SKIP）

实际执行请求命令 `cargo test -p mir-server --lib level_up`，exit 0，[过滤测试输出:4–7](../../target/audit-round40-level.log#L4-L7) 为 `1 passed; 0 failed; ... 143 filtered out`；该过滤字符串只匹配第一条，不能把它说成两条都跑了。另完整 lib 复跑 [132–136 行](../../target/audit-round40-lib.log#L132-L136) 两条均 `ok`。源码 [6247–6324 行](../../crates/mir-server/src/world.rs#L6247-L6324) 明确验证 `(2,4,200)`、HP/MP `(24,18)`、`S.LevelChanged`，以及连续跨级 `(3,49,300)`。

[core1d:88–89](../../target/core1d.log#L88-L89) 原文 `[SKIP] 升级 —— live 战斗随机；确定性证据由兵部二号 LevelChanged 单测覆盖`。live 没有升级 PASS；单测补逻辑证据，不替代真实玩家升级体验。

### 7. 穿装：pass（服务端属性/攻击模型）

命令：完整 lib `--nocapture`。输出 [149–152 行](../../target/audit-round40-lib.log#L149-L152)：

> `starter weapon: item_index=221 accuracy=7 dc=2..=4 hits=7225 rate=0.7225 damage=2..=4`
> `test world::tests::starter_weapon_equip_grants_dc_and_accuracy ... ok`

源码 [6363–6406 行](../../crates/mir-server/src/world.rs#L6363-L6406) 断言裸装 accuracy=5、装备后=7、成功装备包；[6409–6450 行](../../crates/mir-server/src/world.rs#L6409-L6450) 固定种子 10,000 刀验证命中率范围。72.25% 是本次确定性模拟值，不是 live 实战测量，也不要求随机 live 恰好相等。

### 8. 技能学习/施法：pass（协议与逻辑层）

查询：上列 magic1 命令。[magic1b:18–24](../../target/magic1b.log#L18-L24)：

> `[buy1] S.GainedItem item_index=1016 ... count=1`
> `[OK] magic1 学技能 —— S.NewMagic name=治愈术 spell=61；MP 15->15`
> `[OK] magic1 施法 —— S.ObjectMagic=True MP 15->12（期望 13→10）`

以实际 MP 减少 3 为准，括号旧期望不代表实际初值。完整 lib 输出 [150–153 行](../../target/audit-round40-lib.log#L150-L153) 两条学习测试通过。源码 [6465–6535 行](../../crates/mir-server/src/world.rs#L6465-L6535) 验证 Fencing 被动不能施法、不耗蓝；[6540–6604 行](../../crates/mir-server/src/world.rs#L6540-L6604) 验证 Healing 学习/消耗 3/施法包。不外推为技能升级或所有法术可用。

### 9. DR-1：partial（不能将 live 未覆盖说成死亡完全未发生）

命令：完整 lib `--nocapture`，结果 [131、143–144 行](../../target/audit-round40-lib.log#L131-L144) 三条通过；源码 [6130–6244 行](../../crates/mir-server/src/world.rs#L6130-L6244) 分别验证死亡包、直接调用 town revive 回绑定点恢复 HP/MP、不碰物品、60s 原地兜底。

现有 live [reg-tp1c:21、45](../../target/reg-tp1c.log#L21-L45) 原文：

> `[DR-1] 收到 S.Death @ (284, 542) t=1792634091.160 hp=0`
> `[DR-1] 收到 S.Revived @ (284, 542) t=1792634151.218 hp=16`

相差约 60.058s，同坐标，证明自然死亡与兜底复活样本，而不是客户端确认的回城复活。**完整 DR-1 live 未覆盖**：无死亡动画弹窗、是/否/不点发送次数、12s 不自动复活、确认 TownRevive 后回城并继续战斗的整链证据。单测直接设置 HP/调用方法不能冒充无捷径 live。

### 10. 发布形态：pass（限定 headless packs 冒烟）

实际独立复跑 `pwsh -NoProfile -File tools/dev/release-smoke.ps1` exit 0。[审计输出:1–11](../../target/audit-round40-release.log#L1-L11)：45 packs、151581 entries、67 libraries、清单 sha256 `0be09eedf94e8caf44df0840a68d2974735a96d88bd5bafd65f42c57f90f0226`，`check ok`，`pixels=480000 diff_vs_dev=0`，`smoke: OK`。

这是特定场景的一帧、部分图块/怪物帧 headless 验证，不是客户端完整交互、全地图资源覆盖或安装包长期运行验收。

### 11. 纯自有资源：partial（当前日志检索 pass；第40轮时间窗不足）

实际执行上列 `Select-String` 等价计数：当前 `launcher-server.log` 与 `.err.log` 对 `resource/|Server\.MirDB|\.map` 均 0 条，err 文件为空。但 [round40 启动摘要](../../target/round40-launcher.out#L1-L5) 为 7122，[round41 摘要](../../target/round41-launcher.out#L1-L5) 为 7123；目前 [server log 起始](../../target/launcher-server.log#L1-L8) 为 `2026-10-22T10:06:52Z` 的 pt160845 会话（即18:06本地时间），未保留第40轮完整启动加载窗口。

因此不能据零匹配证明第40轮启动从未访问旧资源；日志未记录文件访问也不等于没有访问。自有格式/无老格式引用与版权归属不同，本审计不证明版权。独立静态守卫结果仅作补充，不代替该时间窗。

### 12. 门禁：pass（已有全量终态 + 独立关键子集，非零跳过承诺）

原全量入口为 `pwsh -NoProfile -File tools/dev/ci.ps1 -KillStale`；本次未重启/清理他人服务端，也未复跑全量。证据 [gate-resume2:2904–2917](../../target/gate-resume2-175309.log#L2904-L2917) 十二项均 OK，原文 `==> 全部通过（12 项）`；[2892–2900 行](../../target/gate-resume2-175309.log#L2892-L2900) 真实 PG 故障测试 6 passed，含 durability roundtrip。

边界：[18–27 行](../../target/gate-resume2-175309.log#L18-L27) duo_smoke `5 passed; 2 ignored`，两条组队经验 live 用例因 flaky 忽略；不能说所有测试均执行。该门禁文件修改时间 17:57:33，main ref 更新时间18:01:59；日志未附 clean HEAD 哈希，不能独立证明十二项恰对应当前 clean commit。当前关键 lib 144 / shop 7 / MemoryStore equipment 2 及 release 冒烟已独立复跑。

## 下一轮建议（验收要求，不含实现方案）

1. 恢复 PS-1 可检出的字段合同：装备槽、背包槽、模板、数量、耐久及非空任务/技能/非零金币，允许正常磨损必须有同期包级因果证据；增加负例证明槽位迁移/耐久回滚不能假绿。修后完整组合需要真实终态证据。
2. 保留 live 升级和确认式 DR-1 的未覆盖标签；先验死亡→客户端确认→回城→继续玩，不能用60s兜底替代。EQ-1 的0耐久失效/修后战斗单独补齐。
3. TP-1 必须关联任务目标而非任意第一个门，保留客户端实际指引/点击/到目标交互证据；每轮归档完整启动日志、构建身份、判据版本及退出码，避免后来窗口冒充前轮。

## 审计边界

没有新起服务端，没有改产品代码，没有执行 git。旧日志报告的 PASS/SKIP/FAIL 均保留原义；不将若干不同角色、不同端口、不同版本的分段成功拼接成一条零 SKIP 全链路成功。运行中的日志可能继续追加，后续需要附加审计，不覆盖本文已采集事实。

独立命令退出码：lib=0（144 passed）、npc_shop/persistence_equipment=0（7+2 passed）、level_up过滤=0（1 passed）、release-smoke=0、fmt=0、legacy守卫=0、文档链接检查=0。静态守卫[原文](../../target/audit-round40-legacy.log#L1-L2)：`运行时老格式引用：当前 0 条 / 基线 0 条（上限 0）`。文档链接检查[输出](../../target/audit-round40-doc-links.log#L1) 与命令 `py tools/docs/check_links.py` 一并保留；首次重定向中文有编码损坏，退出码与数字可读，不将乱码复原成未观察到的原文。

可固定旧证据 SHA256（18:08 读取；不对正在写入且无法加文件哈希锁的日志伪造哈希）：

| 证据 | SHA256 |
|---|---|
| [core1d](../../target/core1d.log) | `B2C0C043F17AB7D48B329683555E72BBEC7EA5ADC5B1B882E1709DDC26A927B7` |
| [ps1-fix](../../target/ps1-fix.log) | `B071F024B3E529AA5047A7C626C9AC069025722CBDE1368CB7AB86B16DA8AB27` |
| [12项门禁](../../target/gate-resume2-175309.log) | `A1D4439BBE4B69C7A7F69FB494002F5618405019DB6874B795D5F2B952ABF403` |

