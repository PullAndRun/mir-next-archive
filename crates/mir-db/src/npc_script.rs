//! NPC 脚本（`Envir/NPCs/<file>.txt`）解析：**纯数据**，不依赖服务端。
//!
//! 逐条对照 Crystal `Server/MirObjects/NPC/NPCScript.cs` 与
//! `Server/MirObjects/NPC/NPCSegment.cs` 的解析流程：
//!
//! | 本模块 | Crystal | 说明 |
//! |---|---|---|
//! | [`NpcScript::load`] | `NPCScript.LoadInfo`（`NPCScript.cs:136-166`） | 读文件 → 展开 `#INSERT`/`#INCLUDE` → 切页 |
//! | `expand_inserts` | `NPCScript.ParseInsert`（`:303-328`） | `#INSERT [路径]`，路径相对 `Envir/` |
//! | `expand_includes` | `NPCScript.ParseInclude`（`:330-385`） | `#INCLUDE [路径] @Section` + `{`…`}` 块 |
//! | `parse_sections` | `NPCScript.ParsePage`（`:415-507`） | `[@Section]` 切页、`#IF` 切段 |
//! | `parse_segment` | `NPCScript.ParseSegment`（`:509-620`） | `#IF`/`#SAY`/`#ACT`/`#ELSE*`、`<text/@goto>` 按钮 |
//! | [`NpcScript::goods`] | `NPCScript.ParseGoods`（`:650-684`） | `[TRADE]` 段：`物品名 [数量]` |
//! | [`NpcScript::types`] | `NPCScript.ParseTypes`（`:622-649`） | `[TYPES]` / `[USEDTYPES]` 段 |
//! | [`NpcScript::quest_indexes`] | `NPCScript.ParseQuests`（`:685-721`） | `[QUESTS]` 段：正数=发布、负数=交付 |
//!
//! # 商品表到底在哪
//!
//! 任务描述里提到的「`[Goods]` 段 / `Envir/Goods/*.txt`」在 Crystal 里并不存在：
//!
//! * **卖的东西**写在同一份 NPC 脚本的 `[TRADE]` 段里（`NPCScript.cs:652-684`，关键字是
//!   `TradeKey = "[TRADE]"`，`NPCScript.cs:75`），每行 `物品名 [数量]`；
//! * `Envir/Goods/<NPC 序号>.msd` 是**二进制**文件，装的是「玩家卖回给 NPC 的二手货」
//!   （`UsedGoods`，`NPCScript.LoadGoods`，`:180-217`），不是商品表。本作暂不实现（见
//!   `docs/world/npc.md` 的未做清单），[`NpcScript`] 只解析文本部分。
//!
//! # 简化
//!
//! * 段落/页的查找用**精确 key**（Crystal 用 `StartsWith` 做前景扫描，`NPCScript.cs:433`）；
//!   现实中脚本的页头都写成 `[@Name]`，精确匹配的结果与 Crystal 一致，且不会误命中。
//! * `%ARG(n)`（`NPCSegment.cs:51-68`）与 `%A1` 变量（`:86-100`）不做替换：样本库里只有
//!   4 个按钮带参数、没有一处用到 `%ARG`，服务端拿到的就是原样参数。
//! * `#INSERT` 的展开带 64 次上限（Crystal 用 `lines.AddRange` 后继续遍历，自引用会无限增长）。

use std::path::{Path, PathBuf};

/// 主页面 key（Crystal `NPCScript.MainKey`，`NPCScript.cs:44`）。
pub const MAIN_SECTION: &str = "[@MAIN]";

/// 退出页面 key。
///
/// Crystal 服务端**不认识**它：客户端点 `<Close/@exit>` 时直接关窗口、不发包
/// （`Client/MirScenes/Dialogs/NPCDialogs.cs:288-293`）。本作仍把它当「结束对话」处理，
/// 见 `crate::npc` 的说明。
pub const EXIT_SECTION: &str = "[@EXIT]";

/// 一次载入最多展开多少条 `#INSERT` / `#INCLUDE`。
pub const MAX_INCLUDES: usize = 64;

/// NPC 脚本解析错误。
#[derive(Debug, thiserror::Error)]
pub enum NpcScriptError {
    /// 读文件失败（脚本不存在、权限、编码等）。
    #[error("读取 NPC 脚本 {path} 失败：{source}")]
    Io {
        /// 出错的路径。
        path: String,
        /// 底层 IO 错误。
        #[source]
        source: std::io::Error,
    },
}

/// 一个 NPC 脚本（一份 `Envir/NPCs/<file>.txt` 的解析结果）。
#[derive(Debug, Clone, Default, PartialEq)]
pub struct NpcScript {
    /// 脚本文件名（不带目录与 `.txt`，即 `NpcInfo.file_name`）。
    pub file_name: String,
    /// 页面列表（按文件里出现的顺序；key 已统一成大写）。
    pub sections: Vec<NpcPage>,
    /// `[TRADE]` 段的商品（`NPCScript.cs:650-684`）。
    pub goods: Vec<GoodsItem>,
    /// `[TYPES]` 段：这个 NPC 允许交易/修理的 `ItemType` 原始值（`NPCScript.cs:622-635`）。
    pub types: Vec<i32>,
    /// `[USEDTYPES]` 段：允许回收的 `ItemType` 原始值（`NPCScript.cs:637-648`）。
    pub used_types: Vec<i32>,
    /// `[QUESTS]` 段：这个 NPC 关联的任务（`NPCScript.ParseQuests`，`NPCScript.cs:685-721`）。
    ///
    /// Crystal 的语义是**带符号**的（`:711-714`）：
    ///
    /// * **正数** `N` → 这个 NPC 是任务 `N` 的发布者（`QuestInfo.NpcIndex = LoadedObjectID`）；
    /// * **负数** `-N` → 这个 NPC 是任务 `N` 的交付者（`QuestInfo.FinishNpcIndex = LoadedObjectID`）。
    ///
    /// 本作保留原始符号，查询用 [`NpcScript::publishes_quest`] / [`NpcScript::finishes_quest`]。
    pub quest_indexes: Vec<i32>,
    /// 解析过程中的告警（缺文件、语法不认识等）。不参与逻辑，只给日志与测试看。
    pub warnings: Vec<String>,
}

/// 一页对话（Crystal `NPCPage`，`Server/MirObjects/NPC/NPCPage.cs:5-19`）。
#[derive(Debug, Clone, Default, PartialEq)]
pub struct NpcPage {
    /// 页面 key，大写且带方括号，例如 `[@MAIN]`。
    pub key: String,
    /// 页面里的段落（`#IF` 分隔；Crystal `NPCPage.SegmentList`）。
    pub segments: Vec<NpcSegment>,
    /// 页面里所有按钮的并集（Crystal `NPCPage.Buttons`，`NPCScript.cs:468-473`）。
    pub buttons: Vec<String>,
}

