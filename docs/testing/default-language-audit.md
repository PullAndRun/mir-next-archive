# 默认语言独立核验（2026-10-22）

> 核验人：刑部（xingbu）。只读 + 只跑命令；未启动服务端、未改代码。
> 核验基线：当前工作区 `HEAD=78798fc`（`git` 未执行，只读取 `.git/refs/heads/main`）。
> 结论：**“默认全中文”在配置层、默认数据集和核心文本上成立，但不能表述为 100% 全中文。**
> 主要边界：物品/怪物/技能仍有非中文名；NPC 显示名是 A+ 映射层；英文模式只切换 UI/名称层，数据名仍来自中文库。
> 另有一个**运行时异常必须放在最前**：中文脚本存在，但 `cjk-quest5.log` 里 Jane 的对话页原文为 `[]`，见 §0.1。

---

## 0. 先给与“全中文”不完全一致的事实

### 0.1 Jane 脚本是中文，但真机日志的对话页为空

脚本 `resource/community/suprcode-crystal-database/cjlaaa/Envir/NPCs/BichonProvince/BorderVillage/0Helper-0.txt` 第 12-17 行确实是中文：

```text
12: [@Main-1]
13: #SAY
14: 你好 <$USERNAME>, 这是你的首次冒险吗?
15: 请按 ('H') 来打开帮助菜单.
16:  
17: <关闭/@exit>
```

但工部现有日志 `target/cjk-quest5.log:63` 原始行是：

```text
  [quest1] Jane 对话页原文=[]
```

同一份日志第 65 行仍记录任务接取成功，说明**脚本文件中文成立，但这次运行没有采集到对话页行**；这与“NPC 对话在运行时已全中文展示”不是同一件事。该项应作为待查项，不应写成“已确认运行时对话全中文”。

### 0.2 请求字面参数 `--language en` 不存在

代码实际解析的是 `--lang` 与 `--set-language`：

- `crates/mir-client/src/main.rs:112-129`：`"--lang"` / `"--set-language"`
- 未找到 `"--language"` 参数分支。

所以 `--language en` 会走 `:287` 的 `未知参数`；可用的英文切换路径是 `--lang en` 或 `--set-language en`（或 `config/client.toml` 的 `language = "en"`）。如果验收命令字面要求 `--language`，这是一个命名/文档缺口，而不是语言功能缺失。

### 0.3 未覆盖名称不是 0

`docs/world/data-languages.md` 第 28-34 行的实测：
- 物品 `ItemInfo.Name`：1156/1349 = 85.7% 中文 → **14.3% 非中文**
- 怪物 `MonsterInfo.Name`：420/544 = 77.2% 中文 → **22.8% 非中文**
- 技能 `MagicInfo.Name`：78/107 = 72.9% 中文 → **27.1% 非中文**
- NPC `NpcInfo.Name`：14/309 = 4.5% 中文（A+ 显示名映射层另有 291 个 `npc_names` 键，见 §5）

因此“默认全中文”只能作为**默认 UI/默认数据/核心文本的中文**，不能作为“所有名称 100% 中文”。

---

## 1. 默认数据源：`GameData::discover()` 第一候选是 cjlaaa

### 证据

`crates/mir-server/src/gamedata.rs:58-65`：

```rust
pub const DB_CANDIDATES: &[&str] = &[
    // v105，中文库 + 完整 Envir；默认数据集（docs/world/data-languages.md）
    "resource/community/suprcode-crystal-database/cjlaaa/Server.MirDB",
    // v111，英文库，仅兜底
    "resource/community/suprcode-crystal-database/Jev/Server.MirDB",
    // v83，旧格式/英文兜底
    "resource/community/clgforever910-mir2database/Daneo1989/Server.MirDB",
];
```

`gamedata.rs:790-809` 的单测同时断言：默认路径含 `cjlaaa`、map0 标题含 CJK、物品/怪物名过半 CJK。

### 命令与输出

在仓库根目录、**不设置** `MIR2_DB_PATH`：

```powershell
cargo test -p mir-server --lib default_dataset_is_the_chinese_cjlaaa -- --nocapture
```

实测：

```text
running 1 test
test gamedata::tests::default_dataset_is_the_chinese_cjlaaa ... ok

test result: ok. 1 passed; 0 failed; 0 ignored
```

命令退出码：**0**。

---

## 2. 默认语言配置

### 2.1 账号语言迁移默认 `zh-cn`

`crates/mir-store/migrations/0002_account_language.sql:14`：

```sql
ALTER TABLE accounts ADD COLUMN language TEXT NOT NULL DEFAULT 'zh-cn';
```

### 2.2 客户端配置默认 `zh-cn`

`crates/mir-client/src/client_config.rs:146-150`：

```rust
impl Default for ClientConfig {
    fn default() -> Self {
        Self {
            language: "zh-cn".to_owned(),
```

单测 `crates/mir-client/src/client_config.rs:333-335` 断言 `config.language == "zh-cn"`。

### 2.3 客户端设置默认 `zh-cn`

`crates/mir-client/src/settings.rs:63-66`：

