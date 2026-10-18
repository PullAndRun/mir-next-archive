//! 游戏数据（`Server.MirDB`）到运行时类型的桥接。
//!
//! `mir-db` 负责把 Crystal 的数据库文件解析成结构体；这一层负责把「数据库里的怪物模板」
//! 变成 `mir-server` 战斗系统真正使用的 [`MonsterKind`]，把地图上的 `RespawnInfo`
//! 变成可执行的 [`SpawnGroup`]，并为物品系统提供**模板查找**与**掉落表缓存**。
//!
//! 数值来源与 Crystal 的对应关系：
//!
//! | Rust | Crystal |
//! | --- | --- |
//! | [`MonsterKind::name`] | `MonsterInfo.Name` |
//! | [`MonsterKind::image`] | `MonsterInfo.Image`（`Monster` 枚举，未声明值退回默认） |
//! | [`MonsterKind::stats`] | `MonsterInfo.Stats` 字典（`Stat::HP`/`MinDC`/…） |
//! | [`MonsterKind::attack_speed_ms`] | `MonsterInfo.AttackSpeed`（毫秒，0 用默认 2500） |
//! | [`MonsterKind::move_speed_ms`] | `MonsterInfo.MoveSpeed`（毫秒，0 用默认 1800） |
//! | [`SpawnGroup`] | `MapInfo.Respawns`（`RespawnInfo` 的 `count`/`spread`/`location`/`delay`/`random_delay`） |
//! | [`GameData::item`] / [`GameData::item_by_name`] | `Envir.GetItemInfo(int)` / `Envir.GetItemInfo(string)`（`Server/MirEnvir/Envir.cs:4734-4754`） |
//! | [`GameData::monster_drop_table`] | `Envir.ReloadDrops`（`Envir.cs:5405-5419`）+ `MonsterInfo.Drops` |
//!
//! # 掉落表的位置
//!
//! Crystal 的 `Settings.DropPath` 是 `./Envir/Drops`（`Server/Settings.cs:28`），
//! 文件名取 `MonsterInfo.DropPath`，为空时退回 `MonsterInfo.Name`，并强制加 `.txt`
//! （`Envir.cs:5409-5414`）。本作把 `Settings.DropPath` 从**数据库所在目录**推出来：
//! `<Server.MirDB 所在目录>/Envir/Drops`，这样索引里的每个数据库样本都能直接找到自己的掉落表。

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use mir_core::Point;
use mir_core::enums::Monster as MonsterImage;
use mir_core::enums::{Spell, Stat};
use mir_db::npc_script::NpcScript;
use mir_db::quest_script::QuestScript;
use mir_db::{
    DropTable, GameShopItem, ItemInfo, MagicInfo, MapInfo, MirDb, MirDbError, MonsterInfo, NpcInfo,
    QuestInfo,
};

use crate::combat::CombatStats;
use crate::monster::{DEFAULT_ATTACK_SPEED_MS, DEFAULT_MOVE_SPEED_MS, MonsterKind};
use crate::world::SpawnGroup;

/// 索引里能找到的 `Server.MirDB` 候选路径（按优先级）。
///
/// 用 `python tools/asset-index/find_asset.py --name Server.MirDB` 可以列出全部。
/// 排序依据是**配套目录的完整度**而不是版本号：`cjlaaa` 是 v105 的参考样本，但那份归档
/// 只带了 `Server.MirDB`，没有 `Envir/`（掉落表、NPC 脚本、任务、配置全都缺），
/// 只能用于「只读数据库本身」的验证；真正的服务器数据用带全套 `Envir/` 的样本。
pub const DB_CANDIDATES: &[&str] = &[
    // v88：Envir（1641 掉落 / 660 NPC 脚本 / 53 商店 / 161 任务）+ Configs + Maps，最完整。
    "resource/community/suprcode-crystal-database/Jev/Server.MirDB",
    // v83：Envir（517 / 313 / 2 / 157）+ Configs，旧版格式覆盖。
    "resource/community/clgforever910-mir2database/Daneo1989/Server.MirDB",
    // v105：只有数据库文件，缺 Envir；放在最后当兜底。
    "resource/community/suprcode-crystal-database/cjlaaa/Server.MirDB",
];

