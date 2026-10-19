//! GM 管理命令（Crystal `PlayerObject.Chat` 里的 `@` 命令）。
//!
//! 设计要点：
//!
//! * **权限**：账号 `accounts.admin`（Crystal `AccountInfo.AdminAccount`）**或**账号名在
//!   配置项 [`GM_ACCOUNTS_ENV`]（`MIR2_GM_ACCOUNTS`，逗号分隔）名单里。判定在
//!   [`is_gm`]，由 `session.rs` 在聊天分派前调用；非 GM 的 `@...` 静默按普通聊天处理
//!   （不报错、不泄露命令存在）。
//! * **回复用英文**：GM/运维可见，不进玩家 i18n 词表（也避免中文串触发 CI 的
//!   i18n 硬编码棘轮）。
//! * **审计**：每条命令都记一条 `tracing::info!`（账号/角色、命令、参数）。
//! * **`@kick` 的会话登记表**：[`World`] 拿不到别的会话（每个连接是独立 task），
//!   `World` 本轮又不能加字段，所以用一个进程级登记表把 `session_id` 映射到踢线通道；
//!   `server.rs` 在每个连接建立时登记、断开时注销。
//!
//! 命令表与 Crystal 的对应关系见 [`World::run_gm_command`] 的文档。

use std::collections::{BTreeMap, HashMap, HashSet};
use std::fmt::Write as _;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, MutexGuard, OnceLock};

use mir_core::Point;
use mir_core::enums::Spell;
use mir_protocol::ServerPacket;
use mir_protocol::data::UserItem;
use mir_protocol::packets::server::{
    Death, DeleteItem as DeleteItemPacket, LevelChanged, MagicLeveled, ObjectDied,
};
use tokio::sync::Notify;
use tokio::sync::mpsc::UnboundedSender;

use crate::config::ServerConfig;
use crate::monster::MonsterKind;
use crate::store::Account;
use crate::world::{PLAYER_REVIVE_DELAY_MS, Player, World};

/// GM 名单的配置项名（`config.rs` 的 `from_env` 读它）。
pub const GM_ACCOUNTS_ENV: &str = "MIR2_GM_ACCOUNTS";

/// 角色等级上限（Crystal `Globals.MaxLevel = 255`）。
const MAX_LEVEL: u16 = 255;

/// `@mapinfo` 最多列多少条附近传送点（够脚本挑一个最近的，又不至于刷爆聊天）。
const MOVEMENT_LIST_LIMIT: usize = 10;

/// 把传送点表格式化成 `@mapinfo` 的几行：**按距玩家的距离升序**、最多 [`MOVEMENT_LIST_LIMIT`] 条。
///
/// 每行：`    (326,288) -> 0108(#4) (8,12) dist=18 [bigmap]`。
/// 标记含义（脚本据此筛）：
/// * `[bigmap]`：`ShowOnBigMap = true`（客户端 `S.NewMapInfo` 也会拿到它）；
/// * `[hole]`：要挖洞才能过（`NeedHole`）；
/// * `[move]`：只记录目的地、由 NPC 的 `ENTERMAP` 触发（`NeedMove`）；
/// * `[conquest]`：攻城区域专属（`ConquestIndex > 0`，只有城主行会能过）。
fn movement_lines(
    movements: &[mir_db::MovementInfo],
    current_map: &str,
    position: Point,
    resolve: &dyn Fn(i32) -> Option<String>,
) -> Vec<String> {
    let distance = |info: &mir_db::MovementInfo| {
        (info.source.x - position.x)
            .abs()
            .max((info.source.y - position.y).abs())
    };
    let mut sorted: Vec<&mir_db::MovementInfo> = movements.iter().collect();
    sorted.sort_by_key(|info| (distance(info), info.source.x, info.source.y));
    sorted
        .into_iter()
        .take(MOVEMENT_LIST_LIMIT)
        .map(|info| {
            // `MapIndex == 0` 是**同图**传送（`maps.rs::resolve_movement_target`）。
            let target = if info.map_index == 0 {
                current_map.to_owned()
            } else {
                resolve(info.map_index).unwrap_or_else(|| format!("?#{}", info.map_index))
            };
            let mut marks = String::new();
            if info.show_on_big_map {
                marks.push_str(" [bigmap]");
            }
            if info.need_hole {
                marks.push_str(" [hole]");
            }
            if info.need_move {
                marks.push_str(" [move]");
            }
            if info.conquest_index > 0 {
                marks.push_str(" [conquest]");
            }
            format!(
                "    ({},{}) -> {target}(#{}) ({},{}) dist={}{marks}",
                info.source.x,
                info.source.y,
                info.map_index,
                info.destination.x,
                info.destination.y,
                distance(info),
            )
        })
        .collect()
}

/// `@mob` 单次最多刷多少只（防止手滑把地图刷爆）。
const MAX_SPAWN_BATCH: u32 = 50;

/// `@mob` 的落点偏移：**相邻八格优先**（右 → 下 → 右下 → …），自己脚下放到最后。
///
/// 顺序很重要：GM/脚本文档约定「刷出来的怪就在你身边一格」，这样紧接着的 `C.Attack`
/// 就能打到（第 10 轮 playtest 因为怪刷在脚下/远处导致打不到，见任务 t77）。
const SPAWN_OFFSETS: [(i32, i32); 9] = [
    (1, 0),
    (0, 1),
    (1, 1),
    (-1, 1),
    (1, -1),
    (-1, 0),
    (0, -1),
    (-1, -1),
    (0, 0),
];

/// `@learn` 缺省授予的技能等级（满级 3；修炼值固定 0）。
const DEFAULT_LEARN_LEVEL: u8 = 3;

/// `@mob` 刷出的怪物在回复里最多列多少个 object id（脚本一般只要一两只）。
const MAX_REPORTED_IDS: usize = 8;

/// `@learn` 接受的中文别名表（数据文件：`config/gm/spell_aliases.json`）。
///
/// 这**不是玩家可见文本**：它只是 GM 手里的输入简写（`@learn 火墙`），
/// 所以放在 `config/gm/` 而不是 i18n 语言包里——也正因如此，
/// `tools/i18n/scan_hardcoded.py` 的口径（67/15）不再把这张表算成硬编码中文。
/// 文件缺失/解析失败时降级为空表，`@learn` 仍可用枚举名与编号。
const SPELL_ALIASES_JSON: &str = include_str!("../../../config/gm/spell_aliases.json");

/// 解析并缓存别名表（首次使用时解析一次）。
///
/// 值必须是 `Spell` 的枚举名；认不出来的条目打 `tracing::error!` 后跳过，
/// 保证「数据写错了」不会让整个 `@learn` 不可用。
fn spell_aliases() -> &'static BTreeMap<String, Spell> {
    static ALIASES: OnceLock<BTreeMap<String, Spell>> = OnceLock::new();
    ALIASES.get_or_init(|| {
        let raw: BTreeMap<String, String> = match serde_json::from_str(SPELL_ALIASES_JSON) {
            Ok(raw) => raw,
            Err(error) => {
                tracing::error!(
                    %error,
                    "config/gm/spell_aliases.json 解析失败，@learn 降级为「仅枚举名/编号」"
                );
                return BTreeMap::new();
            }
        };
        let mut table = BTreeMap::new();
        for (alias, name) in raw {
            if let Some(spell) = spell_by_enum_name(&name) {
                table.insert(alias, spell);
            } else {
                tracing::error!(
                    alias = %alias,
                    spell = %name,
                    "config/gm/spell_aliases.json 里的技能名不是已知的 Spell 枚举名，已跳过"
                );
            }
        }
        table
    })
}