/// 一个段落（Crystal `NPCSegment`，`Server/MirObjects/NPC/NPCSegment.cs:14-49`）。
#[derive(Debug, Clone, Default, PartialEq)]
pub struct NpcSegment {
    /// `#IF` 的条件行（Crystal `NPCSegment.CheckList`）。
    pub checks: Vec<NpcCheck>,
    /// `#ACT` 的动作行（Crystal `NPCSegment.ActList`）。
    pub actions: Vec<NpcAction>,
    /// `#ELSEACT` 的动作行（Crystal `NPCSegment.ElseActList`）。
    pub else_actions: Vec<NpcAction>,
    /// `#SAY` 的正文（原样保存，占位符与颜色标记留给服务端渲染）。
    pub say: Vec<String>,
    /// `#ELSESAY` 的正文。
    pub else_say: Vec<String>,
    /// `#SAY` 里的按钮（`<文本/@goto>` 中的 `[@goto]`，大写）。
    pub buttons: Vec<String>,
    /// `#ELSESAY` 里的按钮。
    pub else_buttons: Vec<String>,
    /// `#ACT`/`#ELSEACT` 里 `GOTO` 一类动作指向的页面（Crystal `GotoButtons`）。
    pub goto_buttons: Vec<String>,
    /// `#ELSEIF` 的条件行。
    ///
    /// Crystal 走到这里会抛 `NotImplementedException`（`NPCSegment.cs:557-558`）——
    /// 也就是说 `#ELSEIF` **不是** Crystal 支持的语法。本作把它收在这里并忽略，
    /// 保证这类脚本仍能加载（样本库 660 个脚本里 0 处使用）。
    pub else_checks: Vec<String>,
}

/// 一条条件行（Crystal `NPCChecks { CheckType Type; List<string> Params }`，`NPCChecks.cs:3-15`）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NpcCheck {
    /// 条件种类。
    pub kind: NpcCheckKind,
    /// 关键字之后的原始参数（按空格切分，已去掉空项）。
    pub params: Vec<String>,
    /// 原始整行（大写关键字 + 原参数），用于日志与「不支持」提示。
    pub source: String,
}

/// 本作认识的条件种类。
///
/// 对应 Crystal `CheckType`（`NPCChecks.cs:17-69`）里本作实现了的那几个；其余一律
/// [`NpcCheckKind::Unsupported`]，再按关键字分派就是 `mir-server` 的事。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum NpcCheckKind {
    /// `LEVEL <op> <n>`（Crystal `CheckType.Level`，解析 `NPCSegment.cs:119-123`）。
    Level,
    /// `CHECKGOLD <op> <n>`（`CheckType.CheckGold`，`:125-129`）。
    Gold,
    /// `CHECKPKPOINT <op> <n>`（`CheckType.CheckPkPoint`，`:221-225`）。
    PkPoint,
    /// `CHECKITEM <物品名> [数量] [持久(秒)]`（`CheckType.CheckItem`，`:140-147`）。
    Item,
    /// `CHECKCLASS <职业名>`（`CheckType.CheckClass`，`:155-159`）。
    Class,
    /// `CHECKGENDER <性别名>`（`CheckType.CheckGender`，`:149-153`）。
    Gender,
    /// `CHECKQUEST <任务序号> ACTIVE|COMPLETE`（`CheckType.CheckQuest`，`:348-352`）。
    Quest,
    /// `CHECK [<flag>] <0|1>`（`CheckType.Check`，`:245-253`）。
    Flag,
    /// `RANDOM <n>`：`Envir.Random.Next(0, n) == 0` 才算通过（`CheckType.Random`，`:2414-2422`）。
    Random,
    /// 认识关键字但本作没实现的条件（例如 `CHECKMAP`、`CHECKNAMELIST`）。
    Unsupported,
    /// 连关键字都不认识。
    #[default]
    Unknown,
}

/// 一条动作行（Crystal `NPCActions { ActionType Type; List<string> Params }`，`NPCActions.cs:3-15`）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NpcAction {
    /// 动作种类。
    pub kind: NpcActionKind,
    /// 关键字之后的原始参数。
    pub params: Vec<String>,
    /// 原始整行。
    pub source: String,
}

/// 本作实现的动作种类（对应 Crystal `ActionType`，`NPCActions.cs:17-122`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum NpcActionKind {
    /// `GOTO @页面`（`ActionType.Goto`，解析 `NPCSegment.cs:550-554`，执行 `:3684-3689`）。
    Goto,
    /// `BREAK`：本页剩下的段落不再处理（`ActionType.Break`，`:579-581`、`:3700-3704`）。
    Break,
    /// `GIVEGOLD <n>`（`ActionType.GiveGold`，`:467-471`、`:3244-3253`）。
    GiveGold,
    /// `TAKEGOLD <n>`（`ActionType.TakeGold`，`:473-477`、`:3255-3264`）。
    TakeGold,
    /// `SET [<flag>] <0|1>`（`ActionType.Set`，`:796-804`、`:3706-3726`）。
    SetFlag,
    /// `ACCEPTQUEST <任务序号>`：**本作扩展**，替玩家接取任务。
    ///
    /// Crystal 的 `NPCActions` 里**没有**这个动作（全仓库搜索 `ACCEPTQUEST` 零命中），
    /// 任务接取只能由客户端弹内置任务面板后发 `C.AcceptQuest`（`ClientPackets.cs`，
    /// 服务端入口 `PlayerObject.AcceptQuest`，`PlayerObject.cs:11251`）。
    /// 加上它是因为任务书要求「让 161 份真实脚本里的任务流程可用」，见 `docs/world/quests.md`。
    AcceptQuest,
    /// `FINISHQUEST <任务序号>`：**本作扩展**，替玩家交付任务（Crystal 无此动作）。
    FinishQuest,
    /// `ABANDONQUEST <任务序号>`：**本作扩展**，替玩家放弃任务（Crystal 无此动作）。
    AbandonQuest,
    /// 认识关键字但本作没实现（例如 `GIVEITEM`、`MONGEN`、`MOV`）。
    Unsupported,
    /// 不认识的关键字。
    #[default]
    Unknown,
}

/// `[TRADE]` 里的一件商品。
///
/// Crystal 的 `ParseGoods` 会把每行解析成 `UserItem`（`Envir.CreateShopItem(info, (uint)i)`，
/// `NPCScript.cs:667`），其中 `UniqueID` 就是**这一行在脚本里的行号**。本作保留同样的编号，
/// 这样客户端用 `C.BuyItem { item_index }` 买东西时对得上号。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GoodsItem {
    /// 商品编号（Crystal `UserItem.UniqueID`）：该行在展开后的脚本行里的下标。
    pub unique_id: u64,
    /// 物品模板名（`ItemInfo.Name` 的写法，例如 `WoodenSword`）。
    pub name: String,
    /// 一次买几个（脚本第二列；缺省 1）。
    pub count: u16,
}

/// 条件比较运算符（Crystal `NPCSegment.Compare`，`NPCSegment.cs:4996-5008`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompareOp {
    /// `<`
    Lt,
    /// `>`
    Gt,
    /// `<=`
    Le,
    /// `>=`
    Ge,
    /// `==`
    Eq,
    /// `!=`
    Ne,
}