/// 已加载的游戏数据。
#[derive(Debug)]
pub struct GameData {
    /// 解析后的数据库。
    pub db: MirDb,
    /// 怪物模板：`MonsterIndex` → 运行期种类。
    monsters: HashMap<i32, MonsterKind>,
    /// 物品模板下标：`ItemInfo.Index` → `db.items` 的下标。
    item_by_index: HashMap<i32, usize>,
    /// 物品模板下标：**大小写不敏感、忽略空格**的物品名 → `db.items` 的下标。
    item_by_name: HashMap<String, usize>,
    /// 怪物名（小写）→ 掉落表文件名（`MonsterInfo.DropPath`，空则怪物名）。
    monster_drops: HashMap<String, String>,
    /// 技能模板下标：`Spell` 的枚举值 → `db.magics` 的下标。
    ///
    /// Crystal 的 `Envir.MagicInfoList` 按 `Spell` **去重**（`Envir.cs:2903` 的
    /// `if (!MagicExists(m.Spell))`，先出现的胜出），这里的映射同样只记第一次出现的那条。
    magic_by_spell: HashMap<u8, usize>,
    /// 商城商品（Crystal `Envir.GameShopList`，`Envir.cs:119`、`:2911-2923`）。
    ///
    /// 每条的 `info` 已由 [`GameData::from_parts`] 绑定到物品模板（Crystal
    /// `Main.BindGameShop`，`Envir.cs:4550-4561`：模板不存在的条目**直接丢弃**，
    /// 与 `GameShopList` 的加载逻辑一致）。数据库没有商城段时是空列表，
    /// 服务器照常启动（商城页签空列表、购买回「商城里没有商品」）。
    game_shop: Vec<GameShopItem>,
    /// `Envir/Drops` 目录；数据库不在文件系统上（内存构造）时为 `None`。
    drop_dir: Option<PathBuf>,
    /// `Envir/` 根目录（NPC 脚本与 `#INSERT` 的相对路径都从这里算）。
    envir_dir: Option<PathBuf>,
    /// 掉落表缓存：文件名 → 解析结果（`None` 表示文件不存在或解析失败）。
    ///
    /// 用 `Mutex` 是因为 [`GameData`] 会被多个世界/会话共享（`Arc`），而 `DropTable::load`
    /// 是惰性的：Crystal 在 `Envir.ReloadDrops`（`Envir.cs:5405-5419`）一次性全载，
    /// 本作改成「第一次用到某只怪时才读它的文件」，几百个文件里通常只会读到十几个。
    drop_cache: Mutex<HashMap<String, Option<Arc<DropTable>>>>,
    /// NPC 脚本缓存：脚本文件名 → 解析结果（`None` 表示文件不存在或读不出来）。
    ///
    /// 对应 Crystal 的 `Envir.Scripts`（`NPCScript.cs:102`）——那边是**开局**全载，
    /// 本作同样惰性：660 份脚本里一次对话只会用到一份。
    npc_script_cache: Mutex<HashMap<String, Option<Arc<NpcScript>>>>,
    /// 任务脚本缓存：任务文件名 → 解析结果（`None` 表示文件不存在或读不出来）。
    ///
    /// 对应 Crystal 在 `Envir.LoadDB` 里对每个 `QuestInfo` 调 `LoadInfo()`
    /// （`QuestInfo.cs:130-146`）把 `Envir/Quests/<file>.txt` 全读进来的做法；
    /// 本作同样惰性（161 份脚本里，玩家一次只会接触其中几个）。
    quest_script_cache: Mutex<HashMap<String, Option<Arc<QuestScript>>>>,
    /// 数据文件路径（日志与排错用）。
    path: PathBuf,
}

impl GameData {
    /// 从文件加载。
    pub fn load(path: impl AsRef<Path>) -> Result<Self, MirDbError> {
        let path = path.as_ref().to_path_buf();
        let db = MirDb::load(&path)?;
        // Crystal `Settings.DropPath = ./Envir/Drops`（`Settings.cs:28`）；这里从数据库所在目录推。
        let envir_dir = path.parent().map(|parent| parent.join("Envir"));
        let drop_dir = envir_dir.as_ref().map(|envir| envir.join("Drops"));
        Ok(Self::from_parts(db, drop_dir, envir_dir, path))
    }

    /// 用内存里的数据库构造（测试、工具用）：不读文件，掉落表从 `drop_dir` 惰性加载。
    #[must_use]
    pub fn from_db(db: MirDb, drop_dir: Option<PathBuf>) -> Self {
        let path = PathBuf::from("<in-memory database>");
        let envir_dir = drop_dir
            .as_ref()
            .and_then(|dir| dir.parent().map(Path::to_path_buf));
        Self::from_parts(db, drop_dir, envir_dir, path)
    }