/// `@reboot` 允许的最大倒计时（秒）：一天。
const MAX_REBOOT_SECONDS: u32 = 86_400;

/// 解析 `MIR2_GM_ACCOUNTS`：逗号分隔的账号名；空白项忽略，大小写不敏感去重。
///
/// ```text
/// MIR2_GM_ACCOUNTS = "admin, gm_alice ,,"
///   → ["admin", "gm_alice"]
/// ```
#[must_use]
pub fn parse_gm_accounts(raw: &str) -> Vec<String> {
    let mut seen = HashSet::new();
    let mut accounts = Vec::new();
    for name in raw.split(',') {
        let name = name.trim();
        if name.is_empty() {
            continue;
        }
        if seen.insert(name.to_ascii_lowercase()) {
            accounts.push(name.to_owned());
        }
    }
    accounts
}

/// GM 判定：`account.admin` **或**账号名在 [`ServerConfig::gm_accounts`] 名单里
/// （大小写不敏感）。
#[must_use]
pub fn is_gm(account: &Account, config: &ServerConfig) -> bool {
    account.admin
        || config
            .gm_accounts
            .iter()
            .any(|name| name.eq_ignore_ascii_case(&account.username))
}

// ---------------------------------------------------------------------------
// 会话登记表（@kick 用）
// ---------------------------------------------------------------------------

/// 一条已登记的连接：账号名/角色名用于 `@kick` 反查，`kick` 用来通知读循环断开。
struct SessionEntry {
    account: String,
    character: String,
    kick: UnboundedSender<()>,
}

fn session_registry() -> &'static Mutex<HashMap<u64, SessionEntry>> {
    static REGISTRY: OnceLock<Mutex<HashMap<u64, SessionEntry>>> = OnceLock::new();
    REGISTRY.get_or_init(|| Mutex::new(HashMap::new()))
}

fn pending_kicks() -> &'static Mutex<HashSet<u64>> {
    static PENDING: OnceLock<Mutex<HashSet<u64>>> = OnceLock::new();
    PENDING.get_or_init(|| Mutex::new(HashSet::new()))
}

/// 取锁（中毒时按内部数据继续用：登记表只是运维辅助，不应让整个进程崩）。
fn lock<T>(mutex: &'static Mutex<T>) -> MutexGuard<'static, T> {
    mutex
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// 连接建立时登记（`server.rs` 的 `serve` 调）。
pub fn register_session(session_id: u64, kick: UnboundedSender<()>) {
    if let Some(entry) = lock(session_registry()).get_mut(&session_id) {
        entry.kick = kick;
        return;
    }
    lock(session_registry()).insert(
        session_id,
        SessionEntry {
            account: String::new(),
            character: String::new(),
            kick,
        },
    );
}

/// 连接断开时注销。
pub fn unregister_session(session_id: u64) {
    lock(session_registry()).remove(&session_id);
    lock(pending_kicks()).remove(&session_id);
}

/// 登录进世界后补上账号名/角色名（`@kick <账号|角色>` 的反查键）。
pub fn set_session_identity(session_id: u64, account: &str, character: &str) {
    let mut registry = lock(session_registry());
    let Some(entry) = registry.get_mut(&session_id) else {
        return;
    };
    account.clone_into(&mut entry.account);
    character.clone_into(&mut entry.character);
}

/// 按账号名或角色名找已登记的会话。
#[must_use]
pub fn find_session(name: &str) -> Option<(u64, String)> {
    let registry = lock(session_registry());
    registry.iter().find_map(|(id, entry)| {
        let matched =
            entry.account.eq_ignore_ascii_case(name) || entry.character.eq_ignore_ascii_case(name);
        matched.then(|| (*id, entry.character.clone()))
    })
}

/// 通知某个会话断开：已登记 → 立刻发信号并返回 `true`；未登记（测试/边界）→
/// 记进待踢集合，会话处理下一条包时退出。
pub fn kick_session(session_id: u64) -> bool {
    let registry = lock(session_registry());
    if let Some(entry) = registry.get(&session_id) {
        return entry.kick.send(()).is_ok();
    }
    drop(registry);
    lock(pending_kicks()).insert(session_id);
    false
}

/// 会话在包分派前检查自己是否被踢（未登记会话的兜底路径）。
#[must_use]
pub fn take_pending_kick(session_id: u64) -> bool {
    lock(pending_kicks()).remove(&session_id)
}

/// 踢掉所有已登记的会话（`@reboot` 到点时用）。
///
/// 返回被通知的会话数；未登记会话不在统计里（它们本来就不在跑读循环）。
pub fn kick_all_sessions() -> usize {
    let ids: Vec<u64> = lock(session_registry()).keys().copied().collect();
    let mut kicked = 0;
    for id in ids {
        if kick_session(id) {
            kicked += 1;
        }
    }
    kicked
}

// ---------------------------------------------------------------------------
// 优雅停机（@reboot）
// ---------------------------------------------------------------------------

/// 重启倒计时的截止时刻（Unix 秒；`None` = 没有安排重启）。
fn reboot_deadline_slot() -> &'static Mutex<Option<i64>> {
    static DEADLINE: OnceLock<Mutex<Option<i64>>> = OnceLock::new();
    DEADLINE.get_or_init(|| Mutex::new(None))
}

/// accept/读循环用的「该停下来了」通知：`@reboot` 到点时触发。
fn reboot_notify() -> &'static Notify {
    static NOTIFY: OnceLock<Notify> = OnceLock::new();
    NOTIFY.get_or_init(Notify::new)
}

/// 已经广播过的倒计时秒数（避免每个 tick 重复喊同一句）。
fn reboot_announced() -> &'static Mutex<Option<i64>> {
    static ANNOUNCED: OnceLock<Mutex<Option<i64>>> = OnceLock::new();
    ANNOUNCED.get_or_init(|| Mutex::new(None))
}

/// 是否已经安排了重启（`@reboot` 可重复执行，以最后一次为准）。
static REBOOTING: AtomicBool = AtomicBool::new(false);

/// 当前 Unix 秒（倒计时用墙上时钟，不依赖 `World` 的单调时钟）。
#[must_use]
pub fn unix_now_secs() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |elapsed| {
            i64::try_from(elapsed.as_secs()).unwrap_or(i64::MAX)
        })
}

/// 安排一次重启，返回截止的 Unix 秒（`seconds = 0` 表示立即）。
pub fn schedule_reboot(seconds: u32) -> i64 {
    let deadline = unix_now_secs().saturating_add(i64::from(seconds));
    *lock(reboot_deadline_slot()) = Some(deadline);
    *lock(reboot_announced()) = None;
    REBOOTING.store(true, Ordering::Release);
    deadline
}