impl CompareOp {
    /// 解析运算符；不认识返回 `None`（Crystal 在 `Compare` 里抛 `ArgumentException`）。
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "<" => Some(Self::Lt),
            ">" => Some(Self::Gt),
            "<=" => Some(Self::Le),
            ">=" => Some(Self::Ge),
            "==" => Some(Self::Eq),
            "!=" => Some(Self::Ne),
            _ => None,
        }
    }

    /// 按 `Compare` 的语义比较两个数（传引用：比较不消费参数）。
    #[must_use]
    pub fn compare<T: Ord>(self, left: &T, right: &T) -> bool {
        match self {
            Self::Lt => left < right,
            Self::Gt => left > right,
            Self::Le => left <= right,
            Self::Ge => left >= right,
            Self::Eq => left == right,
            Self::Ne => left != right,
        }
    }
}

impl NpcScript {
    /// 载入一个脚本。
    ///
    /// `file_name` 是数据库里的 `NpcInfo.file_name`（例如
    /// `BichonProvince/BichonWall/Blacksmith1`，可以带 `.txt`，也可以是绝对路径）；
    /// `envir_root` 是 `Envir/` 目录，用于拼 `Envir/NPCs/…` 与展开 `#INSERT` 的相对路径
    /// （Crystal `Settings.NPCPath` / `Settings.EnvirPath`，`Server/Settings.cs:18,24`）。
    pub fn load(
        file_name: impl AsRef<Path>,
        envir_root: impl AsRef<Path>,
    ) -> Result<Self, NpcScriptError> {
        let path = resolve_script_path(file_name.as_ref(), envir_root.as_ref());
        let lines = read_lines(&path)?;
        let name = file_name.as_ref().to_string_lossy().replace('\\', "/");
        Ok(Self::parse_lines(&name, lines, Some(envir_root.as_ref())))
    }

    /// 直接从内存行解析（不做 `#INSERT` / `#INCLUDE` 展开——那两步需要文件系统）。
    ///
    /// `file_name` 只用于记录（`[TRADE]` 的行号与它无关）。
    #[must_use]
    pub fn parse(file_name: impl AsRef<str>, lines: Vec<String>) -> Self {
        Self::parse_lines(file_name.as_ref(), lines, None)
    }

    fn parse_lines(file_name: &str, mut lines: Vec<String>, envir_root: Option<&Path>) -> Self {
        let mut warnings = Vec::new();
        let name = file_name
            .strip_suffix(".txt")
            .unwrap_or(file_name)
            .to_owned();
        if let Some(root) = envir_root {
            expand_includes(&mut lines, root, &mut warnings);
            expand_inserts(&mut lines, root, &mut warnings);
        }
        let sections = parse_sections(&lines);
        let goods = parse_goods(&lines);
        let types = parse_type_list(&lines, "[TYPES]");
        let used_types = parse_type_list(&lines, "[USEDTYPES]");
        let quest_indexes = parse_quest_indexes(&lines);
        Self {
            file_name: name,
            sections,
            goods,
            types,
            used_types,
            quest_indexes,
            warnings,
        }
    }

    /// 脚本文件名（不带目录与扩展名）。
    #[must_use]
    pub fn file_name(&self) -> &str {
        &self.file_name
    }

    /// 所有页面（文件里的顺序）。
    #[must_use]
    pub fn sections(&self) -> &[NpcPage] {
        &self.sections
    }

    /// 所有页面的 key（大写、带方括号）。
    pub fn section_keys(&self) -> impl Iterator<Item = &str> {
        self.sections.iter().map(|page| page.key.as_str())
    }

    /// 按 key 找页面。
    ///
    /// 比较时忽略大小写；带参数的按钮（`[@Guess(1)]`）会退回去掉括号部分再找一次
    /// （Crystal `NPCPage.ArgumentParse`，`NPCPage.cs:21-41`）。
    #[must_use]
    pub fn section(&self, key: &str) -> Option<&NpcPage> {
        let wanted = normalize_key(key);
        if let Some(page) = self.sections.iter().find(|page| page.key == wanted) {
            return Some(page);
        }
        let bare = strip_arguments(&wanted);
        self.sections.iter().find(|page| page.key == bare)
    }

    /// 商品表（`[TRADE]`）。
    #[must_use]
    pub fn goods(&self) -> &[GoodsItem] {
        &self.goods
    }

    /// `[TYPES]`。
    #[must_use]
    pub fn types(&self) -> &[i32] {
        &self.types
    }

    /// `[USEDTYPES]`。
    #[must_use]
    pub fn used_types(&self) -> &[i32] {
        &self.used_types
    }

    /// `[QUESTS]` 段的原始内容（带符号，正=发布、负=交付）。
    #[must_use]
    pub fn quest_indexes(&self) -> &[i32] {
        &self.quest_indexes
    }

    /// 这个 NPC 是否发布任务 `quest_index`（Crystal `[Quests]` 里的正数，`NPCScript.cs:711-712`）。
    #[must_use]
    pub fn publishes_quest(&self, quest_index: i32) -> bool {
        quest_index > 0 && self.quest_indexes.contains(&quest_index)
    }

    /// 这个 NPC 是否交付任务 `quest_index`（Crystal `[Quests]` 里的负数，`NPCScript.cs:713-714`）。
    #[must_use]
    pub fn finishes_quest(&self, quest_index: i32) -> bool {
        quest_index > 0 && self.quest_indexes.contains(&-quest_index)
    }

    /// 解析告警。
    #[must_use]
    pub fn warnings(&self) -> &[String] {
        &self.warnings
    }

    /// 脚本里所有出现过的条件关键字（大写，去重后按字典序）；测试与文档用。
    #[must_use]
    pub fn check_keywords(&self) -> Vec<String> {
        let mut out: Vec<String> = self
            .sections
            .iter()
            .flat_map(|page| &page.segments)
            .flat_map(|segment| &segment.checks)
            .map(|check| check.kind.keyword().to_owned())
            .collect();
        out.sort_unstable();
        out.dedup();
        out
    }
}

impl NpcCheckKind {
    /// 这个种类对应的**第一个**关键字（大写）。
    #[must_use]
    pub fn keyword(self) -> &'static str {
        match self {
            Self::Level => "LEVEL",
            Self::Gold => "CHECKGOLD",
            Self::PkPoint => "CHECKPKPOINT",
            Self::Item => "CHECKITEM",
            Self::Class => "CHECKCLASS",
            Self::Gender => "CHECKGENDER",
            Self::Quest => "CHECKQUEST",
            Self::Flag => "CHECK",
            Self::Random => "RANDOM",
            Self::Unsupported | Self::Unknown => "",
        }
    }
}

/// 把 key 规范成大写并补上方括号（`main` → `[@MAIN]`）。
#[must_use]
pub fn normalize_key(key: &str) -> String {
    let trimmed = key.trim();
    let upper = trimmed.to_uppercase();
    if upper.starts_with('[') {
        return upper;
    }
    let bare = upper.strip_prefix('@').unwrap_or(&upper);
    format!("[@{bare}]")
}