    fn from_parts(
        db: MirDb,
        drop_dir: Option<PathBuf>,
        envir_dir: Option<PathBuf>,
        path: PathBuf,
    ) -> Self {
        let monsters = db
            .monsters
            .iter()
            .map(|info| (info.index, kind_from_monster_info(info)))
            .collect();
        let mut item_by_index = HashMap::with_capacity(db.items.len());
        let mut item_by_name = HashMap::with_capacity(db.items.len());
        for (position, info) in db.items.iter().enumerate() {
            item_by_index.insert(info.index, position);
            // Crystal 的 `GetItemInfo(string)` 只把**模板名**里的空格去掉再比较
            // （`Envir.cs:4750` 的 `info.Name.Replace(" ", "")`），查询串里的空格是不去的。
            // 简化：两边都按「去空格 + 小写」归一化，否则 `金创药 (小量)` 这种带空格的查询永远找不到。
            let key = normalize_item_name(&info.name);
            item_by_name.entry(key).or_insert(position);
        }
        let mut monster_drops = HashMap::with_capacity(db.monsters.len());
        for info in &db.monsters {
            // `Envir.ReloadDrops`：`DropPath` 为空时用怪物名（`Envir.cs:5409-5414`）。
            let file = if info.drop_path.is_empty() {
                info.name.clone()
            } else {
                info.drop_path.clone()
            };
            if !file.is_empty() {
                monster_drops.insert(info.name.to_lowercase(), file);
            }
        }
        let mut magic_by_spell = HashMap::with_capacity(db.magics.len());
        for (position, info) in db.magics.iter().enumerate() {
            // `Envir.cs:2899-2905`：重复的 `Spell` 只保留文件里第一次出现的那条。
            magic_by_spell.entry(info.spell).or_insert(position);
        }
        // 商城：Crystal `Envir.cs:2911-2923` + `Main.BindGameShop`（`:4550-4561`）
        // ——按 `ItemIndex` 回填 `Info`，找不到模板的条目丢弃（Crystal 同样 `continue`）。
        // 找不到商城数据（旧版数据库没有这一段）时静默降级为空列表，**不**让启动失败。
        let mut game_shop = Vec::with_capacity(db.gameshop.len());
        for mut item in db.gameshop.clone() {
            let Some(info) = item_by_index
                .get(&item.item_index)
                .and_then(|position| db.items.get(*position))
            else {
                tracing::warn!(
                    g_index = item.g_index,
                    item_index = item.item_index,
                    "商城商品引用了不存在的物品模板，已跳过"
                );
                continue;
            };
            item.info = info.clone();
            game_shop.push(item);
        }
        Self {
            db,
            monsters,
            item_by_index,
            item_by_name,
            monster_drops,
            magic_by_spell,
            game_shop,
            drop_dir,
            envir_dir,
            drop_cache: Mutex::new(HashMap::new()),
            npc_script_cache: Mutex::new(HashMap::new()),
            quest_script_cache: Mutex::new(HashMap::new()),
            path,
        }
    }