/// 当前重启截止时刻（Unix 秒）。
#[must_use]
pub fn reboot_deadline() -> Option<i64> {
    *lock(reboot_deadline_slot())
}

/// 清掉重启状态（测试与「重启被取消」用）。
pub fn clear_reboot() {
    *lock(reboot_deadline_slot()) = None;
    *lock(reboot_announced()) = None;
    REBOOTING.store(false, Ordering::Release);
}

/// 是否已经安排了重启。
#[must_use]
pub fn reboot_scheduled() -> bool {
    REBOOTING.load(Ordering::Acquire)
}

/// 全服公告文案（英文：与 GM 回执同一类「运维可见」文本）。
#[must_use]
pub fn reboot_message(seconds: u32) -> String {
    if seconds == 0 {
        "Server is rebooting now.".to_owned()
    } else {
        format!("Server is rebooting in {seconds} second(s). Please log out safely.")
    }
}

/// 倒计时的广播节奏：`remaining` 秒时是否需要喊话。
///
/// 规则：≤10 秒每秒喊；≤60 秒每 10 秒喊；再往上只在 120/300/600 秒喊。
#[must_use]
pub fn reboot_should_announce(remaining: i64) -> bool {
    if remaining <= 0 {
        return true;
    }
    if remaining <= 10 {
        return true;
    }
    if remaining <= 60 {
        return remaining % 10 == 0;
    }
    matches!(remaining, 120 | 300 | 600)
}

/// tick 侧的重启推进：返回本 tick 要广播的剩余秒数（`None` = 还没到点/已经喊过）。
///
/// 到点（`remaining == 0`）时唤醒 accept/读循环；调用方负责发公告与
/// [`kick_all_sessions`]。
pub fn poll_reboot(now: i64) -> Option<i64> {
    let deadline = reboot_deadline()?;
    let remaining = deadline.saturating_sub(now).max(0);
    if remaining == 0 {
        let mut announced = lock(reboot_announced());
        if *announced == Some(0) {
            return None;
        }
        *announced = Some(0);
        drop(announced);
        // `notify_waiters` 叫醒已在等的人；`notify_one` 留一张「许可」给还没进入
        // `select!` 的 accept/读循环（`Notify` 的语义：无等待者时许可会被保留）。
        reboot_notify().notify_waiters();
        reboot_notify().notify_one();
        return Some(0);
    }
    if !reboot_should_announce(remaining) {
        return None;
    }
    // 同一秒只喊一次：按「上次喊的秒数」去重（tick 是 100ms，会重复进来）。
    let mut announced = lock(reboot_announced());
    if *announced == Some(remaining) {
        return None;
    }
    *announced = Some(remaining);
    Some(remaining)
}

/// 等重启到点（`serve_forever` 的 accept 循环 select 它）。
pub async fn wait_for_reboot() {
    reboot_notify().notified().await;
}

impl World {
    /// 执行一条 GM 命令。返回 `true` 表示这条聊天文本已被 GM 命令消费（调用方不要再走
    /// 普通聊天）；`false` 表示不是命令（不以 `@` 开头 / 发话者不在世界里），按普通聊天处理。
    ///
    /// **调用方必须先做 [`is_gm`] 判定**：本函数不查账号权限（[`World`] 里没有账号信息），
    /// 由 `session.rs` 保证只有 GM 才会走到这里。
    ///
    /// 命令表（英文回复 + `tracing::info!` 审计）：
    ///
    /// | 命令 | 说明 |
    /// |---|---|
    /// | `@level <1-255>` | 设置等级并重算属性/回满 HP/MP |
    /// | `@exp <n>` | 设置当前经验（`LevelChanged` 同步给客户端） |
    /// | `@gold <n>` | 增加金币 |
    /// | `@credit <n>` | 增加元宝 |
    /// | `@item <名称\|索引> [数量]` | 发物品到背包（名称支持唯一片段匹配） |
    /// | `@map <名称\|索引> [x y]` | 传送到地图（可选坐标） |
    /// | `@heal` | 回满 HP/MP；死亡时复活 |
    /// | `@kill [角色名]` | 杀死目标（缺省自己） |
    /// | `@who` | 在线玩家列表 |
    /// | `@kick <账号\|角色>` | 断开会话 |
    /// | `@recall <角色名>` | 把目标传送到自己身边 |
    /// | `@mob <名称\|索引> [数量]` | 在脚下刷怪 |
    /// | `@clearitem` | 清掉本图地面物品 |
    /// | `@revive [角色名]` | 复活自己/指定角色 |
    /// | `@setpk <点数>` | 设置 PK 值并刷新名字颜色 |
    /// | `@warp <角色名> <地图> [x y]` | 把别的角色传走 |
    /// | `@mapinfo` | 当前地图/坐标/怪物数 |
    /// | `@clearbag` | 清空背包（不碰装备） |
    /// | `@reboot <秒>` | 倒计时后优雅停机 |
    /// | `@help` | 命令列表 |
    pub fn run_gm_command(&mut self, object_id: u32, text: &str) -> bool {
        let text = text.trim();
        let Some(rest) = text.strip_prefix('@') else {
            return false;
        };
        let mut parts = rest.split_whitespace();
        let Some(command) = parts.next() else {
            // 只有一个 `@`：不当命令，交回普通聊天。
            return false;
        };
        let command = command.to_ascii_lowercase();
        let args: Vec<&str> = parts.collect();
        let Some(player) = self.player(object_id) else {
            return false;
        };
        let gm_name = player.name.clone();
        tracing::info!(
            gm = %gm_name,
            object_id,
            command = %command,
            args = ?args,
            "GM command"
        );
        for line in self.execute_gm_command(object_id, &command, &args) {
            self.send_system_chat(object_id, &line);
        }
        true
    }

    /// 命令分派：返回要回给 GM 的若干行系统提示（英文）。
    fn execute_gm_command(&mut self, object_id: u32, command: &str, args: &[&str]) -> Vec<String> {
        match command {
            "help" => help_lines(),
            "level" => vec![self.gm_set_level(object_id, args)],
            "exp" => vec![self.gm_set_experience(object_id, args)],
            "gold" => vec![self.gm_add_gold(object_id, args)],
            "credit" => vec![self.gm_add_credit(object_id, args)],
            "item" => vec![self.gm_give_item(object_id, args)],
            "map" => vec![self.gm_teleport(object_id, args)],
            "heal" => vec![self.gm_heal(object_id)],
            "kill" => vec![self.gm_kill(object_id, args)],
            "who" => self.gm_who(object_id),
            "kick" => vec![self.gm_kick(args)],
            "recall" => vec![self.gm_recall(object_id, args)],
            "mob" => vec![self.gm_spawn_monsters(object_id, args)],
            "learn" => vec![self.gm_learn_magic(object_id, args)],
            "clearitem" => vec![self.gm_clear_ground_items(object_id)],
            "revive" => vec![self.gm_revive(object_id, args)],
            "setpk" => vec![self.gm_set_pk_points(object_id, args)],
            "warp" => vec![self.gm_warp(object_id, args)],
            "mapinfo" => self.gm_map_info(object_id),
            "clearbag" => vec![self.gm_clear_bag(object_id)],
            "reboot" => vec![self.gm_reboot(args)],
            other => vec![format!(
                "Unknown GM command '@{other}'. Type @help for the command list."
            )],
        }
    }