```rust
fn default_language() -> String {
    // 项目要求默认全中文化。
    Language::ZhCn.code().to_owned()
}
```

`settings.rs:103-107` 的 `Default` 使用 `default_language()`；单测 `settings::tests::defaults_to_simplified_chinese`（`settings.rs:290`）通过。

### 2.4 `LanguageCatalog` 默认 `ZhCn`

`crates/mir-core/src/i18n.rs:2324-2330`：

```rust
impl LanguageCatalog {
    pub const fn new() -> Self {
        Self {
            language: Language::ZhCn,
        }
    }
```

单测 `i18n::tests::default_language_is_simplified_chinese`（`i18n.rs:2435`）通过。

### 实测命令与输出

```powershell
cargo test -p mir-core --lib i18n::tests::default_language_is_simplified_chinese -- --exact
cargo test -p mir-client --lib client_config::tests::defaults_are_complete -- --exact
cargo test -p mir-client --lib settings::tests::defaults_to_simplified_chinese -- --exact
```

三条均为：

```text
test result: ok. 1 passed; 0 failed; 0 ignored
```

退出码均为 **0**。

---

## 3. 真实证据（工部日志，未起服务）

`target/eq1-gate*.log` 与 `target/dr1-green*.log` 在当前 `target/` **不存在**；本节使用实际存在的 `target/cjk-quest5.log` 与 `target/gate-cjk-032850.log`，不把不存在的文件写成已检查。

### 3.1 `target/cjk-quest5.log`（UTF-8，73 行）

原始行：

```text
1: [t=+00:00] [OK]   连接服务端 —— 127.0.0.1:7105
10:   [quest1] 怪物名样本=['皇城弓箭护卫', '皇城卫士', '皇城卫士', '皇城卫士', '皇城弓箭护卫', '皇城卫士']
11:   [聊天] 欢迎来到传奇-Next。
12: 新手提示：背包里已有出生装；到书店买技能书，双击阅读即可学会第一个技能。
63:   [quest1] Jane 对话页原文=[]
65: [t=+05:41] [OK]   quest1 接取 —— Jane @ (284, 606)
66:   [quest1] task_list=['把食人花叶捎给工艺夫人', '前往 比奇省(0)']
67: [t=+05:41] [OK]   quest1 导航行 —— 前往 比奇省(0)
71: [t=+05:48] [OK]   quest1 交付/奖励 —— completed_quests 含 1；经验 0->50（+50，cjlaaa），金币 0->0
73: 结果：全部通过（0 跳过）
```

可见**怪物名、系统问候、任务文本、导航行、任务奖励记录都是中文**；第 63 行的 Jane 对话页为空，单独列为待查（§0.1）。

### 3.2 `target/gate-cjk-032850.log`

该日志是测试/服务端运行日志，能看到中文服务端 debug 行，例如：

```text
1878: ...DEBUG mir_server::session: NPC 对话 session=2 npc=1 page=[@BUYSELL] lines=4 panels=["buy", "sell"]...
1881: ...DEBUG mir_server::session: 买到物品 session=2 item=973 count=1 cost=500
1885: ...DEBUG mir_server::session: 使用物品 session=2 item=Fencing hp=0 mp=0...
```

说明服务端内部消息为中文；但这一行只证明 page 名和面板，不证明 Jane/Smith/Butcher 具体正文。具体正文由 §4 的脚本原始行证明，运行时 Jane 页仍按 §0.1 记录。

---

## 4. NPC 对话中文：3 个脚本原始行

### 4.1 Jane（`Assistant_Jane`）

文件：`resource/community/suprcode-crystal-database/cjlaaa/Envir/NPCs/BichonProvince/BorderVillage/0Helper-0.txt`

```text
12: [@Main-1]
13: #SAY
14: 你好 <$USERNAME>, 这是你的首次冒险吗?
15: 请按 ('H') 来打开帮助菜单.
16:  
17: <关闭/@exit>
```

### 4.2 Smith（`Blacksmith_Smith`）

文件：`resource/community/suprcode-crystal-database/cjlaaa/Envir/NPCs/BichonProvince/BorderVillage/Blacksmith-0.txt`

```text
12: [@Main-1]
13: #SAY
14: 你好，请问你需要些什么？
15:  
16: <浏览/@BuySell> 商店.
17: <修理/@Repair> 武器.
```

### 4.3 Butcher（`Merchant_Sarah`）

文件：`resource/community/suprcode-crystal-database/cjlaaa/Envir/NPCs/BichonProvince/BichonWall/Butcher-0.txt`

```text
12: [@Main-1]
13: #SAY
14: 欢迎，我能为你做些什么?
15:  
16: <卖/@Sell> 肉.
17: <询问/@肉help> 如何获得肉.
```

结论：**三个脚本的正文均为中文**；Jane 的运行时页面采集异常见 §0.1，不能把脚本存在等同于运行时已正确展示。

---

## 5. 语言切换仍可用；英文模式的数据名限制

### 5.1 切换路径