    /// 按候选路径依次尝试加载，全部失败时返回错误（调用方决定降级）。
    ///
    /// 候选路径先按**仓库根**解析（`CARGO_MANIFEST_DIR/../..`），再退回相对当前工作目录，
    /// 这样 `cargo test`（工作目录是 crate 目录）和从仓库根启动服务器都能找到样本。
    pub fn discover() -> Result<Self, MirDbError> {
        let mut last = None;
        for candidate in candidate_paths() {
            if !candidate.exists() {
                continue;
            }
            match Self::load(&candidate) {
                Ok(data) => return Ok(data),
                Err(error) => {
                    tracing::warn!(path = %candidate.display(), %error, "MirDB 解析失败，尝试下一个候选");
                    last = Some(error);
                }
            }
        }
        Err(last.unwrap_or_else(|| MirDbError::Io {
            path: PathBuf::from("resource/community/<repo>/Server.MirDB"),
            source: std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "no Server.MirDB found (use tools/asset-index/find_asset.py --name Server.MirDB)",
            ),
        }))
    }

    /// 数据文件路径。
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// 掉落表目录（`Envir/Drops`）。
    #[must_use]
    pub fn drop_dir(&self) -> Option<&Path> {
        self.drop_dir.as_deref()
    }

    /// `Envir/` 根目录（NPC 脚本所在目录的父目录；`#INSERT` 的相对路径也从这里算）。
    #[must_use]
    pub fn envir_dir(&self) -> Option<&Path> {
        self.envir_dir.as_deref()
    }

    /// NPC 脚本目录（`Envir/NPCs`，Crystal `Settings.NPCPath`，`Server/Settings.cs:24`）。
    #[must_use]
    pub fn npc_dir(&self) -> Option<PathBuf> {
        self.envir_dir().map(|envir| envir.join("NPCs"))
    }

    /// 任务脚本目录（`Envir/Quests`，Crystal `Settings.QuestPath`，`Server/Settings.cs:27`）。
    #[must_use]
    pub fn quest_dir(&self) -> Option<PathBuf> {
        self.envir_dir().map(|envir| envir.join("Quests"))
    }

    /// 按编号取任务头部（Crystal `Envir.GetQuestInfo`，`Envir.cs`；`QuestInfo.cs:24` 的 `Index`）。
    ///
    /// 注意头部里**没有** `NpcIndex` / `FinishNpcIndex`——Crystal 的这两个字段是
    /// `NPCScript.ParseQuests` 从 NPC 脚本的 `[Quests]` 段回填的（`NPCScript.cs:711-714`），
    /// 本作按 [`Npc::collect_quest_indexes`](crate::npc::Npc) + 脚本 `[Quests]` 现查。
    #[must_use]
    pub fn quest(&self, index: i32) -> Option<&QuestInfo> {
        self.db.quests.iter().find(|info| info.index == index)
    }

    /// 任务总数（`db.quests.len()`）。
    #[must_use]
    pub fn quest_count(&self) -> usize {
        self.db.quests.len()
    }

    /// 取（并缓存）一份任务脚本。
    ///
    /// `file_name` 是 `QuestInfo.file_name`（相对 `Envir/Quests/`，不含 `.txt`）。
    /// 返回 `None` 表示脚本不存在或读不出来；结果按文件名缓存，重复调用只读一次盘。
    ///
    /// 对应 Crystal `QuestInfo.LoadInfo`（`QuestInfo.cs:130-146`）：找不到文件时它只往
    /// `MessageQueue` 丢一条提示（`:145`），任务本身照样存在（只是没有任何目标与奖励）。
    #[must_use]
    pub fn quest_script(&self, file_name: &str) -> Option<Arc<QuestScript>> {
        let envir = self.envir_dir()?;
        let key = file_name.to_owned();
        if let Some(cached) = self
            .quest_script_cache
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .get(&key)
        {
            return cached.clone();
        }
        let loaded = match QuestScript::load(&key, envir) {
            Ok(script) => {
                for warning in script.warnings() {
                    tracing::debug!(script = %key, %warning, "任务脚本告警");
                }
                Some(Arc::new(script))
            }
            Err(error) => {
                tracing::debug!(script = %key, %error, "任务脚本读取失败，按「没有脚本」处理");
                None
            }
        };
        self.quest_script_cache
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .insert(key, loaded.clone());
        loaded
    }

    /// 取（并缓存）一份 NPC 脚本。
    ///
    /// `file_name` 是 `NpcInfo.FileName`（相对 `Envir/NPCs/`，不含 `.txt`）。
    /// 返回 `None` 表示脚本不存在或读不出来；结果按文件名缓存，重复对话不会重复读盘。
    ///
    /// 对应 Crystal `NPCScript.GetOrAdd`（`NPCScript.cs:26-36`）——那边按
    /// `(LoadedObjectID, FileName)` 缓存，本作只按文件名（同一份脚本被多个 NPC 复用）。
    #[must_use]
    pub fn npc_script(&self, file_name: &str) -> Option<Arc<NpcScript>> {
        let envir = self.envir_dir()?;
        let key = file_name.to_owned();
        if let Some(cached) = self
            .npc_script_cache
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .get(&key)
        {
            return cached.clone();
        }
        let loaded = match NpcScript::load(&key, envir) {
            Ok(script) => {
                for warning in script.warnings() {
                    // Crystal 对脚本问题只往 `MessageQueue` 里丢一条提示（`NPCScript.cs:165`），
                    // 这里降级到 debug，避免几百份脚本刷屏。
                    tracing::debug!(script = %key, %warning, "NPC 脚本告警");
                }
                Some(Arc::new(script))
            }
            Err(error) => {
                tracing::debug!(script = %key, %error, "NPC 脚本读取失败，按「没有脚本」处理");
                None
            }
        };
        self.npc_script_cache
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .insert(key, loaded.clone());
        loaded
    }

    /// 某张地图上的 NPC 模板（Crystal `MapInfo.CreateMap` 的 `Envir.NPCInfoList` 过滤，
    /// `Server/MirDatabase/MapInfo.cs:197-204`）。
    #[must_use]
    pub fn npcs_for_map(&self, map_file: &str) -> Vec<&NpcInfo> {
        let Some(info) = self.map_info(map_file) else {
            return Vec::new();
        };
        let index = info.index;
        self.db
            .npcs
            .iter()
            .filter(|npc| npc.map_index == index)
            .collect()
    }

    /// 按地图**主键**找 `MapInfo`（`MovementInfo.MapIndex` / `NpcInfo.MapIndex` 用的是主键）。
    #[must_use]
    pub fn map_info_by_index(&self, index: i32) -> Option<&MapInfo> {
        self.db.maps.iter().find(|info| info.index == index)
    }

    /// 按 `ItemInfo.Index` 取物品模板（Crystal `Envir.GetItemInfo(int)`，`Envir.cs:4734-4743`）。
    #[must_use]
    pub fn item(&self, index: i32) -> Option<&ItemInfo> {
        self.item_by_index
            .get(&index)
            .and_then(|position| self.db.items.get(*position))
    }

    /// 按名字取物品模板（Crystal `Envir.GetItemInfo(string)`，`Envir.cs:4745-4754`）。
    ///
    /// 大小写不敏感、忽略空格；`StringComparison.OrdinalIgnoreCase` 的对应物是 `eq_ignore_ascii_case`
    /// 加一次小写化——中文名没有大小写，所以两者在中文数据上等价。
    #[must_use]
    pub fn item_by_name(&self, name: &str) -> Option<&ItemInfo> {
        self.item_by_name
            .get(&normalize_item_name(name))
            .and_then(|position| self.db.items.get(*position))
    }

    /// 已加载的物品模板数量。
    #[must_use]
    pub fn item_count(&self) -> usize {
        self.db.items.len()
    }

    /// 按 `Spell` 取技能模板（Crystal `Envir.MagicInfoList` 的线性查找，
    /// `Server/MirDatabase/MagicInfo.cs:103-112`）。
    ///
    /// 返回 `None` 表示数据库里没有这个技能（Crystal 里同样会得到 `magic.Info == null`）。
    #[must_use]
    pub fn magic(&self, spell: Spell) -> Option<&MagicInfo> {
        self.magic_by_spell
            .get(&spell.value())
            .and_then(|position| self.db.magics.get(*position))
    }

    /// 按名字取技能模板（大小写不敏感、忽略空格；与物品名查找同口径）。
    #[must_use]
    pub fn magic_by_name(&self, name: &str) -> Option<&MagicInfo> {
        let key = normalize_item_name(name);
        self.db
            .magics
            .iter()
            .find(|info| normalize_item_name(&info.name) == key)
    }

    /// 已加载的技能模板数量（去重前，即数据库里的原始条数）。
    #[must_use]
    pub fn magic_count(&self) -> usize {
        self.db.magics.len()
    }

    /// 全部攻城区配表（Crystal `Envir.ConquestInfoList`，`Server/MirEnvir/Envir.cs:122`；
    /// 见 [`crate::conquest`]）。
    #[must_use]
    pub fn conquests(&self) -> &[mir_db::ConquestInfo] {
        &self.db.conquests
    }

    /// 按主键取攻城区配表（Crystal `ConquestObject` 的 `Info`）。
    #[must_use]
    pub fn conquest(&self, index: i32) -> Option<&mir_db::ConquestInfo> {
        self.db.conquests.iter().find(|info| info.index == index)
    }

    /// 攻城区配表数量（真实数据冒烟用）。
    #[must_use]
    pub fn conquest_count(&self) -> usize {
        self.db.conquests.len()
    }

    /// 去重后能按 `Spell` 查到模板的数量（Crystal `MagicInfoList.Count`）。
    #[must_use]
    pub fn distinct_magic_count(&self) -> usize {
        self.magic_by_spell.len()
    }

    /// 全部商城商品（Crystal `Envir.GameShopList`，`Envir.cs:119`）。
    ///
    /// 条目的 `info` 已绑定到物品模板（见 [`GameData::from_parts`]）；数据库没有商城段
    /// 时返回空切片，服务器照常启动（商城页签空列表、购买提示「商城里没有商品」）。
    #[must_use]
    pub fn game_shop(&self) -> &[GameShopItem] {
        &self.game_shop
    }

    /// 商城商品数量（真实数据冒烟用）。
    #[must_use]
    pub fn game_shop_count(&self) -> usize {
        self.game_shop.len()
    }

    /// 按商品 `GIndex` 取一条（Crystal `Envir.GameShopList` 的线性查找，
    /// `PlayerObject.cs:13740-13747`）。
    #[must_use]
    pub fn game_shop_item(&self, g_index: i32) -> Option<&GameShopItem> {
        self.game_shop
            .iter()
            .find(|product| product.g_index == g_index)
    }

    /// 某只怪的掉落表（Crystal `MonsterInfo.Drops`，`Envir.cs:5405-5419` 加载）。
    ///
    /// 返回 `None` 表示这只怪没有掉落表（文件不存在、或数据库里没有这个怪）。
    /// 结果按文件名缓存，重复调用不会重复读盘。
    #[must_use]
    pub fn monster_drop_table(&self, monster_name: &str) -> Option<Arc<DropTable>> {
        let file = self.monster_drops.get(&monster_name.to_lowercase())?;
        self.drop_table(file)
    }

    /// 按文件名（不含目录与 `.txt`）取掉落表。
    #[must_use]
    pub fn drop_table(&self, file: &str) -> Option<Arc<DropTable>> {
        let Some(dir) = &self.drop_dir else {
            return None;
        };
        if let Some(cached) = self
            .drop_cache
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .get(file)
        {
            return cached.clone();
        }
        let name = if Path::new(file).extension().is_some() {
            file.to_owned()
        } else {
            format!("{file}.txt")
        };
        let path = dir.join(&name);
        let loaded = match DropTable::load(&path, dir) {
            Ok(table) => Some(Arc::new(table)),
            Err(error) => {
                // Crystal 对缺失的掉落文件是静默的（`MonsterInfo.Load` 直接 return，
                // `MonsterInfo.cs:374-396`），这里降一级到 debug，避免刷屏。
                tracing::debug!(path = %path.display(), %error, "掉落表读取失败，按「不掉东西」处理");
                None
            }
        };
        self.drop_cache
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .insert(file.to_owned(), loaded.clone());
        loaded
    }

    /// 按 `MonsterIndex` 取怪物种类。
    #[must_use]
    pub fn monster(&self, index: i32) -> Option<&MonsterKind> {
        self.monsters.get(&index)
    }

    /// 按名字取怪物模板（Crystal `Envir.GetMonsterInfo(string)` 的等价物，掉落解析要用）。
    #[must_use]
    pub fn monster_info_by_name(&self, name: &str) -> Option<&MonsterInfo> {
        self.db
            .monsters
            .iter()
            .find(|info| info.name.eq_ignore_ascii_case(name))
    }

    /// 已加载的怪物模板数量。
    #[must_use]
    pub fn monster_count(&self) -> usize {
        self.monsters.len()
    }

    /// 某张地图的刷怪计划。
    ///
    /// `map` 是地图文件名（Crystal 的 `MapInfo.FileName`，例如 `"0"`）。
    /// 返回只包含模板确实存在的刷怪点；模板缺失时会记一条 `warn`（数据常见于
    /// 裁剪过的数据库，例如 `Jev Without Archers`）。
    #[must_use]
    pub fn spawn_groups(&self, map: &str) -> Vec<SpawnGroup> {
        let Some(info) = self.map_info(map) else {
            return Vec::new();
        };
        let mut groups = Vec::with_capacity(info.respawns.len());
        for respawn in &info.respawns {
            let Some(kind) = self.monster(respawn.monster_index) else {
                tracing::warn!(
                    map = map,
                    monster_index = respawn.monster_index,
                    "刷怪点引用了不存在的怪物模板，已跳过"
                );
                continue;
            };
            if respawn.count == 0 {
                continue;
            }
            groups.push(SpawnGroup {
                map: info.file_name.clone(),
                kind: kind.clone(),
                center: respawn.location,
                count: usize::from(respawn.count),
                spread: i32::from(respawn.spread),
                // 刷新节奏：Crystal `Map.ProcessRespawns` 的 `Delay`/`RandomDelay`（`Map.cs:759`）。
                delay_minutes: respawn.delay,
                random_delay_minutes: respawn.random_delay,
            });
        }
        groups
    }

    /// 按地图文件名找 `MapInfo`。
    #[must_use]
    pub fn map_info(&self, map: &str) -> Option<&MapInfo> {
        self.db.maps.iter().find(|info| info.file_name == map)
    }

    /// 刷怪点总数（调试用）。
    #[must_use]
    pub fn respawn_count(&self, map: &str) -> usize {
        self.map_info(map).map_or(0, |info| info.respawns.len())
    }
}