    fn gm_set_level(&mut self, object_id: u32, args: &[&str]) -> String {
        let Some(level) = parse_bounded::<u16>(args.first().copied(), &1, &MAX_LEVEL) else {
            return format!("Usage: @level <1-{MAX_LEVEL}>");
        };
        // `RefreshLevelStats` 在 Crystal 里会一并 `RefreshMaxExperience`（`HumanObject.cs:1740-1742`），
        // 所以这里也要按新等级重算「下一级所需经验」，否则 `S.LevelChanged` 带旧上限。
        let max_experience = self.exp_table().exp_for_level(level);
        if let Some(player) = self.player_mut(object_id) {
            player.level = level;
            player.max_experience = max_experience;
        }
        self.refresh_stats(object_id);
        let (hp, mp) = match self.player(object_id) {
            Some(player) => (player.stats.hp, player.stats.mp),
            None => return "ERR: you are not in the world.".to_owned(),
        };
        self.set_player_hp(object_id, hp);
        self.set_player_mp(object_id, mp);
        self.sync_level_packet(object_id);
        format!("Level set to {level} (next level at {max_experience}).")
    }

    fn gm_set_experience(&mut self, object_id: u32, args: &[&str]) -> String {
        let Some(exp) = parse_bounded::<i64>(args.first().copied(), &0, &i64::MAX) else {
            return "Usage: @exp <non-negative amount>".to_owned();
        };
        let Some(level) = self.player(object_id).map(|player| player.level.max(1)) else {
            return "ERR: you are not in the world.".to_owned();
        };
        let max_experience = self.exp_table().exp_for_level(level);
        if let Some(player) = self.player_mut(object_id) {
            player.experience = exp;
            player.max_experience = max_experience;
        }
        self.sync_level_packet(object_id);
        format!("Experience set to {exp} (next level at {max_experience}).")
    }

    fn gm_add_gold(&mut self, object_id: u32, args: &[&str]) -> String {
        let Some(amount) = parse_bounded::<u32>(args.first().copied(), &0, &u32::MAX) else {
            return "Usage: @gold <amount>".to_owned();
        };
        self.gain_gold(object_id, amount);
        let total = self.player(object_id).map_or(0, |player| player.gold);
        format!("Gold +{amount} (total {total}).")
    }

    fn gm_add_credit(&mut self, object_id: u32, args: &[&str]) -> String {
        let Some(amount) = parse_bounded::<u32>(args.first().copied(), &0, &u32::MAX) else {
            return "Usage: @credit <amount>".to_owned();
        };
        self.gain_credit(object_id, amount);
        let total = self.player(object_id).map_or(0, |player| player.credit);
        format!("Credits +{amount} (total {total}).")
    }

    fn gm_give_item(&mut self, object_id: u32, args: &[&str]) -> String {
        let Some(query) = args.first() else {
            return "Usage: @item <name|index> [count]".to_owned();
        };
        let count = match args.get(1) {
            None => 1_u16,
            Some(raw) => match raw.parse::<u16>() {
                Ok(count) if count > 0 => count,
                _ => return "Usage: @item <name|index> [count>0]".to_owned(),
            },
        };
        let Some(info) = self.find_item_template(query) else {
            return format!("Item not found: '{query}'.");
        };
        let (index, name) = (info.index, info.name.clone());
        let item = UserItem {
            unique_id: self.allocate_item_uid(),
            item_index: index,
            count,
            ..UserItem::default()
        };
        if self.gain_item(object_id, item) {
            format!("Gave {count} x {name}.")
        } else {
            format!("Bag full or overweight: {count} x {name} not given.")
        }
    }

    fn gm_teleport(&mut self, object_id: u32, args: &[&str]) -> String {
        let Some(query) = args.first() else {
            return "Usage: @map <name|index> [x y]".to_owned();
        };
        let Some(map) = self.resolve_map_name(query) else {
            return format!("Map not found: '{query}'.");
        };
        let position = match (args.get(1), args.get(2)) {
            (Some(x), Some(y)) => match (x.parse::<i32>(), y.parse::<i32>()) {
                (Ok(x), Ok(y)) => Some(Point::new(x, y)),
                _ => return "Usage: @map <name|index> [x y] (x/y must be integers)".to_owned(),
            },
            (None, None) => self.player(object_id).map(|player| player.position),
            _ => return "Usage: @map <name|index> [x y]".to_owned(),
        };
        let Some(position) = position else {
            return "ERR: you are not in the world.".to_owned();
        };
        if self.place_player(object_id, &map, position) {
            format!("Teleported to {map} ({}, {}).", position.x, position.y)
        } else {
            format!("Cannot enter map '{map}'.")
        }
    }

    fn gm_heal(&mut self, object_id: u32) -> String {
        if self.player(object_id).is_some_and(|player| player.dead) && self.revive_player(object_id)
        {
            return "Revived and healed.".to_owned();
        }
        self.refresh_stats(object_id);
        let (hp, mp) = match self.player(object_id) {
            Some(player) => (player.stats.hp, player.stats.mp),
            None => return "ERR: you are not in the world.".to_owned(),
        };
        self.set_player_hp(object_id, hp);
        self.set_player_mp(object_id, mp);
        format!("Healed to {hp} HP / {mp} MP.")
    }

    fn gm_kill(&mut self, object_id: u32, args: &[&str]) -> String {
        let target_id = match args.first() {
            None => object_id,
            Some(name) => match self.player_id_by_name(name) {
                Some(target_id) => target_id,
                None => return format!("Player not found: '{name}'."),
            },
        };
        let name = self
            .player(target_id)
            .map_or_else(String::new, |player| player.name.clone());
        if self.gm_kill_player(target_id) {
            format!("Killed {name}.")
        } else {
            format!("{name} is already dead.")
        }
    }

    fn gm_who(&mut self, _object_id: u32) -> Vec<String> {
        let mut players: Vec<(u32, String, u16, String)> = self
            .players
            .iter()
            .map(|(id, player)| (*id, player.name.clone(), player.level, player.map.clone()))
            .collect();
        players.sort_by_key(|(id, ..)| *id);
        let mut lines = vec![format!("{} player(s) online:", players.len())];
        for (id, name, level, map) in players {
            lines.push(format!("  {name} lv{level} @{map} (#{id})"));
        }
        // 只返回行，由 `run_gm_command` 统一发送（避免「合并一条 + 逐行再发」的重复输出）。
        lines
    }