/// 去掉 key 里的参数部分：`[@GUESS(1)]` → `[@GUESS]`（Crystal `NPCPage.ArgumentParse`）。
#[must_use]
pub fn strip_arguments(key: &str) -> String {
    match (key.find('('), key.rfind(')')) {
        (Some(start), Some(end)) if end > start => format!("{}{}", &key[..start], &key[end + 1..]),
        _ => key.to_owned(),
    }
}

/// 取出 key 里的参数：`[@GUESS(1,2)]` → `["1", "2"]`（Crystal `NPCPage.Args`）。
#[must_use]
pub fn section_arguments(key: &str) -> Vec<String> {
    let (Some(start), Some(end)) = (key.find('('), key.rfind(')')) else {
        return Vec::new();
    };
    if end <= start + 1 {
        return Vec::new();
    }
    key[start + 1..end]
        .split(',')
        .map(|arg| arg.trim().to_owned())
        .collect()
}

/// 按 `NpcInfo.file_name` 拼出脚本真实路径。
///
/// 绝对路径原样返回；相对路径拼 `envir_root/NPCs/`，并补 `.txt`
/// （Crystal `Path.Combine(Settings.NPCPath, FileName + ".txt")`，`NPCScript.cs:142`）。
///
/// 数据库里的 `FileName` 两种分隔符都出现过（`A/B` 与 `A\B`），这里统一成 `/`：
/// Windows 上两种都能打开，但统一之后在 Unix 上也能正确解析。
#[must_use]
pub fn resolve_script_path(file_name: &Path, envir_root: &Path) -> PathBuf {
    if file_name.is_absolute() {
        return file_name.to_path_buf();
    }
    let normalized = file_name.to_string_lossy().replace('\\', "/");
    let mut path = envir_root.join("NPCs").join(normalized);
    if path.extension().is_none() {
        path.set_extension("txt");
    }
    path
}

/// 读文本行：按 `\n` 切、去掉行尾 `\r`、去掉 UTF-8 BOM。
///
/// Crystal 用 `File.ReadAllLines`（`NPCScript.cs:146`），它对 BOM 与 CRLF 的处理与此一致。
fn read_lines(path: &Path) -> Result<Vec<String>, NpcScriptError> {
    let bytes = std::fs::read(path).map_err(|source| NpcScriptError::Io {
        path: path.display().to_string(),
        source,
    })?;
    let text = String::from_utf8_lossy(&bytes);
    let text = text.strip_prefix('\u{feff}').unwrap_or(&text);
    Ok(text
        .split('\n')
        .map(|line| line.strip_suffix('\r').unwrap_or(line).to_owned())
        .collect())
}

/// `#INSERT [路径] …` 展开（Crystal `NPCScript.ParseInsert`，`NPCScript.cs:303-328`）。
///
/// 细节照抄：
/// * 路径取第二个空格分隔的字段，去掉首尾的 `[` `]`（`:311-315`）；
/// * **追加到行尾**（`:322` 的 `lines.AddRange`），不是就地插入；
/// * 展开完把所有 `#INSERT` 行删掉（`:325`）。
///
/// 简化：同一路径最多展开 8 次、总展开次数上限 [`MAX_INCLUDES`]（Crystal 自引用会撑爆内存）。
fn expand_inserts(lines: &mut Vec<String>, envir_root: &Path, warnings: &mut Vec<String>) {
    let mut index = 0;
    let mut expansions = 0;
    let mut per_file: std::collections::BTreeMap<String, usize> = std::collections::BTreeMap::new();
    while index < lines.len() {
        let is_insert = lines[index].to_uppercase().starts_with("#INSERT");
        if !is_insert {
            index += 1;
            continue;
        }
        let parts: Vec<&str> = lines[index].split(' ').collect();
        if parts.len() < 2 {
            index += 1;
            continue;
        }
        let target = parts[1];
        let inner = target
            .strip_prefix('[')
            .and_then(|rest| rest.strip_suffix(']'))
            .unwrap_or(target)
            .replace('\\', "/");
        let path = envir_root.join(&inner);
        let seen = per_file.entry(inner.clone()).or_insert(0);
        *seen += 1;
        if *seen > 8 || expansions >= MAX_INCLUDES {
            warnings.push(format!("#INSERT {inner} 展开次数超限，已跳过"));
            index += 1;
            continue;
        }
        match read_lines(&path) {
            Ok(mut inserted) => {
                expansions += 1;
                lines.append(&mut inserted);
            }
            Err(error) => warnings.push(format!("#INSERT {inner} 失败：{error}")),
        }
        index += 1;
    }
    lines.retain(|line| !line.to_uppercase().starts_with("#INSERT"));
}

/// `#INCLUDE [路径] @Section` 展开（Crystal `NPCScript.ParseInclude`，`NPCScript.cs:330-385`）。
///
/// 语义：去被引用的文件里找 `[@SECTION]` 行，然后取紧随其后的 `{` … `}` 之间的行，
/// 插到 `#INCLUDE` 行的**后面**（`:377`），最后删掉所有 `#INCLUDE` 行（`:382`）。
fn expand_includes(lines: &mut Vec<String>, envir_root: &Path, warnings: &mut Vec<String>) {
    let mut index = 0;
    let mut expansions = 0;
    while index < lines.len() {
        if !lines[index].to_uppercase().starts_with("#INCLUDE") {
            index += 1;
            continue;
        }
        let parts: Vec<&str> = lines[index].split(' ').filter(|p| !p.is_empty()).collect();
        if parts.len() < 3 || expansions >= MAX_INCLUDES {
            if parts.len() < 3 {
                warnings.push(format!("{} 缺参数，已忽略", lines[index].trim()));
            }
            index += 1;
            continue;
        }
        let target = parts[1]
            .strip_prefix('[')
            .and_then(|rest| rest.strip_suffix(']'))
            .unwrap_or(parts[1])
            .replace('\\', "/");
        let page = format!("[{}]", parts[2]).to_uppercase();
        let path = envir_root.join(&target);
        match read_lines(&path) {
            Ok(external) => {
                expansions += 1;
                if let Some(block) = extract_block(&external, &page) {
                    for (offset, line) in block.into_iter().enumerate() {
                        lines.insert(index + 1 + offset, line);
                    }
                } else {
                    warnings.push(format!("#INCLUDE {target} 里没有找到 {page} 的 {{…}} 块"));
                }
            }
            Err(error) => warnings.push(format!("#INCLUDE {target} 失败：{error}")),
        }
        index += 1;
    }
    lines.retain(|line| !line.to_uppercase().starts_with("#INCLUDE"));
}

/// 从外部文件里取 `[@Page]` 之后 `{` … `}` 之间的行（`NPCScript.cs:353-373`）。
fn extract_block(external: &[String], page: &str) -> Option<Vec<String>> {
    let start_index = external
        .iter()
        .position(|line| line.to_uppercase().starts_with(page))?;
    let mut out = Vec::new();
    let mut started = false;
    for line in &external[start_index + 1..] {
        let trimmed = line.trim();
        if trimmed == "{" {
            started = true;
            continue;
        }
        if trimmed == "}" {
            return started.then_some(out);
        }
        if started {
            out.push(line.clone());
        }
    }
    None
}