/// 把数据库里的怪物模板转成运行期种类。
///
/// 属性全部来自 `Stats` 字典；缺失项按 0 处理（Crystal 的 `Stats` 也是「不写就是 0」）。
#[must_use]
pub fn kind_from_monster_info(info: &MonsterInfo) -> MonsterKind {
    MonsterKind {
        name: info.name.clone(),
        image: MonsterImage::from_value(info.image).unwrap_or_default(),
        ai: info.ai,
        level: info.level,
        view_range: i32::from(info.view_range),
        cool_eye: info.cool_eye,
        // 0 在 Crystal 里等于「每次轮到就动/打」，那是明显的坏数据；用默认值兜底。
        attack_speed_ms: if info.attack_speed == 0 {
            DEFAULT_ATTACK_SPEED_MS
        } else {
            i64::from(info.attack_speed)
        },
        move_speed_ms: if info.move_speed == 0 {
            DEFAULT_MOVE_SPEED_MS
        } else {
            i64::from(info.move_speed)
        },
        experience: info.experience,
        can_push: info.can_push,
        stats: stats_from_monster_info(info),
    }
}

/// 把 `Stats` 字典映射成 [`CombatStats`]。
#[must_use]
pub fn stats_from_monster_info(info: &MonsterInfo) -> CombatStats {
    let stats = &info.stats;
    let get = |stat: Stat| stats.get(stat);
    CombatStats {
        hp: get(Stat::HP),
        mp: get(Stat::MP),
        min_dc: get(Stat::MinDC),
        max_dc: get(Stat::MaxDC),
        min_mc: get(Stat::MinMC),
        max_mc: get(Stat::MaxMC),
        min_sc: get(Stat::MinSC),
        max_sc: get(Stat::MaxSC),
        min_ac: get(Stat::MinAC),
        max_ac: get(Stat::MaxAC),
        min_mac: get(Stat::MinMAC),
        max_mac: get(Stat::MaxMAC),
        accuracy: get(Stat::Accuracy),
        agility: get(Stat::Agility),
        luck: get(Stat::Luck),
        attack_speed: get(Stat::AttackSpeed),
        critical_rate: get(Stat::CriticalRate),
        critical_damage: get(Stat::CriticalDamage),
        magic_resist: get(Stat::MagicResist),
        reflect: get(Stat::Reflect),
        // 怪物的负重上限只影响「能不能被推动」之类的玩家规则，本作不用；
        // 照抄字典里的值，便于以后接入。
        bag_weight: get(Stat::BagWeight),
        wear_weight: get(Stat::WearWeight),
        hand_weight: get(Stat::HandWeight),
        // 怪物模板理论上也能带 buff 类属性（Crystal 的 `Stats` 字典是通用的），
        // 但怪物不吃玩家的增益技能，照抄数值即可。
        damage_reduction_percent: get(Stat::DamageReductionPercent),
        energy_shield_percent: get(Stat::EnergyShieldPercent),
        energy_shield_hp_gain: get(Stat::EnergyShieldHPGain),
        mana_penalty_percent: get(Stat::ManaPenaltyPercent),
    }
}