    fn gm_kick(&mut self, args: &[&str]) -> String {
        let Some(query) = args.first() else {
            return "Usage: @kick <account|character>".to_owned();
        };
        if let Some((session_id, name)) = find_session(query) {
            let delivered = kick_session(session_id);
            return if delivered {
                format!("Kicked '{name}' (session {session_id}).")
            } else {
                format!("Kicked '{name}' (session {session_id}); disconnect pending.")
            };
        }
        // 会话登记表里没有（测试/裸 World）：退回按角色名找玩家，标记其会话待踢。
        match self.player_id_by_name(query) {
            Some(target_id) => {
                let (session_id, name) =
                    self.player(target_id).map_or((0, String::new()), |player| {
                        (player.session_id, player.name.clone())
                    });
                kick_session(session_id);
                format!("Kicked '{name}' (session {session_id}); disconnect pending.")
            }
            None => format!("Player not found: '{query}'."),
        }
    }

    fn gm_recall(&mut self, object_id: u32, args: &[&str]) -> String {
        let Some(query) = args.first() else {
            return "Usage: @recall <character>".to_owned();
        };
        let Some(target_id) = self.player_id_by_name(query) else {
            return format!("Player not found: '{query}'.");
        };
        let Some((map, position, name)) = self
            .player(object_id)
            .map(|player| (player.map.clone(), player.position, player.name.clone()))
        else {
            return "ERR: you are not in the world.".to_owned();
        };
        let target_name = self
            .player(target_id)
            .map_or_else(String::new, |player| player.name.clone());
        if self.place_player(target_id, &map, position) {
            format!("Recalled {target_name} to {name}.")
        } else {
            format!("Cannot move {target_name} to {map}.")
        }
    }

    /// 物品模板查找：先按索引，再按精确名（大小写/空格不敏感），最后按唯一片段匹配。
    fn find_item_template(&self, query: &str) -> Option<&mir_db::ItemInfo> {
        if let Ok(index) = query.parse::<i32>()
            && let Some(info) = self.item_info(index)
        {
            return Some(info);
        }
        if let Some(info) = self.game_data().and_then(|data| data.item_by_name(query)) {
            return Some(info);
        }
        let needle = query.to_ascii_lowercase();
        let limit = self.game_data().map_or(0, |data| data.item_count());
        (0..=limit).find_map(|index| {
            self.item_info(i32::try_from(index).ok()?).and_then(|info| {
                info.name
                    .to_ascii_lowercase()
                    .contains(&needle)
                    .then_some(info)
            })
        })
    }

    /// 地图名宽松解析：索引 → `MapInfo.file_name`；文件名直接匹配。
    fn resolve_map_name(&self, query: &str) -> Option<String> {
        let data = self.game_data()?;
        if let Ok(index) = query.parse::<i32>() {
            return data
                .map_info_by_index(index)
                .map(|info| info.file_name.clone());
        }
        data.map_info(query).map(|info| info.file_name.clone())
    }

    /// `@learn <技能名|spell 枚举名|编号> [等级 0-3]`：给自己授予一个技能（GM 通道）。
    ///
    /// 用途：playtest / 自动化脚本要「先学技能再试技能」，而正常途径（书 + `C.UseItem`）
    /// 依赖真实数据库里的书物品。这里直接走 [`World::learn_magic`]（与用书同一条路径：
    /// 会校验模板、防重复、发 `S.NewMagic`），再把等级提到 `level`（缺省
    /// [`DEFAULT_LEARN_LEVEL`]，修炼值固定 0），补发 `S.MagicLeveled`。
    ///
    /// 名称匹配（大小写不敏感，允许下划线/空格/连字符）：
    /// 1. `Spell` 枚举名（`ThunderBolt` / `thunder_bolt`）；
    /// 2. 常用中文别名（`config/gm/spell_aliases.json`，如 `火墙` / `雷电术` / `召唤神兽`）；
    /// 3. 数字编号（`@learn 25` → `Spell::ThunderBolt`）。
    fn gm_learn_magic(&mut self, object_id: u32, args: &[&str]) -> String {
        let Some(query) = args.first() else {
            return "Usage: @learn <spell name|index> [level 0-3]".to_owned();
        };
        let level = match args.get(1) {
            None => DEFAULT_LEARN_LEVEL,
            Some(raw) => match raw.parse::<u8>() {
                Ok(level) if level <= 3 => level,
                _ => return "Usage: @learn <spell name|index> [level 0-3]".to_owned(),
            },
        };
        let Some(player) = self.player(object_id) else {
            return "ERR: you are not in the world.".to_owned();
        };
        let gm_name = player.name.clone();

        let Some(spell) = Self::resolve_spell(query) else {
            return format!(
                "Spell not found: '{query}'. Known spells: {}",
                self.spell_candidates(query).join(", ")
            );
        };
        let spell_name = spell_name(spell);
        if self
            .player(object_id)
            .is_some_and(|player| player.magics.iter().any(|magic| magic.spell == spell))
        {
            let current = self
                .player(object_id)
                .and_then(|player| player.magics.iter().find(|magic| magic.spell == spell))
                .map_or(0, |magic| magic.level);
            return format!("You already know {spell_name} (level {current}).");
        }
        match self.learn_magic(object_id, spell) {
            Ok(mut magic) => {
                magic.level = level;
                magic.experience = 0;
                if let Some(player) = self.players.get_mut(&object_id) {
                    if let Some(stored) = player
                        .magics
                        .iter_mut()
                        .find(|stored| stored.spell == spell)
                    {
                        *stored = magic;
                    }
                }
                // `S.MagicLeveled`：`S.NewMagic` 里带的是 0 级，这里补一条正确等级。
                self.send_to_player(
                    object_id,
                    &ServerPacket::MagicLeveled(MagicLeveled {
                        object_id,
                        spell,
                        level,
                        experience: 0,
                    })
                    .encode_frame(),
                );
                let total = self
                    .player(object_id)
                    .map_or(0, |player| player.magics.len());
                tracing::info!(
                    gm = %gm_name,
                    object_id,
                    spell = %spell_name,
                    level,
                    "GM learned spell"
                );
                format!("Learned {spell_name} at level {level} (you now know {total} spell(s)).")
            }
            Err(error) => format!("Cannot learn {spell_name}: {error}"),
        }
    }

    /// 把 `@learn` 的查询串解析成 `Spell`：数字编号 → 中文别名 → 枚举名。
    fn resolve_spell(query: &str) -> Option<Spell> {
        if let Ok(index) = query.parse::<u8>()
            && let Some(spell) = Spell::from_value(index)
        {
            return Some(spell);
        }
        let normalized = normalize_name(query);
        if let Some(spell) = spell_aliases()
            .iter()
            .find(|(alias, _)| normalize_name(alias) == normalized)
            .map(|(_, spell)| *spell)
        {
            return Some(spell);
        }
        spell_by_enum_name(query)
    }

    /// 名字打错时给出的候选：优先「包含查询串的枚举名」，否则列出当前数据库里有模板的技能。
    fn spell_candidates(&self, query: &str) -> Vec<String> {
        let normalized = normalize_name(query);
        let mut hits: Vec<String> = all_spells()
            .filter(|spell| {
                let name = normalize_name(&spell_name(*spell));
                !normalized.is_empty() && (name.contains(&normalized) || normalized.contains(&name))
            })
            .map(spell_name)
            .collect();
        if hits.is_empty() {
            hits = all_spells()
                .filter(|spell| self.magic_info(*spell).is_some())
                .map(spell_name)
                .collect();
        }
        hits.truncate(12);
        if hits.is_empty() {
            hits.push("(no spell templates in the loaded database)".to_owned());
        }
        hits
    }