/// 把整份行切成页面，再把页面切成段落（Crystal `ParsePages` / `ParsePage`）。
///
/// 空行与 `;` 开头的注释行不进入页面的正文（`NPCScript.cs:419`、`:431`）。
fn parse_sections(lines: &[String]) -> Vec<NpcPage> {
    let mut pages = Vec::new();
    let mut current: Option<NpcPage> = None;
    let mut buffer: Vec<String> = Vec::new();

    for line in lines {
        if is_section_header(line) {
            if let Some(page) = current.take() {
                pages.push(finish_page(page, &buffer));
                buffer.clear();
            }
            let key = line.trim().to_uppercase();
            // 只有 `[@…]` 才是对话页；`[TYPES]` / `[TRADE]` / `[QUESTS]` 是数据段，
            // 交给 [`parse_type_list`] / [`parse_goods`]，不当作页面
            // （Crystal 也只把按钮能跳到的页放进 `NPCPages`，`NPCScript.cs:398-410`）。
            current = key.starts_with("[@").then_some(NpcPage {
                key,
                segments: Vec::new(),
                buttons: Vec::new(),
            });
            continue;
        }
        if current.is_some() {
            buffer.push(line.clone());
        }
    }
    if let Some(page) = current.take() {
        pages.push(finish_page(page, &buffer));
    }
    pages
}

/// 段落切分 + 段落解析。
fn finish_page(mut page: NpcPage, raw: &[String]) -> NpcPage {
    // Crystal 先把空行全删掉，再在 `#IF` 处切段（`NPCScript.cs:419`、`:454-458`）。
    let lines: Vec<&String> = raw
        .iter()
        .filter(|line| !line.is_empty() && !line.starts_with(';'))
        .collect();
    let mut segments = Vec::new();
    let mut current: Vec<String> = Vec::new();
    for line in lines {
        // Crystal 在每个 `#IF` 处切开段落（`NPCScript.cs:454-458` 的 `nextSection`）。
        if starts_with_directive(line, "IF") && !current.is_empty() {
            segments.push(parse_segment(&current));
            current.clear();
        }
        current.push(line.clone());
    }
    if !current.is_empty() {
        segments.push(parse_segment(&current));
    }
    page.buttons = segments
        .iter()
        .flat_map(|segment| {
            segment
                .buttons
                .iter()
                .chain(segment.else_buttons.iter())
                .chain(segment.goto_buttons.iter())
        })
        .cloned()
        .collect();
    page.segments = segments;
    page
}

/// 判断是不是页面头：`[...]`（Crystal `NPCScript.cs:449` / `:562`）。
fn is_section_header(line: &str) -> bool {
    let trimmed = line.trim();
    trimmed.starts_with('[') && trimmed.ends_with(']')
}

/// `#IF` / `#SAY` 一类的指令行。
///
/// Crystal 用 `lines[i].StartsWith("#")` **不做**前导空白裁剪（`NPCSegment.cs:532`）；
/// 这里保持同样的口径：行首有空格就不是指令，而是正文。
fn starts_with_directive(line: &str, name: &str) -> bool {
    let Some(rest) = line.strip_prefix('#') else {
        return false;
    };
    let rest = rest.trim_start();
    let upper = rest.to_uppercase();
    upper == name || upper.starts_with(&format!("{name} "))
}

/// 解析一个段落的行（Crystal `NPCSegment.ParseSegment`，`NPCScript.cs:509-620`）。
fn parse_segment(lines: &[String]) -> NpcSegment {
    #[derive(Clone, Copy, PartialEq, Eq)]
    enum Mode {
        Start,
        Checks,
        Say,
        Act,
        ElseSay,
        ElseAct,
        ElseIfChecks,
    }

    let mut segment = NpcSegment::default();
    let mut mode = Mode::Start;
    for line in lines {
        // `trimmed` 只用于「识别指令」，正文一律用**原始行**：Crystal 的 `#SAY` 正文
        // 只做 `TrimEnd()`（`NPCSegment.cs:600`），行首空格是有意义的排版。
        let trimmed = line.trim();
        if line.starts_with('#') {
            let directive = trimmed
                .trim_start_matches('#')
                .split_whitespace()
                .next()
                .unwrap_or_default()
                .to_uppercase();
            mode = match directive.as_str() {
                "IF" => Mode::Checks,
                "SAY" => Mode::Say,
                "ACT" => Mode::Act,
                "ELSESAY" => Mode::ElseSay,
                "ELSEACT" => Mode::ElseAct,
                // Crystal 会在这里抛 `NotImplementedException`（`:557-558`），见 [`NpcSegment::else_checks`]。
                "ELSEIF" => Mode::ElseIfChecks,
                _ => mode,
            };
            continue;
        }
        match mode {
            Mode::Checks => segment.checks.push(parse_check(line)),
            Mode::ElseIfChecks => segment.else_checks.push(line.clone()),
            Mode::Act => {
                // `#ACT` 里的按钮进 goto 表（`NPCScript.cs:545-548` 的 `currentButtons = gotoButtons`）。
                collect_buttons(line, &mut segment.goto_buttons);
                collect_directive_buttons(line, &mut segment.buttons, &mut segment.goto_buttons);
                segment.actions.push(parse_action(line));
            }
            Mode::ElseAct => {
                collect_buttons(line, &mut segment.goto_buttons);
                collect_directive_buttons(line, &mut segment.buttons, &mut segment.goto_buttons);
                segment.else_actions.push(parse_action(line));
            }
            // 页头后面直接写正文（没写 `#SAY`）与 `#SAY` 走同一条路径：
            // Crystal 里 `currentSay`/`currentButtons` 的初值就是 SAY 表（`NPCScript.cs:522`）。
            Mode::Say | Mode::Start => {
                collect_buttons(line, &mut segment.buttons);
                collect_directive_buttons(line, &mut segment.buttons, &mut segment.goto_buttons);
                segment.say.push(line.trim_end().to_owned());
            }
            Mode::ElseSay => {
                collect_buttons(line, &mut segment.else_buttons);
                segment.else_say.push(line.trim_end().to_owned());
            }
        }
    }
    segment
}

/// 提取一行里的 `<文本/@goto>` 按钮（Crystal 正则 `<.*?/(\@.*?)>`，`NPCScript.cs:524`）。
///
/// 语义照抄：按钮 key 是 `<` 之后第一个 `/@…` 到 `>` 之间的内容，遇到 `'/'` 截断
/// （`:570-573` 的 `argu.Split('/')[0]`）。
fn collect_buttons(line: &str, out: &mut Vec<String>) {
    let bytes = line.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] != b'<' {
            index += 1;
            continue;
        }
        let rest = &line[index + 1..];
        // 找 `/@`：这两个字符一定在同一个 ASCII 边界上，用字节查找即可。
        let Some(slash) = rest.find("/@") else {
            break;
        };
        // `after` 指向 `@`（Crystal 的捕获组是 `\@.*?`，**包含** `@`）。
        let after = index + 1 + slash + 1;
        let Some(close_offset) = line[after..].find('>') else {
            break;
        };
        let inner = &line[after..after + close_offset];
        let key = inner.split('/').next().unwrap_or(inner).trim();
        if !key.is_empty() {
            out.push(format!("[{}]", key.to_uppercase()));
        }
        index = after + close_offset;
    }
}