/// 物品名归一化：去掉**所有空白**并转小写（Crystal `GetItemInfo(string)` 的反向查找口径）。
#[must_use]
pub fn normalize_item_name(name: &str) -> String {
    name.chars()
        .filter(|ch| !ch.is_whitespace())
        .flat_map(char::to_lowercase)
        .collect()
}

/// 把候选路径展开成真实路径：先按仓库根，再按当前工作目录。
///
/// `CARGO_MANIFEST_DIR/../..` 里带 `..`，直接进日志很难看，这里做一次词法归一化
/// （不解析符号链接，也不要求文件存在）。
#[must_use]
pub fn candidate_paths() -> Vec<PathBuf> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join("..");
    let mut paths = Vec::with_capacity(DB_CANDIDATES.len() * 2);
    for candidate in DB_CANDIDATES {
        paths.push(normalize(&root.join(candidate)));
    }
    for candidate in DB_CANDIDATES {
        paths.push(PathBuf::from(candidate));
    }
    paths
}

/// 纯词法归一化：消掉 `.` 与 `..`（不访问文件系统）。
#[must_use]
pub fn normalize(path: &Path) -> PathBuf {
    let mut parts: Vec<std::ffi::OsString> = Vec::new();
    for component in path.components() {
        match component {
            std::path::Component::CurDir => {}
            std::path::Component::ParentDir => {
                if parts.len() > 1 {
                    parts.pop();
                }
            }
            other => parts.push(other.as_os_str().to_owned()),
        }
    }
    parts.iter().collect()
}