    /// `@mob <名称|索引> [数量]`：在 GM 脚下（3×3 邻格轮着放）刷怪。
    ///
    /// Crystal 的 `@MOB`（`PlayerObject.Chat`）同样是「在施法者附近生成」；本作只做最小实现：
    /// 不设归属、不计入刷怪点，怪物按自己的 AI 立刻开始活动。
    fn gm_spawn_monsters(&mut self, object_id: u32, args: &[&str]) -> String {
        let Some(query) = args.first() else {
            return "Usage: @mob <name|index> [count]".to_owned();
        };
        let count = match args.get(1) {
            None => 1_u32,
            Some(raw) => match raw.parse::<u32>() {
                Ok(count) if (1..=MAX_SPAWN_BATCH).contains(&count) => count,
                _ => return format!("Usage: @mob <name|index> [count 1..{MAX_SPAWN_BATCH}]"),
            },
        };
        let Some(kind) = self.find_monster_kind(query) else {
            return format!("Monster not found: '{query}'.");
        };
        let Some((map, position, name)) = self
            .player(object_id)
            .map(|player| (player.map.clone(), player.position, player.name.clone()))
        else {
            return "ERR: you are not in the world.".to_owned();
        };
        let kind_name = kind.name.clone();
        let mut spawned = Vec::with_capacity(usize::try_from(count).unwrap_or(0));
        for step in 0..count {
            let offset = SPAWN_OFFSETS[usize::try_from(step).unwrap_or(0) % SPAWN_OFFSETS.len()];
            let cell = Point::new(position.x + offset.0, position.y + offset.1);
            spawned.push(self.spawn_monster(&map, kind.clone(), cell));
        }
        // 把 object id 写进回复：脚本拿到后可以直接 `C.Attack`/`S.ObjectMonster` 对账，
        // 不必再靠「刚刷出来的是哪个 ID」去猜。
        let listed: Vec<String> = spawned
            .iter()
            .take(MAX_REPORTED_IDS)
            .map(u32::to_string)
            .collect();
        let mut reply = format!(
            "Spawned {count} x {kind_name} near {name} on {map}. object ids: [{}]",
            listed.join(", ")
        );
        if spawned.len() > MAX_REPORTED_IDS {
            // `write!` 到 `String`：clippy 不建议 `push_str(&format!(..))`。
            let _ = write!(reply, " (+{} more)", spawned.len() - MAX_REPORTED_IDS);
        }
        reply
    }

    /// `@clearitem`：清掉 GM 当前地图上的所有地面物品（物品堆 + 金币堆）。
    fn gm_clear_ground_items(&mut self, object_id: u32) -> String {
        let Some(map) = self.player(object_id).map(|player| player.map.clone()) else {
            return "ERR: you are not in the world.".to_owned();
        };
        let ids: Vec<u32> = self
            .ground_items()
            .iter()
            .filter(|ground| ground.map == map)
            .map(|ground| ground.object_id)
            .collect();
        let cleared = ids.len();
        for object_id in ids {
            self.remove_ground_item(object_id);
        }
        format!("Cleared {cleared} ground item(s) on {map}.")
    }

    /// `@revive [角色]`：复活自己或指定角色（Crystal `RevivePlayer` 的手动入口）。
    fn gm_revive(&mut self, object_id: u32, args: &[&str]) -> String {
        let target_id = match args.first() {
            None => object_id,
            Some(name) => match self.player_id_by_name(name) {
                Some(target_id) => target_id,
                None => return format!("Player not found: '{name}'."),
            },
        };
        let Some((name, dead)) = self
            .player(target_id)
            .map(|player| (player.name.clone(), player.dead))
        else {
            return "ERR: you are not in the world.".to_owned();
        };
        if !dead {
            return format!("{name} is not dead.");
        }
        if self.revive_player(target_id) {
            format!("Revived {name}.")
        } else {
            format!("Cannot revive {name}.")
        }
    }

    /// `@setpk <点数>`：设置 PK 值并立即刷新名字颜色（Crystal `RefreshNameColour`）。
    fn gm_set_pk_points(&mut self, object_id: u32, args: &[&str]) -> String {
        let Some(points) = parse_bounded::<i32>(args.first().copied(), &0, &i32::MAX) else {
            return "Usage: @setpk <points >= 0>".to_owned();
        };
        let Some(player) = self.player_mut(object_id) else {
            return "ERR: you are not in the world.".to_owned();
        };
        player.pk_points = points;
        let name = player.name.clone();
        self.refresh_name_colour(object_id);
        tracing::info!(gm = %name, object_id, points, "GM setpk");
        format!("PK points set to {points}.")
    }

    /// `@warp <角色> <地图> [x y]`：把**别的**角色传送到指定地图（GM 自己用 `@map`）。
    fn gm_warp(&mut self, _object_id: u32, args: &[&str]) -> String {
        let (Some(name), Some(map_query)) = (args.first(), args.get(1)) else {
            return "Usage: @warp <character> <map> [x y]".to_owned();
        };
        let Some(target_id) = self.player_id_by_name(name) else {
            return format!("Player not found: '{name}'.");
        };
        let Some(map) = self.resolve_map_name(map_query) else {
            return format!("Map not found: '{map_query}'.");
        };
        let position = match (args.get(2), args.get(3)) {
            (None, None) => self.player(target_id).map(|player| player.position),
            (Some(x), Some(y)) => match (x.parse::<i32>(), y.parse::<i32>()) {
                (Ok(x), Ok(y)) => Some(Point::new(x, y)),
                _ => {
                    return "Usage: @warp <character> <map> [x y] (x/y must be integers)"
                        .to_owned();
                }
            },
            _ => return "Usage: @warp <character> <map> [x y]".to_owned(),
        };
        let Some(position) = position else {
            return "ERR: target is not in the world.".to_owned();
        };
        let target_name = self
            .player(target_id)
            .map_or_else(String::new, |player| player.name.clone());
        if self.place_player(target_id, &map, position) {
            tracing::info!(
                target = %target_name,
                %map,
                x = position.x,
                y = position.y,
                "GM warp"
            );
            format!(
                "Warped {target_name} to {map} ({}, {}).",
                position.x, position.y
            )
        } else {
            format!("Cannot warp {target_name} to {map}.")
        }
    }