/// `#ACT` 里的 `GOTO` / `GROUPGOTO` / `DELAYGOTO` / `ROLLDIE` 也登记成按钮
/// （Crystal `NPCScript.cs:577-597`）。
fn collect_directive_buttons(
    line: &str,
    buttons: &mut Vec<String>,
    goto_buttons: &mut Vec<String>,
) {
    let parts: Vec<&str> = line.split_whitespace().collect();
    if parts.len() < 2 {
        return;
    }
    let keyword = parts[0].to_uppercase();
    let mut target = None;
    match keyword.as_str() {
        "GOTO" | "GROUPGOTO" => target = Some(parts[1]),
        "TIMERECALL" | "DELAYGOTO" | "TIMERECALLGROUP" => {
            if parts.len() > 2 {
                target = Some(parts[2]);
            }
        }
        "ROLLDIE" | "ROLLYUT" => {
            buttons.push(format!("[{}]", parts[1].to_uppercase()));
        }
        _ => {}
    }
    if let Some(target) = target {
        goto_buttons.push(fallback_key(target));
    }
}

/// `@main` / `[@main]` 两种写法都归一成 `[@MAIN]`。
fn fallback_key(raw: &str) -> String {
    normalize_key(raw)
}

/// 解析 `#IF` 下的一行条件（Crystal `NPCSegment.ParseCheck`，`NPCSegment.cs:102-435`）。
fn parse_check(line: &str) -> NpcCheck {
    let parts: Vec<&str> = line.split_whitespace().collect();
    let keyword = parts.first().map(|p| p.to_uppercase()).unwrap_or_default();
    let mut params: Vec<String> = parts.iter().skip(1).map(|p| (*p).to_owned()).collect();
    let source = line.to_owned();
    let kind = match keyword.as_str() {
        // Crystal 的关键字是 `LEVEL`（`NPCSegment.cs:119`）；`CHECKLEVEL` 是对外文档里的常见写法，
        // 一并接受（等价于 `LEVEL`）。
        "LEVEL" | "CHECKLEVEL" => NpcCheckKind::Level,
        "CHECKGOLD" => NpcCheckKind::Gold,
        "CHECKPKPOINT" => NpcCheckKind::PkPoint,
        "CHECKITEM" => NpcCheckKind::Item,
        "CHECKCLASS" => NpcCheckKind::Class,
        "CHECKGENDER" => NpcCheckKind::Gender,
        "CHECKQUEST" => NpcCheckKind::Quest,
        "CHECK" => {
            // `CHECK [539] 1` 的方括号只是脚本写法，Crystal 会把下标抠出来
            // （`NPCSegment.cs:247-252`），这里也抠掉。
            if let Some(first) = params.first_mut() {
                if let Some(inner) = first
                    .strip_prefix('[')
                    .and_then(|rest| rest.strip_suffix(']'))
                {
                    *first = inner.to_owned();
                }
            }
            NpcCheckKind::Flag
        }
        "RANDOM" => NpcCheckKind::Random,
        "" => NpcCheckKind::Unknown,
        _ => NpcCheckKind::Unsupported,
    };
    NpcCheck {
        kind,
        params,
        source,
    }
}

/// 解析 `#ACT` 下的一行动作（Crystal `NPCSegment.ParseAct`，`NPCSegment.cs:436-1228`）。
fn parse_action(line: &str) -> NpcAction {
    let parts: Vec<&str> = line.split_whitespace().collect();
    let keyword = parts.first().map(|p| p.to_uppercase()).unwrap_or_default();
    let mut params: Vec<String> = parts.iter().skip(1).map(|p| (*p).to_owned()).collect();
    let kind = match keyword.as_str() {
        "GOTO" => NpcActionKind::Goto,
        "BREAK" => NpcActionKind::Break,
        "GIVEGOLD" => NpcActionKind::GiveGold,
        "TAKEGOLD" => NpcActionKind::TakeGold,
        "SET" => {
            // 与 `CHECK [...]` 同样的方括号处理（Crystal `NPCSegment.cs:798-803`）。
            if let Some(first) = params.first_mut() {
                if let Some(inner) = first
                    .strip_prefix('[')
                    .and_then(|rest| rest.strip_suffix(']'))
                {
                    *first = inner.to_owned();
                }
            }
            NpcActionKind::SetFlag
        }
        // 本作扩展（Crystal `NPCActions` 里没有这三个，见 [`NpcActionKind::AcceptQuest`]）。
        "ACCEPTQUEST" => NpcActionKind::AcceptQuest,
        "FINISHQUEST" => NpcActionKind::FinishQuest,
        "ABANDONQUEST" => NpcActionKind::AbandonQuest,
        "" => NpcActionKind::Unknown,
        _ => NpcActionKind::Unsupported,
    };
    NpcAction {
        kind,
        params,
        source: line.to_owned(),
    }
}

/// 解析 `[TRADE]` 段（Crystal `NPCScript.ParseGoods`，`NPCScript.cs:650-684`）。
fn parse_goods(lines: &[String]) -> Vec<GoodsItem> {
    let mut goods = Vec::new();
    let mut index = 0;
    while index < lines.len() {
        if !lines[index].to_uppercase().starts_with("[TRADE]") {
            index += 1;
            continue;
        }
        let mut cursor = index + 1;
        while cursor < lines.len() {
            let line = &lines[cursor];
            if line.starts_with('[') {
                return goods;
            }
            if line.is_empty() {
                cursor += 1;
                continue;
            }
            let data: Vec<&str> = line.split_whitespace().collect();
            if let Some(name) = data.first() {
                let count = if data.len() == 2 {
                    data[1].parse::<u16>().unwrap_or(0)
                } else {
                    1
                };
                goods.push(GoodsItem {
                    unique_id: cursor as u64,
                    name: (*name).to_owned(),
                    count: if count == 0 { 1 } else { count },
                });
            }
            cursor += 1;
        }
        index = cursor;
    }
    goods
}

/// 解析一个「每行一个整数」的类型列表段（Crystal `NPCScript.ParseTypes`，`:622-649`）。
fn parse_type_list(lines: &[String], header: &str) -> Vec<i32> {
    let mut out = Vec::new();
    for (index, line) in lines.iter().enumerate() {
        if !line.to_uppercase().starts_with(header) {
            continue;
        }
        for item in &lines[index + 1..] {
            if item.is_empty() {
                continue;
            }
            let Ok(value) = item.trim().parse::<i32>() else {
                break;
            };
            out.push(value);
        }
    }
    out
}