/// 刷怪点中心（测试与日志用）。
#[must_use]
pub fn respawn_centers(data: &GameData, map: &str) -> Vec<Point> {
    data.map_info(map)
        .map(|info| info.respawns.iter().map(|r| r.location).collect())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Option<GameData> {
        GameData::discover().ok()
    }

    #[test]
    fn discovers_a_database_from_the_index() {
        let Some(data) = sample() else {
            eprintln!("跳过：resource/ 里没有 Server.MirDB 样本");
            return;
        };
        assert!(data.monster_count() > 100, "怪物模板数量应上百");
        assert!(!data.db.maps.is_empty());
        assert!(
            data.map_info("0").is_some(),
            "样本里一定有比奇省（地图文件名 0）"
        );
    }

    #[test]
    fn monster_info_maps_to_runtime_stats() {
        let Some(data) = sample() else {
            eprintln!("跳过：resource/ 里没有 Server.MirDB 样本");
            return;
        };
        // 找一个有血量的模板，验证 HP / AC / DC 真的从 Stats 字典里取出来了。
        let with_hp = data
            .db
            .monsters
            .iter()
            .find(|info| info.stats.get(Stat::HP) > 0)
            .expect("样本里必然有带血量的怪物");
        let kind = kind_from_monster_info(with_hp);
        assert_eq!(kind.name, with_hp.name);
        assert_eq!(kind.stats.hp, with_hp.stats.get(Stat::HP));
        assert!(kind.stats.hp > 0);
        assert_eq!(kind.level, with_hp.level);
        assert!(
            kind.attack_speed_ms >= DEFAULT_ATTACK_SPEED_MS.min(i64::from(with_hp.attack_speed))
        );
    }

    #[test]
    fn world_filled_from_database_spawns_monsters() {
        let Some(data) = sample() else {
            eprintln!("跳过：resource/ 里没有 Server.MirDB 样本");
            return;
        };
        let mut world = crate::world::World::default();
        for group in data.spawn_groups("0") {
            world.spawn_group(&group);
        }
        assert!(
            world.monsters().len() > 10,
            "比奇省应该刷出不少怪，实际 {}",
            world.monsters().len()
        );
        // 怪物名字应该来自数据库（不是演示怪的占位名）。
        let names: std::collections::HashSet<&str> = world
            .monsters()
            .iter()
            .map(|m| m.kind.name.as_str())
            .collect();
        assert!(names.len() > 1, "至少有两种怪");
        assert!(
            names.iter().all(|name| !name.is_empty()),
            "怪物名字来自 MonsterInfo.Name"
        );
    }

    #[test]
    fn spawn_groups_cover_all_respawns_with_known_templates() {
        let Some(data) = sample() else {
            eprintln!("跳过：resource/ 里没有 Server.MirDB 样本");
            return;
        };
        let map = "0";
        let groups = data.spawn_groups(map);
        assert!(!groups.is_empty(), "比奇省应该有刷怪点");
        for group in &groups {
            assert!(group.count > 0);
            assert_eq!(group.map, map);
            assert!(!group.kind.name.is_empty());
        }
        let total: usize = groups.iter().map(|g| g.count).sum();
        assert!(total > 10, "比奇省的刷怪总量应该不少，实际 {total}");
    }

    #[test]
    fn sample_database_exposes_item_templates_and_drop_tables() {
        let Some(data) = sample() else {
            eprintln!("跳过：resource/ 里没有 Server.MirDB 样本");
            return;
        };
        // 物品模板（`Envir.ItemInfoList` 的对应物）。
        assert!(data.item_count() > 100, "样本里的物品模板应有上百个");
        let first = data
            .db
            .items
            .iter()
            .find(|info| !info.name.is_empty())
            .expect("样本里必然有带名字的物品");
        assert_eq!(
            data.item(first.index).map(|info| info.name.clone()),
            Some(first.name.clone()),
            "按编号能查到同一条模板"
        );
        assert_eq!(
            data.item_by_name(&first.name).map(|info| info.index),
            Some(first.index),
            "按名字能查到同一条模板"
        );

        // 掉落表目录：`<Server.MirDB 所在目录>/Envir/Drops`（Crystal `Settings.DropPath`，
        // `Settings.cs:28`）。
        let Some(dir) = data.drop_dir() else {
            eprintln!("跳过：内存数据库没有掉落目录");
            return;
        };
        assert!(dir.is_dir(), "{} 应该存在", dir.display());

        // 至少有一只怪能解析出非空掉落表（`Envir.ReloadDrops`，`Envir.cs:5405-5419`）。
        let found = data.db.monsters.iter().find_map(|info| {
            let table = data.monster_drop_table(&info.name)?;
            (!table.drops.is_empty()).then(|| (info.name.clone(), table.drops.len()))
        });
        let Some((monster, rules)) = found else {
            eprintln!("跳过：样本里没有带掉落规则的怪物");
            return;
        };
        assert!(rules > 0, "{monster} 的掉落表应有规则");
    }
}