    /// `@mapinfo`：当前地图/坐标/站立格是否可走/本图怪物、玩家与地面物品数，
    /// 以及**附近的传送点**（`MapInfo.Movements`，含 `ShowOnBigMap = false` 的那些）。
    ///
    /// 为什么要列非大地图传送点：Crystal 的 `S.NewMapInfo` 只带 `ShowOnBigMap` 的条目
    /// （`Server/MirObjects/PlayerObject.cs:1009-1056`，客户端用来画大地图图标），所以
    /// playtest 这类「照客户端协议说话」的脚本看不到「走进城门就换图」的那些格子。
    /// 这条 GM 命令是**开发者视角**的旁路：只用于观察数据，真正换图仍然由玩家走进去触发
    /// （`maps.rs::check_movement`），不改变客户端语义。
    fn gm_map_info(&mut self, object_id: u32) -> Vec<String> {
        let Some(player) = self.player(object_id) else {
            return vec!["ERR: you are not in the world.".to_owned()];
        };
        let (map, position, name) = (player.map.clone(), player.position, player.name.clone());
        let walkable = self
            .map_of(&map)
            .is_some_and(|map| map.walkable(position.x, position.y));
        let monsters = self
            .monsters
            .iter()
            .filter(|monster| monster.map == map)
            .count();
        let players = self
            .players
            .values()
            .filter(|player| player.map == map)
            .count();
        let ground = self
            .ground_items()
            .iter()
            .filter(|ground| ground.map == map)
            .count();
        let mut lines = vec![
            format!("Map info for {name}:"),
            format!("  map {map} at ({}, {})", position.x, position.y),
            format!("  walkable: {}", if walkable { "yes" } else { "no" }),
            format!("  monsters on map: {monsters}"),
            format!("  players on map: {players}"),
            format!("  ground items on map: {ground}"),
        ];
        // 附近传送点：按距玩家的**切比雪夫距离**升序（与世界层的 `Point` 距离口径一致）。
        let movements = self.movements_on(&map).to_vec();
        let resolver = |index: i32| -> Option<String> {
            self.game_data()
                .and_then(|data| data.map_info_by_index(index))
                .map(|info| info.file_name.clone())
        };
        lines.push(format!(
            "  movements: {} total, {} on big map (nearest {})",
            movements.len(),
            movements.iter().filter(|info| info.show_on_big_map).count(),
            MOVEMENT_LIST_LIMIT.min(movements.len()),
        ));
        lines.extend(movement_lines(&movements, &map, position, &resolver));
        lines
    }

    /// `@clearbag`：清空背包（不碰装备栏/仓库），逐件下发 `S.DeleteItem`。
    fn gm_clear_bag(&mut self, object_id: u32) -> String {
        let items: Vec<mir_protocol::data::UserItem> = self
            .player(object_id)
            .map(|player| player.inventory.iter().flatten().cloned().collect())
            .unwrap_or_default();
        let cleared = items.len();
        if let Some(player) = self.player_mut(object_id) {
            player.inventory.fill(None);
        }
        for item in items {
            self.send_to_player(
                object_id,
                &ServerPacket::DeleteItem(DeleteItemPacket {
                    unique_id: item.unique_id,
                    count: item.count,
                })
                .encode_frame(),
            );
        }
        format!("Cleared {cleared} item stack(s) from the bag.")
    }

    /// `@reboot <秒>`：安排一次优雅停机（倒计时广播 + 到点踢线 + 停止 accept）。
    fn gm_reboot(&mut self, args: &[&str]) -> String {
        let Some(seconds) = parse_bounded::<u32>(args.first().copied(), &0, &MAX_REBOOT_SECONDS)
        else {
            return format!("Usage: @reboot <seconds 0..{MAX_REBOOT_SECONDS}>");
        };
        let deadline = schedule_reboot(seconds);
        let message = reboot_message(seconds);
        self.broadcast_system(&message);
        tracing::info!(seconds, deadline, "GM reboot scheduled");
        if seconds == 0 {
            "Server rebooting now.".to_owned()
        } else {
            format!("Server reboot scheduled in {seconds} second(s).")
        }
    }

    fn player_id_by_name(&self, name: &str) -> Option<u32> {
        self.players
            .iter()
            .find_map(|(id, player)| player.name.eq_ignore_ascii_case(name).then_some(*id))
    }

    /// 怪物模板查找：先按索引，再按精确名（大小写不敏感），最后按唯一片段匹配。
    fn find_monster_kind(&self, query: &str) -> Option<MonsterKind> {
        let data = self.game_data()?;
        if let Ok(index) = query.parse::<i32>()
            && let Some(kind) = data.monster(index)
        {
            return Some(kind.clone());
        }
        if let Some(info) = data.monster_info_by_name(query) {
            return Some(crate::gamedata::kind_from_monster_info(info));
        }
        let needle = query.to_ascii_lowercase();
        let limit = data.monster_count();
        (0..=limit).find_map(|index| {
            let kind = data.monster(i32::try_from(index).ok()?)?;
            kind.name
                .to_ascii_lowercase()
                .contains(&needle)
                .then(|| kind.clone())
        })
    }

    /// 把当前等级/经验/下一级经验同步给客户端（Crystal `S.LevelChanged`）。
    fn sync_level_packet(&self, object_id: u32) {
        let Some(player) = self.player(object_id) else {
            return;
        };
        self.send_to_player(
            object_id,
            &ServerPacket::LevelChanged(LevelChanged {
                level: player.level,
                experience: player.experience,
                max_experience: player.max_experience,
            })
            .encode_frame(),
        );
    }

    /// GM 处死：Crystal `PlayerObject.Die()` 的最小可复现版本（不发死亡掉落/不中断交易，
    /// 其余与正常死亡一致：`HP=0`、`Dead=true`、`ReviveTime`、`S.Death` + `S.ObjectDied`）。
    fn gm_kill_player(&mut self, target_id: u32) -> bool {
        let Some((map, position, direction, dead)) = self.player(target_id).map(|player| {
            (
                player.map.clone(),
                player.position,
                player.direction,
                player.dead,
            )
        }) else {
            return false;
        };
        if dead {
            return false;
        }
        let now = self.now();
        if let Some(player) = self.player_mut(target_id) {
            player.hp = 0;
            player.dead = true;
            player.death_time = now;
            player.revive_time = now + PLAYER_REVIVE_DELAY_MS;
            player.magic.poisons.clear();
            player.magic.hidden = false;
        }
        self.kill_pets(target_id);
        self.send_to_player(
            target_id,
            &ServerPacket::Death(Death {
                location: position,
                direction,
            })
            .encode_frame(),
        );
        let died = ServerPacket::ObjectDied(ObjectDied {
            object_id: target_id,
            location: position,
            direction,
            type_: 0,
        })
        .encode_frame();
        self.broadcast(&map, position, Some(target_id), &died, target_id);
        true
    }
}

/// `@help` 的命令表（英文，与 [`World::run_gm_command`] 的文档保持一致）。
/// 遍历 `Spell` 的全部变体（`from_value` 覆盖 0..=255，跳过空洞）。
fn all_spells() -> impl Iterator<Item = Spell> {
    (0..=u8::MAX).filter_map(Spell::from_value)
}

/// 按枚举名（大小写/分隔符不敏感）查 `Spell`。
fn spell_by_enum_name(name: &str) -> Option<Spell> {
    let normalized = normalize_name(name);
    all_spells().find(|spell| normalize_name(&spell_name(*spell)) == normalized)
}

/// `Spell` 的枚举名（`Spell::ThunderBolt` → `ThunderBolt`），给 GM 回复/匹配用。
fn spell_name(spell: Spell) -> String {
    format!("{spell:?}")
}