- 客户端 CLI 实际参数：`--lang en` / `--set-language en`（`crates/mir-client/src/main.rs:112-129`）
- 配置文件：`language = "en"`（`crates/mir-client/src/client_config.rs:219` 解析；`:252` 保存）
- 客户端设置：`ClientSettings::set_language`（`settings.rs:150`）
- 相关单测：`client_config::tests::language_switch_normalises_codes`、`settings::tests::set_language_normalises_aliases`（均实测 `1 passed`）
- `Language::En` 的 UI 标签由 `crates/mir-client/src/ui/labels.rs` 提供

**注意**：字面 `--language en` **不是**当前支持的参数（见 §0.2）；如果要求该命令行，需要补别名或修改验收命令。

### 5.2 英文模式的数据名限制（已知限制，不按失败计）

`docs/world/data-languages.md:239` 明确：

> 本层只解决**显示名**；`NpcInfo.Name`（DB 原名）与 Jev 契约不变；英文模式始终显示 DB 原名。

本作切换语言主要切 UI/语言包；`ItemInfo.Name`、`MonsterInfo.Name`、`MagicInfo.Name` 仍直接来自 cjlaaa 数据（其中本身就有 §0.3 的非中文名）。因此英文模式下这些数据名仍可能是中文/混合，属于已登记限制。

---

## 6. 未覆盖清单（来源：`docs/world/data-languages.md`）

| 项 | 中文覆盖 | 非中文 |
|---|---|---|
| 物品 `ItemInfo.Name` | 1156/1349 = 85.7% | **14.3%** |
| 怪物 `MonsterInfo.Name` | 420/544 = 77.2% | **22.8%** |
| 技能 `MagicInfo.Name` | 78/107 = 72.9% | **27.1%** |
| NPC `NpcInfo.Name` 原值 | 14/309 = 4.5% | A+ 显示名映射层覆盖 291 个 `npc_names` 键（`config/lang/zh-cn.json` 实测 `npc_names=291`；`en.json` 同名键集合由 docs A+.4 记录） |

`docs/world/data-languages.md:197` 记录 A+ 统计：有据 190/309（135 纯有据 + 55 partial-cleaned），generated 119；`docs/world/data-languages.md:237-239` 记录 generated 和英文模式限制。以上数字是文档实测口径，本文不重新推导百分比。

---

## 7. 最终判定

| 核验项 | 判定 | 依据 |
|---|---|---|
| 默认数据源 cjlaaa | **成立** | `gamedata.rs:58-65`；单测 `default_dataset_is_the_chinese_cjlaaa` 1 passed |
| 默认语言 zh-cn | **成立** | 迁移 0002:14；client_config:149；settings:63-66；i18n:2327-2330；4 条单测均 1 passed |
| 核心文本中文（问候/任务/导航/怪物名） | **成立** | `cjk-quest5.log` 原始行 10/11/12/66/67/71 |
| NPC 脚本中文（Jane/Smith/Butcher） | **成立** | 三份脚本第 12-17 行原文 |
| NPC 运行时对话页 | **有异常，待查** | `cjk-quest5.log:63` `Jane 对话页原文=[]` |
| 全部名称 100% 中文 | **不成立** | 物品 14.3% / 怪物 22.8% / 技能 27.1% 非中文；NPC 原值仅 4.5% 中文 |
| `--language en` 字面命令 | **不成立（当前 CLI 无此参数）** | `main.rs` 只有 `--lang` / `--set-language` |
| `language = "en"` 配置切换 | **成立** | `client_config.rs:219/252`；单测通过 |

**总判定：“默认全中文”部分成立。** 作为默认体验，数据源、默认语言、UI 语言包和核心任务/怪物/导航文本已经是中文；但名称数据仍有明确未覆盖，NPC 显示名依赖 A+ 映射层，英文模式保留数据原名，且 Jane 运行时对话页有采集异常。不能把本结果写成“100% 全中文”。

---

## 8. 复现命令汇总

```powershell
# 默认数据源单测
cargo test -p mir-server --lib default_dataset_is_the_chinese_cjlaaa -- --nocapture

# 默认语言单测
cargo test -p mir-core --lib i18n::tests::default_language_is_simplified_chinese -- --exact
cargo test -p mir-client --lib client_config::tests::defaults_are_complete -- --exact
cargo test -p mir-client --lib settings::tests::defaults_to_simplified_chinese -- --exact
cargo test -p mir-client --lib client_config::tests::language_switch_normalises_codes -- --exact

# 日志原始行（只读）
Get-Content target/cjk-quest5.log -Encoding utf8
Get-Content target/gate-cjk-032850.log -Encoding utf8 | Select-Object -Skip 1877 -First 10

# 脚本原始行（只读）
Get-Content 'resource/community/suprcode-crystal-database/cjlaaa/Envir/NPCs/BichonProvince/BorderVillage/0Helper-0.txt' -Encoding utf8
Get-Content 'resource/community/suprcode-crystal-database/cjlaaa/Envir/NPCs/BichonProvince/BorderVillage/Blacksmith-0.txt' -Encoding utf8
Get-Content 'resource/community/suprcode-crystal-database/cjlaaa/Envir/NPCs/BichonProvince/BichonWall/Butcher-0.txt' -Encoding utf8
```