/// 解析 `[QUESTS]` 段（Crystal `NPCScript.ParseQuests`，`NPCScript.cs:685-721`）。
///
/// ```csharp
/// if (!lines[i].ToUpper().StartsWith(QuestKey)) continue;
/// while (++i < lines.Count) {
///     if (lines[i].StartsWith("[")) return;
///     if (String.IsNullOrEmpty(lines[i])) continue;
///     int.TryParse(lines[i], out int index);          // 失败 → 0
///     if (index == 0) continue;                        // 0 直接跳过
///     ...
/// }
/// ```
///
/// 语义照抄：非数字行（样本里 `BichonProvince/BorderVillage/Pedlar.txt` 的
/// `[Quests]` 段里混进了一个 `<Close/@exit>` 按钮）被解析成 0 后**静默跳过**——
/// Crystal 连日志都不记，而 `crates/mir-db/tests/npc_scripts.rs` 的
/// 「真实样本零告警」断言也要求这里保持安静。正负号决定「发布 / 交付」，
/// 见 [`NpcScript::quest_indexes`]。
fn parse_quest_indexes(lines: &[String]) -> Vec<i32> {
    let mut out = Vec::new();
    let mut index = 0;
    while index < lines.len() {
        if !lines[index].to_uppercase().starts_with("[QUESTS]") {
            index += 1;
            continue;
        }
        let mut cursor = index + 1;
        while cursor < lines.len() {
            let line = &lines[cursor];
            if line.starts_with('[') {
                break;
            }
            let trimmed = line.trim();
            if !trimmed.is_empty() {
                if let Ok(value) = trimmed.parse::<i32>() {
                    if value != 0 {
                        out.push(value);
                    }
                }
            }
            cursor += 1;
        }
        index = cursor;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn script(text: &str) -> NpcScript {
        NpcScript::parse("Test", text.split('\n').map(str::to_owned).collect())
    }

    #[test]
    fn parses_pages_and_plain_say() {
        let npc = script(
            "[@Main]\n\
             Hello, how may I help you?\n\
             <View/@BuySell> Store.\n\
             <Close/@exit>\n",
        );
        assert_eq!(npc.sections().len(), 1);
        let page = npc.section("main").expect("页存在");
        assert_eq!(page.key, "[@MAIN]");
        assert_eq!(page.segments.len(), 1);
        assert_eq!(page.segments[0].say.len(), 3);
        assert_eq!(
            page.buttons,
            vec!["[@BUYSELL]".to_owned(), "[@EXIT]".to_owned()]
        );
    }

    #[test]
    fn splits_segments_at_if_and_routes_else_branches() {
        let npc = script(
            "[@MAIN]\n\
             #IF\n\
             CHECKPKPOINT > 199\n\
             #SAY\n\
             I will not help you.\n\
             <Close/@exit>\n\
             #ELSEACT\n\
             GOTO @Main-1\n",
        );
        let page = npc.section("@main").expect("页存在");
        assert_eq!(page.segments.len(), 1);
        let segment = &page.segments[0];
        assert_eq!(segment.checks.len(), 1);
        assert_eq!(segment.checks[0].kind, NpcCheckKind::PkPoint);
        assert_eq!(segment.checks[0].params, vec![">", "199"]);
        assert_eq!(segment.say, vec!["I will not help you.", "<Close/@exit>"]);
        assert_eq!(segment.else_actions.len(), 1);
        assert_eq!(segment.else_actions[0].kind, NpcActionKind::Goto);
        assert_eq!(segment.else_actions[0].params, vec!["@Main-1"]);
        assert_eq!(segment.goto_buttons, vec!["[@MAIN-1]"]);
        assert!(segment.else_say.is_empty());
    }

    #[test]
    fn each_if_starts_a_new_segment() {
        let npc = script(
            "[@MAIN]\n\
             #IF\n\
             CHECK [1] 1\n\
             #SAY\n\
             one\n\
             #IF\n\
             CHECKGOLD > 10\n\
             #SAY\n\
             two\n\
             #ELSEACT\n\
             GOTO @End\n",
        );
        let page = npc.section("main").expect("页存在");
        assert_eq!(page.segments.len(), 2);
        assert_eq!(page.segments[0].say, vec!["one"]);
        assert_eq!(page.segments[0].checks[0].kind, NpcCheckKind::Flag);
        assert_eq!(page.segments[1].say, vec!["two"]);
        assert_eq!(page.segments[1].checks[0].kind, NpcCheckKind::Gold);
    }

    #[test]
    fn else_say_is_separated_from_say() {
        let npc = script(
            "[@MAIN]\n\
             #IF\n\
             CHECKQUEST 155 1\n\
             #SAY\n\
             proven\n\
             #ELSESAY\n\
             not yet\n\
             <Back/@main>\n",
        );
        let segment = &npc.section("main").unwrap().segments[0];
        assert_eq!(segment.say, vec!["proven"]);
        assert_eq!(segment.else_say, vec!["not yet", "<Back/@main>"]);
        assert_eq!(segment.else_buttons, vec!["[@MAIN]"]);
        assert!(segment.buttons.is_empty());
        assert_eq!(segment.checks[0].kind, NpcCheckKind::Quest);
        assert_eq!(segment.checks[0].params, vec!["155", "1"]);
    }

    #[test]
    fn color_markup_and_placeholders_are_kept_verbatim() {
        let npc = script("[@MAIN]\nHello {<$USERNAME>/KHAKI}, welcome <$LEVEL>.\n");
        assert_eq!(
            npc.section("main").unwrap().segments[0].say,
            vec!["Hello {<$USERNAME>/KHAKI}, welcome <$LEVEL>."]
        );
    }

    #[test]
    fn buttons_accept_arguments_and_bracket_labels() {
        let npc = script("[@MAIN]\n<[ 1 ]/@Guess(1)>\n<2/@Guess(2)>\n");
        let page = npc.section("main").unwrap();
        assert_eq!(
            page.buttons,
            vec!["[@GUESS(1)]".to_owned(), "[@GUESS(2)]".to_owned()]
        );
        // 带参数的按钮可以退回裸 key 找到同名的页（Crystal `NPCPage.ArgumentParse`）。
        let npc = script("[@Guess]\ntext\n[@MAIN]\n<1/@Guess(1)>\n");
        assert!(npc.section("[@Guess(1)]").is_some());
    }

    #[test]
    fn trade_section_lists_goods_with_line_indexes() {
        let npc = script(
            "[@MAIN]\n\
             hi\n\
             [Types]\n\
             1\n\
             14\n\
             [Trade]\n\
             WoodenSword\n\
             (HP)DrugSmall 5\n",
        );
        assert_eq!(npc.types(), &[1, 14]);
        assert_eq!(
            npc.goods(),
            &[
                GoodsItem {
                    unique_id: 6,
                    name: "WoodenSword".to_owned(),
                    count: 1
                },
                GoodsItem {
                    unique_id: 7,
                    name: "(HP)DrugSmall".to_owned(),
                    count: 5
                },
            ]
        );
    }

    #[test]
    fn quest_section_records_signed_indexes() {
        // `NPCScript.ParseQuests`（`NPCScript.cs:685-721`）：正数=发布、负数=交付。
        let npc = script(
            "[@MAIN]\n\
             hi\n\
             [Quests]\n\
             -154\n\
             155\n\
             -155\n\
             <Close/@exit>\n\
             [Trade]\n\
             WoodenSword\n",
        );
        assert_eq!(npc.quest_indexes(), &[-154, 155, -155]);
        assert!(npc.publishes_quest(155));
        assert!(npc.finishes_quest(154));
        assert!(npc.finishes_quest(155));
        assert!(!npc.publishes_quest(154));
        // 样本里的 `Pedlar.txt` 就是这样：`[Quests]` 段里混进一个按钮，Crystal 静默跳过。
        assert!(npc.warnings().is_empty(), "{:?}", npc.warnings());
        // `[Trade]` 段不受影响。
        assert_eq!(npc.goods().len(), 1);
    }

    #[test]
    fn quest_actions_are_recognised_as_extensions() {
        let npc = script(
            "[@MAIN]\n\
             #ACT\n\
             ACCEPTQUEST 155\n\
             FINISHQUEST 155\n\
             ABANDONQUEST 154\n",
        );
        let actions = &npc.section("main").unwrap().segments[0].actions;
        assert_eq!(actions[0].kind, NpcActionKind::AcceptQuest);
        assert_eq!(actions[0].params, vec!["155"]);
        assert_eq!(actions[1].kind, NpcActionKind::FinishQuest);
        assert_eq!(actions[2].kind, NpcActionKind::AbandonQuest);
    }

    #[test]
    fn unsupported_checks_are_recorded_not_dropped() {
        let npc = script(
            "[@MAIN]\n\
             #IF\n\
             CHECKMAP 0\n\
             CHECKNAMELIST foo.txt\n\
             #ACT\n\
             GIVEITEM WoodenSword\n",
        );
        let segment = &npc.section("main").unwrap().segments[0];
        assert_eq!(segment.checks.len(), 2);
        assert_eq!(segment.checks[0].kind, NpcCheckKind::Unsupported);
        assert_eq!(segment.checks[0].params, vec!["0"]);
        assert_eq!(segment.actions[0].kind, NpcActionKind::Unsupported);
        assert_eq!(npc.warnings().len(), 0, "不认识的关键字不算告警");
    }

    #[test]
    fn else_if_is_accepted_but_ignored() {
        // Crystal 会抛 `NotImplementedException`（`NPCSegment.cs:557-558`）。
        let npc = script("[@MAIN]\n#IF\nCHECK [1] 1\n#ELSEIF\nCHECK [2] 1\n#SAY\nhi\n");
        let segment = &npc.section("main").unwrap().segments[0];
        assert_eq!(segment.checks.len(), 1);
        assert_eq!(segment.else_checks, vec!["CHECK [2] 1"]);
        assert_eq!(segment.say, vec!["hi"]);
    }

    #[test]
    fn compare_operators_follow_crystal() {
        assert!(CompareOp::parse("<").unwrap().compare(&1, &2));
        assert!(CompareOp::parse(">=").unwrap().compare(&2, &2));
        assert!(CompareOp::parse("!=").unwrap().compare(&2, &3));
        assert!(!CompareOp::parse("==").unwrap().compare(&2, &3));
        assert!(CompareOp::parse("<>").is_none());
    }

    #[test]
    fn normalize_key_accepts_all_common_spellings() {
        assert_eq!(normalize_key("@main"), "[@MAIN]");
        assert_eq!(normalize_key("[@Main]"), "[@MAIN]");
        assert_eq!(normalize_key(" main "), "[@MAIN]");
        assert_eq!(strip_arguments("[@GUESS(1,2)]"), "[@GUESS]");
        assert_eq!(section_arguments("[@GUESS(1,2)]"), vec!["1", "2"]);
    }

    #[test]
    fn comments_and_empty_lines_do_not_reach_the_page() {
        let npc = script("[@MAIN]\n;\nreal text\n#ACT\n; 动作里的注释\nGOTO @Main\n");
        let segment = &npc.section("main").unwrap().segments[0];
        assert_eq!(segment.say, vec!["real text"]);
        assert_eq!(segment.actions.len(), 1);
        assert_eq!(segment.actions[0].kind, NpcActionKind::Goto);
    }

    // ------------------------------------------------------------------
    // 需要文件系统的部分：`#INSERT` / `#INCLUDE`
    // ------------------------------------------------------------------

    fn temp_envir(tag: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("mir-next-npc-script-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("NPCs")).expect("建目录");
        std::fs::create_dir_all(dir.join("SystemScripts")).expect("建目录");
        dir
    }

    #[test]
    fn insert_appends_lines_from_another_file() {
        let root = temp_envir("insert");
        std::fs::create_dir_all(root.join("NPCs/Sub")).expect("建目录");
        std::fs::write(
            root.join("NPCs/Sub/Extra.txt"),
            "[@Extra]\n#SAY\n来自被插入的文件\n",
        )
        .expect("写被插入脚本");
        let main = root.join("NPCs/Main.txt");
        std::fs::write(
            &main,
            "#INSERT [NPCs/Sub/Extra.txt] @Main\n[@Main]\n#SAY\n主脚本\n",
        )
        .expect("写主脚本");

        let npc = NpcScript::load("Main", &root).expect("载入成功");
        // `#INSERT` 的内容追加到文件尾（Crystal `NPCScript.cs:322`），所以它是一个独立页面。
        let main_page = npc.section("main").expect("主页存在");
        assert_eq!(main_page.segments[0].say, vec!["主脚本"]);
        let extra = npc.section("extra").expect("被插入的页存在");
        assert_eq!(extra.segments[0].say, vec!["来自被插入的文件"]);
        assert!(npc.warnings().is_empty(), "{:?}", npc.warnings());
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn include_copies_curly_block() {
        let root = temp_envir("include");
        std::fs::write(
            root.join("SystemScripts/Tavern.txt"),
            "[@Tavern]\n{\n#SAY\n欢迎光临\n}\n",
        )
        .expect("写共享脚本");
        let main = root.join("NPCs/Inn.txt");
        std::fs::write(
            &main,
            "[@MAIN]\n#INCLUDE [SystemScripts/Tavern.txt] @Tavern\n",
        )
        .expect("写主脚本");

        let npc = NpcScript::load(main.to_str().expect("路径"), &root).expect("载入成功");
        let page = npc.section("main").expect("页存在");
        let text: Vec<&str> = page.segments[0].say.iter().map(String::as_str).collect();
        assert_eq!(text, vec!["欢迎光临"]);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn missing_script_is_an_io_error() {
        let root = temp_envir("missing");
        let error = NpcScript::load("nope/NotThere", &root).expect_err("应该报错");
        assert!(matches!(error, NpcScriptError::Io { .. }));
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn missing_insert_only_warns() {
        let root = temp_envir("missing-insert");
        let main = root.join("NPCs/Main.txt");
        std::fs::write(
            &main,
            "#INSERT [SystemScripts/Nope.txt] @Main\n[@Main]\nhi\n",
        )
        .expect("写主脚本");
        let npc = NpcScript::load("Main", &root).expect("载入成功");
        assert_eq!(npc.section("main").unwrap().segments[0].say, vec!["hi"]);
        assert_eq!(npc.warnings().len(), 1);
        let _ = std::fs::remove_dir_all(root);
    }
}