/// 名字归一化：只留字母数字与 CJK，统一小写（`thunder-bolt` / `ThunderBolt` 等价）。
fn normalize_name(raw: &str) -> String {
    raw.chars()
        .filter(|ch| ch.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

fn help_lines() -> Vec<String> {
    vec![
        "GM commands:".to_owned(),
        "  @level <1-255>        set level (stats/HP/MP refreshed)".to_owned(),
        "  @exp <n>              set current experience".to_owned(),
        "  @gold <n>             add gold".to_owned(),
        "  @credit <n>           add credits".to_owned(),
        "  @item <name|index> [count]   give items".to_owned(),
        "  @map <name|index> [x y]      teleport".to_owned(),
        "  @heal                 heal / revive yourself".to_owned(),
        "  @kill [character]     kill a player (default: you)".to_owned(),
        "  @who                  list online players".to_owned(),
        "  @kick <account|character>    disconnect a client".to_owned(),
        "  @recall <character>   bring a player to you".to_owned(),
        "  @mob <name|index> [count]    spawn monsters in adjacent cells (replies with ids)"
            .to_owned(),
        "  @learn <spell> [level 0-3]   learn/grant a spell at level 3 by default".to_owned(),
        "  @clearitem            clear ground items on your map".to_owned(),
        "  @revive [character]   revive a dead player (default: you)".to_owned(),
        "  @setpk <points>       set PK points (refreshes name colour)".to_owned(),
        "  @warp <character> <map> [x y]  send another player elsewhere".to_owned(),
        "  @mapinfo              current map / position / counts".to_owned(),
        "  @clearbag             clear your bag (equipment stays)".to_owned(),
        "  @reboot <seconds>     graceful shutdown with countdown".to_owned(),
        "  @help                 this list".to_owned(),
    ]
}

/// 解析并夹紧一个整数参数；缺失/格式错/越界都返回 `None`。
fn parse_bounded<T>(raw: Option<&str>, min: &T, max: &T) -> Option<T>
where
    T: std::str::FromStr + PartialOrd,
{
    let value: T = raw?.parse().ok()?;
    (value >= *min && value <= *max).then_some(value)
}

/// 玩家名（测试/工具用）：按对象 ID 取名字，找不到返回空串。
#[must_use]
pub fn player_name(world: &World, object_id: u32) -> String {
    world
        .player(object_id)
        .map_or_else(String::new, |player: &Player| player.name.clone())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gm_accounts_parser_ignores_blanks_and_duplicates() {
        assert_eq!(
            parse_gm_accounts(" admin, GM_Alice ,,gm_alice "),
            vec!["admin".to_owned(), "GM_Alice".to_owned()]
        );
        assert!(parse_gm_accounts("  ,, ").is_empty());
    }

    /// 造一条传送点（`need_hole`/`need_move`/`show_on_big_map` 由调用方指定）。
    fn movement(
        map_index: i32,
        source: (i32, i32),
        destination: (i32, i32),
    ) -> mir_db::MovementInfo {
        mir_db::MovementInfo {
            map_index,
            source: Point::new(source.0, source.1),
            destination: Point::new(destination.0, destination.1),
            need_hole: false,
            need_move: false,
            conquest_index: 0,
            show_on_big_map: false,
            icon: 0,
        }
    }

    #[test]
    fn movement_lines_are_sorted_by_distance_and_capped() {
        let movements = vec![
            movement(4, (400, 400), (8, 12)), // 距 (330,270) 的切比雪夫距离 130
            movement(6, (332, 271), (4, 10)), // 2
            movement(3, (340, 268), (3, 7)),  // 10
        ];
        let resolve = |index: i32| match index {
            3 => Some("0107".to_owned()),
            4 => Some("0108".to_owned()),
            6 => Some("0109".to_owned()),
            _ => None,
        };
        let lines = movement_lines(&movements, "0", Point::new(330, 270), &resolve);
        assert_eq!(lines.len(), 3, "三条都列得下");
        assert!(
            lines[0].contains("(332,271)") && lines[0].contains("dist=2"),
            "nearest movement must be first: {lines:?}"
        );
        assert!(lines[1].contains("(340,268)") && lines[1].contains("dist=10"));
        assert!(lines[2].contains("(400,400)") && lines[2].contains("dist=130"));
        // 目标图文件名来自 resolver（`MapInfo.MapIndex → FileName`）。
        assert!(lines[0].contains("0109(#6)"), "{}", lines[0]);
    }

    #[test]
    fn movement_lines_mark_special_kinds_and_same_map() {
        let mut big = movement(4, (331, 270), (8, 12));
        big.show_on_big_map = true;
        let mut hole = movement(5, (332, 270), (1, 1));
        hole.need_hole = true;
        let mut need_move = movement(6, (333, 270), (2, 2));
        need_move.need_move = true;
        let mut conquest = movement(7, (334, 270), (3, 3));
        conquest.conquest_index = 69;
        // `MapIndex == 0` 是**同图**传送：目标图就是当前地图。
        let same_map = movement(0, (335, 270), (300, 300));

        let lines = movement_lines(
            &[big, hole, need_move, conquest, same_map],
            "0",
            Point::new(330, 270),
            &|index| Some(format!("map{index}")),
        );
        assert!(lines[0].contains("[bigmap]") && lines[0].contains("map4(#4)"));
        assert!(lines[1].contains("[hole]"));
        assert!(lines[2].contains("[move]"));
        assert!(lines[3].contains("[conquest]"));
        assert!(
            lines[4].contains(" -> 0(#0) "),
            "same-map movement points at current map name: {}",
            lines[4]
        );
        // 无法解析目标图时用 `?#index` 标出来，不 panic。
        let unknown = movement_lines(
            &[movement(999, (330, 270), (1, 1))],
            "0",
            Point::new(330, 270),
            &|_| None,
        );
        assert!(unknown[0].contains("?#999"), "{}", unknown[0]);
    }

    #[test]
    fn movement_lines_never_exceed_the_list_limit() {
        let movements: Vec<mir_db::MovementInfo> = (0..25)
            .map(|offset| movement(4, (330 + offset, 270), (1, 1)))
            .collect();
        let lines = movement_lines(&movements, "0", Point::new(330, 270), &|_| {
            Some("m".to_owned())
        });
        assert_eq!(
            lines.len(),
            MOVEMENT_LIST_LIMIT,
            "movement list must cap at MOVEMENT_LIST_LIMIT"
        );
        assert!(lines[0].contains("dist=0"), "最近的（同格）排第一");
    }

    #[test]
    fn is_gm_accepts_admin_flag_or_listed_account() {
        let config = ServerConfig {
            gm_accounts: parse_gm_accounts("gm_alice"),
            ..ServerConfig::default()
        };
        let mut account = Account::new_with_password(1, "someone", "secret", "");
        assert!(!is_gm(&account, &config), "普通账号不是 GM");
        account.admin = true;
        assert!(is_gm(&account, &config), "accounts.admin 即可");
        account.admin = false;
        account.username = "GM_ALICE".to_owned();
        assert!(is_gm(&account, &config), "名单匹配大小写不敏感");
    }
}
