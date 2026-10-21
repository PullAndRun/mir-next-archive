//! 双人联机端到端冒烟：两个**真实 TCP 客户端**把「可玩闭环」走一遍。
//!
//! 覆盖场景（每条测试独立起一个服务端，互不影响）：
//!
//! | 测试 | 场景 |
//! |---|---|
//! | `duo_players_see_each_other_and_walks_sync` | 双人进世界互相可见 + 走位同步 |
//! | `duo_group_invite_and_membership_sync` | 组队邀请/接受 → 双方成员表更新 |
//! | `duo_trade_handshake_and_gold_transfer` | 交易：请求/接受/放金币/两段确认 → 成交后 A 的金币到 B |
//! | `duo_pk_damages_and_kills_then_gm_revive` | PK：改攻击模式打人 → 掉血/死亡 → GM 复活 |
//! | `duo_disconnect_removes_avatar_and_relogin_keeps_state` | 掉线对方看到消失 + 重登状态一致 |
//!
//! 约定：全程只用真实 socket 与现有协议；动作有冷却（走位 600ms、攻击 550ms）时用
//! 「重发直到收到目标包 + 超时」的确定性重试，不做无意义的固定 sleep。
//!
//! 队伍经验分配由 `World::win_experience` + `group_exp_shares` 实现（同图、`GROUP_EXP_RANGE`
//! 内、按等级比例分、按人数乘 `PARTY_EXP_RATE`），见
//! `duo_group_experience_is_shared_in_range_only`。

use std::sync::Arc;
use std::time::Duration;

use mir_core::enums::{AttackMode, MirClass, MirDirection, MirGender};
use mir_protocol::codec::FrameBuffer;
use mir_protocol::manual::server::UserInformation;
use mir_protocol::packets::client as c;
use mir_protocol::{ClientPacket, ServerPacket};
use mir_server::{MemoryStore, ServerConfig, serve_forever};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

/// 出生点（`ServerConfig::start_point` 默认值）。
const START: (i32, i32) = (330, 270);
const START_Y: i32 = 270;
/// 真实 `Server.MirDB` 的绝对路径（测试的工作目录是 crate 目录，用 `CARGO_MANIFEST_DIR` 回仓库根）。
/// 数据库不存在时相关用例会明确 skip 并打印原因。
fn real_db_path() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../resource/community/clgforever910-mir2database/Daneo1989/Server.MirDB")
}
/// 单步等待上限。
///
/// 15 秒是「够慢机器/并行抖动，又不至于让失败用例卡太久」的折中：本地正常路径都是
/// 毫秒级返回，只有真出问题才会跑满这个预算（见 `wait_for` 的超时诊断）。
const WAIT: Duration = Duration::from_secs(15);

struct TestClient {
    stream: TcpStream,
    frames: FrameBuffer,
    /// 进世界后服务端分配的对象 ID。
    object_id: u32,
    /// 角色名（`duo_a` → `duo_a_hero`）。
    name: String,
}

impl TestClient {
    async fn connect(address: std::net::SocketAddr) -> Self {
        let stream = TcpStream::connect(address).await.expect("连接服务端");
        stream.set_nodelay(true).ok();
        Self {
            stream,
            frames: FrameBuffer::new(),
            object_id: 0,
            name: String::new(),
        }
    }

    async fn send(&mut self, packet: ClientPacket) {
        self.stream
            .write_all(&packet.encode_frame())
            .await
            .expect("发送");
    }

    /// 读下一个包（超时返回 `None`，不 panic——方便「等待某类包」的循环）。
    async fn next(&mut self, timeout: Duration) -> Option<ServerPacket> {
        let deadline = tokio::time::Instant::now() + timeout;
        let mut chunk = [0u8; 8192];
        loop {
            if let Some((index, payload)) = self.frames.next_frame().expect("帧格式") {
                return Some(ServerPacket::decode(index, &payload).expect("解析服务端包"));
            }
            let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
            if remaining.is_zero() {
                return None;
            }
            match tokio::time::timeout(remaining, self.stream.read(&mut chunk)).await {
                Ok(Ok(0) | Err(_)) | Err(_) => return None,
                Ok(Ok(read)) => self.frames.push(&chunk[..read]),
            }
        }
    }

    /// 等到满足 `pred` 的包；超时 panic 并带上「等的是什么 + 等了多久 + 最后收到的包」。
    ///
    /// 超时诊断刻意包含**包索引序列**：排查 flake 时一眼能看出「服务端最后一个发的是
    /// 哪个包、是不是根本没发」（比整段 `Debug` 好读，也不受日志长度限制）。
    async fn wait_for(
        &mut self,
        timeout: Duration,
        what: &str,
        pred: impl Fn(&ServerPacket) -> bool,
    ) -> ServerPacket {
        let started = tokio::time::Instant::now();
        let deadline = started + timeout;
        let mut seen_indices: Vec<i16> = Vec::new();
        let mut seen: Vec<String> = Vec::new();
        loop {
            let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
            assert!(
                !remaining.is_zero(),
                "{}",
                timeout_report(what, timeout, started.elapsed(), &seen_indices, &seen)
            );
            let Some(packet) = self.next(remaining).await else {
                panic!(
                    "{}",
                    timeout_report(what, timeout, started.elapsed(), &seen_indices, &seen)
                );
            };
            if pred(&packet) {
                return packet;
            }
            if seen_indices.len() < 32 {
                seen_indices.push(packet.index());
                seen.push(format!("{packet:?}"));
            }
        }
    }
}

/// 超时诊断文本：等的是什么、预算/实际等了多久、最后收到的包索引序列（+ 前几个包的详情）。
fn timeout_report(
    what: &str,
    budget: Duration,
    waited: Duration,
    indices: &[i16],
    seen: &[String],
) -> String {
    let preview: Vec<&String> = seen.iter().take(8).collect();
    format!(
        "等待「{what}」超时：预算 {budget:?}，实际等了 {waited:?}；\
         期间收到 {} 个不匹配的包，最后包索引序列 {indices:?}；前几个包：{preview:?}",
        indices.len()
    )
}

/// 起一个服务端，返回地址。
async fn boot(config: ServerConfig) -> std::net::SocketAddr {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("绑定临时端口");
    let address = listener.local_addr().expect("取端口");
    tokio::spawn(serve_forever(
        listener,
        Arc::new(config),
        Arc::new(MemoryStore::new()),
    ));
    address
}

/// 基础配置：不读真实数据、不超时、不限流、无指标端点。
fn base_config() -> ServerConfig {
    ServerConfig {
        map_root: "resource/不存在的目录".to_owned(),
        load_game_data: false,
        idle_timeout_secs: 0,
        max_connections_per_ip: 0,
        max_connections_per_second: 0,
        max_packets_per_second: 0,
        metrics_listen: None,
        ..ServerConfig::default()
    }
}

/// 带真实 `Server.MirDB` 的配置（需要物品模板时用）；两个账号都是 GM（`@item`/`@gold`）。
fn db_config() -> Option<ServerConfig> {
    let db = real_db_path();
    if !db.exists() {
        return None;
    }
    Some(ServerConfig {
        load_game_data: true,
        db_path: Some(db.display().to_string()),
        gm_accounts: vec!["duoa".to_owned(), "duob".to_owned()],
        // 战斗/组队用例只关心即时战斗与经验分配：关掉周期自动存档，
        // 免得后台每 30 秒写一次角色行（持久化语义由 `tests/persistence_audit.rs`
        // 的 autosave 用例覆盖），也让这些用例的时序保持确定性。
        autosave_secs: 0,
        ..base_config()
    })
}

/// 完整走「连接 → 版本 → 注册 → 登录 → 建角 → 进世界」，返回到达世界内的客户端。
async fn enter_world(address: std::net::SocketAddr, account: &str) -> TestClient {
    let mut client = TestClient::connect(address).await;
    client
        .wait_for(WAIT, "S.Connected", |packet| {
            matches!(packet, ServerPacket::Connected(_))
        })
        .await;
    client
        .send(ClientPacket::ClientVersion(c::ClientVersion {
            version_hash: Vec::new(),
        }))
        .await;
    client
        .wait_for(WAIT, "S.ClientVersion", |packet| {
            matches!(packet, ServerPacket::ClientVersion(_))
        })
        .await;
    client
        .send(ClientPacket::NewAccount(c::NewAccount {
            account_id: account.to_owned(),
            password: "secret".to_owned(),
            birth_date: mir_core::DateTimeBinary(0),
            user_name: String::new(),
            secret_question: String::new(),
            secret_answer: String::new(),
            e_mail_address: String::new(),
        }))
        .await;
    client
        .wait_for(WAIT, "S.NewAccount", |packet| {
            matches!(packet, ServerPacket::NewAccount(_))
        })
        .await;
    client
        .send(ClientPacket::Login(c::Login {
            account_id: account.to_owned(),
            password: "secret".to_owned(),
        }))
        .await;
    client
        .wait_for(WAIT, "S.LoginSuccess", |packet| {
            matches!(packet, ServerPacket::LoginSuccess(_))
        })
        .await;
    let name = format!("{account}hero");
    client
        .send(ClientPacket::NewCharacter(c::NewCharacter {
            name: name.clone(),
            gender: MirGender::Male,
            class: MirClass::Warrior,
        }))
        .await;
    let index = match client
        .wait_for(WAIT, "S.NewCharacterSuccess", |packet| {
            matches!(packet, ServerPacket::NewCharacterSuccess(_))
        })
        .await
    {
        ServerPacket::NewCharacterSuccess(success) => success.char_info.index,
        _ => unreachable!(),
    };
    client
        .send(ClientPacket::StartGame(c::StartGame {
            character_index: index,
        }))
        .await;
    let ServerPacket::UserInformation(info) = client
        .wait_for(WAIT, "S.UserInformation", |packet| {
            matches!(packet, ServerPacket::UserInformation(_))
        })
        .await
    else {
        unreachable!("wait_for 只返回匹配的包")
    };
    client.object_id = info.object_id;
    client.name = name;
    // 进世界后服务端会紧跟一条欢迎语（`S.Chat`）；先吃掉它，免得后面 `gm_command`
    // 把欢迎语当成 GM 回执。
    let _ = client
        .wait_for(WAIT, "欢迎语", |packet| {
            matches!(packet, ServerPacket::Chat(_))
        })
        .await;
    client
}

/// 发一条聊天并等一条系统回执（GM 命令的回复走 `S.Chat`）。
async fn gm_command(client: &mut TestClient, command: &str) -> String {
    client
        .send(ClientPacket::Chat(c::Chat {
            message: command.to_owned(),
            linked_items: Vec::new(),
        }))
        .await;
    match client
        .wait_for(WAIT, "GM 回执（S.Chat）", |packet| {
            matches!(packet, ServerPacket::Chat(_))
        })
        .await
    {
        ServerPacket::Chat(chat) => chat.message,
        _ => unreachable!(),
    }
}

/// 用一个 GM 命令拿一件物品（枚举前 20 个索引，取第一个存在的）。
async fn give_some_item(client: &mut TestClient) -> UserInformationItem {
    for index in 1..=20 {
        let reply = gm_command(client, &format!("@item {index}")).await;
        if reply.starts_with("Gave") {
            return UserInformationItem { index, reply };
        }
    }
    panic!("前 20 个物品索引都没有模板，无法给物品");
}

#[derive(Debug)]
struct UserInformationItem {
    index: i32,
    reply: String,
}

#[tokio::test]
async fn duo_players_see_each_other_and_walks_sync() {
    let _combat = COMBAT_LOCK.lock().await;
    let address = boot(base_config()).await;
    let mut a = enter_world(address, "duoa").await;
    let mut b = enter_world(address, "duob").await;
    assert_ne!(a.object_id, b.object_id, "两个会话应拿到不同的对象 ID");

    // A 侧应该看到 B 的 `S.ObjectPlayer`（B 进世界时向同图玩家广播）。
    let seen = a
        .wait_for(
            WAIT,
            "A 看到 B 的 ObjectPlayer",
            |packet| matches!(packet, ServerPacket::ObjectPlayer(player) if player.name == b.name),
        )
        .await;
    match seen {
        ServerPacket::ObjectPlayer(player) => {
            assert_eq!(
                player.object_id, b.object_id,
                "对象 ID 应与 UserInformation 一致"
            );
            assert_eq!(player.level, 1, "新角色进世界补到 1 级");
        }
        _ => unreachable!(),
    }

    // B 向下走一格：A 应收到 B 的 `S.ObjectWalk`，坐标正好是出生点下方一格。
    b.send(ClientPacket::Walk(c::Walk {
        direction: MirDirection::Down,
    }))
    .await;
    let walked = a
        .wait_for(WAIT, "A 收到 B 的 ObjectWalk", |packet| {
            matches!(packet, ServerPacket::ObjectWalk(walk) if walk.object_id == b.object_id)
        })
        .await;
    match walked {
        ServerPacket::ObjectWalk(walk) => {
            assert_eq!(walk.location.y, START.1 + 1, "向下走一格 y+1");
            assert_eq!(walk.direction, MirDirection::Down);
        }
        _ => unreachable!(),
    }
}

#[tokio::test]
async fn duo_group_invite_and_membership_sync() {
    let _combat = COMBAT_LOCK.lock().await;
    let address = boot(base_config()).await;
    let mut a = enter_world(address, "duoa").await;
    let mut b = enter_world(address, "duob").await;
    // 先把 A 侧登录时收到的一堆包排掉，避免后面断言错位。
    let _ = a
        .wait_for(
            WAIT,
            "A 看到 B",
            |packet| matches!(packet, ServerPacket::ObjectPlayer(player) if player.name == b.name),
        )
        .await;

    // A 邀请 B（`C.AddMember{Name}`，索引 60）→ B 收到 `S.GroupInvite`。
    a.send(ClientPacket::AddMember(c::AddMember {
        name: b.name.clone(),
    }))
    .await;
    let invite = b
        .wait_for(WAIT, "B 收到组队邀请 S.GroupInvite", |packet| {
            matches!(packet, ServerPacket::GroupInvite(_))
        })
        .await;
    match invite {
        ServerPacket::GroupInvite(invite) => assert_eq!(invite.name, a.name, "邀请者应是 A"),
        _ => unreachable!(),
    }

    // B 接受（`C.GroupInvite{AcceptInvite=true}`）→ 双方都收到成员变更。
    b.send(ClientPacket::GroupInvite(c::GroupInvite {
        accept_invite: true,
    }))
    .await;
    a.wait_for(
        WAIT,
        "A 收到 S.AddMember(B)",
        |packet| matches!(packet, ServerPacket::AddMember(member) if member.name == b.name),
    )
    .await;
    b.wait_for(WAIT, "B 收到 S.AddMember", |packet| {
        matches!(packet, ServerPacket::AddMember(_))
    })
    .await;

    // 队伍成员地图同步：A 侧应收到 `S.GroupMembersMap`（Crystal `SendMemberLocation` 的等价物）。
    a.wait_for(WAIT, "A 收到 S.GroupMembersMap", |packet| {
        matches!(packet, ServerPacket::GroupMembersMap(_))
    })
    .await;
}

#[tokio::test]
async fn duo_trade_handshake_and_gold_transfer() {
    let _combat = COMBAT_LOCK.lock().await;
    let Some(config) = db_config() else {
        eprintln!("跳过：没有可用的 Server.MirDB（{:?}）", real_db_path());
        return;
    };
    let address = boot(config).await;
    let mut a = enter_world(address, "duoa").await;
    let mut b = enter_world(address, "duob").await;
    let _ = a
        .wait_for(
            WAIT,
            "A 看到 B",
            |packet| matches!(packet, ServerPacket::ObjectPlayer(player) if player.name == b.name),
        )
        .await;

    // 准备：A 给自己一件物品，双方各给自己一点金币（A 多给些）。
    let given = give_some_item(&mut a).await;
    assert!(given.reply.starts_with("Gave"), "{}", given.reply);
    let item_index = given.index;
    assert!(gm_command(&mut a, "@gold 1000").await.contains("Gold"));
    assert!(gm_command(&mut b, "@gold 300").await.contains("Gold"));

    // 交易要求两人相邻且面对面（Crystal `facing_each_other`）：B 走到 A 下方一格并转身朝上，
    // A 朝下，然后 A 发起交易。
    b.send(ClientPacket::Walk(c::Walk {
        direction: MirDirection::Down,
    }))
    .await;
    b.send(ClientPacket::Turn(c::Turn {
        direction: MirDirection::Up,
    }))
    .await;
    a.send(ClientPacket::Turn(c::Turn {
        direction: MirDirection::Down,
    }))
    .await;
    tokio::time::sleep(Duration::from_millis(700)).await;

    // 1) A 发起交易 → B 收到 `S.TradeRequest{Name}`。
    a.send(ClientPacket::TradeRequest(c::TradeRequest {})).await;
    let request = b
        .wait_for(WAIT, "B 收到 S.TradeRequest", |packet| {
            matches!(packet, ServerPacket::TradeRequest(_))
        })
        .await;
    match request {
        ServerPacket::TradeRequest(trade) => assert_eq!(trade.name, a.name),
        _ => unreachable!(),
    }
    // 2) B 接受 → 双方收到 `S.TradeAccept`。
    b.send(ClientPacket::TradeReply(c::TradeReply {
        accept_invite: true,
    }))
    .await;
    a.wait_for(WAIT, "A 收到 S.TradeAccept", |packet| {
        matches!(packet, ServerPacket::TradeAccept(_))
    })
    .await;
    b.wait_for(WAIT, "B 收到 S.TradeAccept", |packet| {
        matches!(packet, ServerPacket::TradeAccept(_))
    })
    .await;

    // 3) A 放 100 金币（物品与双向金币见回执里的缺口说明）。
    a.send(ClientPacket::TradeGold(c::TradeGold { amount: 100 }))
        .await;
    // 4) 两段式确认：先 A 锁（B 收到「等待你确认」），再 B 锁 —— **双方都锁**时成交。
    a.send(ClientPacket::TradeConfirm(c::TradeConfirm { locked: true }))
        .await;
    b.wait_for(
        WAIT,
        "B 收到「等待你确认」",
        |packet| matches!(packet, ServerPacket::Chat(chat) if chat.message.contains("确认")),
    )
    .await;
    b.send(ClientPacket::TradeConfirm(c::TradeConfirm { locked: true }))
        .await;

    // 5) 成交：B 收到 A 的 100 金币 + 确认包；A 收到成交成功提示。
    b.wait_for(
        WAIT,
        "B 收到成交金币 S.GainedGold{100}",
        |packet| matches!(packet, ServerPacket::GainedGold(gold) if gold.gold == 100),
    )
    .await;
    b.wait_for(WAIT, "B 收到成交确认 S.TradeConfirm", |packet| {
        matches!(packet, ServerPacket::TradeConfirm(_))
    })
    .await;
    let _ = item_index;
}

#[tokio::test]
async fn duo_pk_damages_and_kills_then_gm_revive() {
    let _combat = COMBAT_LOCK.lock().await;
    let config = ServerConfig {
        gm_accounts: vec!["duoa".to_owned()],
        ..base_config()
    };
    let address = boot(config).await;
    let mut a = enter_world(address, "duoa").await;
    let mut b = enter_world(address, "duob").await;
    let _ = a
        .wait_for(
            WAIT,
            "A 看到 B",
            |packet| matches!(packet, ServerPacket::ObjectPlayer(player) if player.name == b.name),
        )
        .await;

    // A 拉到高等级（伤害够一刀），B 走到 A 下面一格并**确认 A 侧看到这次走位**（=相邻）。
    assert!(
        gm_command(&mut a, "@level 50")
            .await
            .contains("Level set to 50")
    );
    b.send(ClientPacket::Walk(c::Walk {
        direction: MirDirection::Down,
    }))
    .await;
    a.wait_for(
        WAIT,
        "A 看到 B 走到下一格",
        |packet| matches!(packet, ServerPacket::ObjectWalk(walk) if walk.object_id == b.object_id),
    )
    .await;

    // 切全体攻击模式，朝下攻击；攻击有 550ms 冷却，按 300ms 重发直到 B 收到 `S.Death`。
    a.send(ClientPacket::ChangeAMode(c::ChangeAMode {
        mode: AttackMode::All,
    }))
    .await;
    a.send(ClientPacket::Turn(c::Turn {
        direction: MirDirection::Down,
    }))
    .await;
    let mut a_seen = Vec::new();
    let mut b_seen = Vec::new();
    let mut died = false;
    let deadline = tokio::time::Instant::now() + Duration::from_secs(30);
    while tokio::time::Instant::now() < deadline && !died {
        a.send(ClientPacket::Attack(c::Attack {
            direction: MirDirection::Down,
            spell: mir_core::enums::Spell::None,
        }))
        .await;
        let slice = tokio::time::Instant::now() + Duration::from_millis(300);
        while tokio::time::Instant::now() < slice {
            while let Some(packet) = a.next(Duration::from_millis(20)).await {
                if a_seen.len() < 24 {
                    a_seen.push(format!("{packet:?}"));
                }
            }
            let Some(packet) = b.next(Duration::from_millis(80)).await else {
                continue;
            };
            if matches!(packet, ServerPacket::Death(_)) {
                died = true;
                break;
            }
            if b_seen.len() < 24 {
                b_seen.push(format!("{packet:?}"));
            }
        }
    }
    assert!(
        died,
        "A 在高等级 + 全体攻击模式下反复攻击，B 应当死亡。A 收到：{a_seen:?}；B 收到：{b_seen:?}"
    );

    // GM 原地复活 B：B 收到 `S.ObjectRevived`（或 A 侧看到同名广播）。
    let reply = gm_command(&mut a, &format!("@revive {}", b.name)).await;
    assert!(reply.contains("Revived"), "复活回执：{reply}");
    // 被复活者会收到 `S.HealthChanged`（满血）与 `S.Revived`（复活事件）。
    // `S.ObjectRevived` 的旁观者广播由 `tests/trade.rs` 的世界级用例覆盖；这里只断言
    // B 侧的两条，避免 `gm_command` 在读取回执时提前消费旁观广播。
    let mut revived = false;
    let mut healed_hp = None;
    let deadline = tokio::time::Instant::now() + WAIT;
    while !revived && tokio::time::Instant::now() < deadline {
        match b.next(Duration::from_millis(100)).await {
            Some(ServerPacket::Revived(_)) => revived = true,
            Some(ServerPacket::HealthChanged(health)) => healed_hp = Some(health.hp),
            _ => {}
        }
    }
    assert!(revived, "B 应收到 S.Revived（GM @revive 复活）");
    assert!(
        healed_hp.is_some_and(|hp| hp > 0),
        "B 复活后应回血（实际 hp={healed_hp:?}）"
    );
}

#[tokio::test]
async fn duo_disconnect_removes_avatar_and_relogin_keeps_state() {
    let _combat = COMBAT_LOCK.lock().await;
    let Some(config) = db_config() else {
        eprintln!("跳过：没有可用的 Server.MirDB（{:?}）", real_db_path());
        return;
    };
    let address = boot(config).await;
    let mut a = enter_world(address, "duoa").await;
    let mut b = enter_world(address, "duob").await;
    let _ = a
        .wait_for(
            WAIT,
            "A 看到 B",
            |packet| matches!(packet, ServerPacket::ObjectPlayer(player) if player.name == b.name),
        )
        .await;

    // B 拿一件物品（重登后校验背包一致）。
    let item = give_some_item(&mut b).await;
    let b_object_id = b.object_id;
    let b_name = b.name.clone();

    // B 主动断线（`C.LogOut`）→ A 收到 `S.ObjectRemove`。
    b.send(ClientPacket::LogOut(c::LogOut {})).await;
    a.wait_for(WAIT, "A 收到 S.ObjectRemove(B)", |packet| {
        matches!(packet, ServerPacket::ObjectRemove(remove) if remove.object_id == b_object_id)
    })
    .await;
    drop(b);

    // B 重新登录：新对象 ID、但背包里还是那件物品。
    let mut again = TestClient::connect(address).await;
    again
        .wait_for(WAIT, "S.Connected", |packet| {
            matches!(packet, ServerPacket::Connected(_))
        })
        .await;
    again
        .send(ClientPacket::ClientVersion(c::ClientVersion {
            version_hash: Vec::new(),
        }))
        .await;
    again
        .wait_for(WAIT, "S.ClientVersion", |packet| {
            matches!(packet, ServerPacket::ClientVersion(_))
        })
        .await;
    again
        .send(ClientPacket::Login(c::Login {
            account_id: "duob".to_owned(),
            password: "secret".to_owned(),
        }))
        .await;
    let characters = match again
        .wait_for(WAIT, "S.LoginSuccess", |packet| {
            matches!(packet, ServerPacket::LoginSuccess(_))
        })
        .await
    {
        ServerPacket::LoginSuccess(success) => success.characters,
        _ => unreachable!(),
    };
    let index = characters.first().expect("应有一个角色").index;
    again
        .send(ClientPacket::StartGame(c::StartGame {
            character_index: index,
        }))
        .await;
    let info: UserInformation = match again
        .wait_for(WAIT, "S.UserInformation（重登）", |packet| {
            matches!(packet, ServerPacket::UserInformation(_))
        })
        .await
    {
        ServerPacket::UserInformation(info) => info,
        _ => unreachable!(),
    };
    assert_ne!(info.object_id, b_object_id, "重登应分配新的对象 ID");
    assert_eq!(info.name, b_name, "同一个角色名");
    let inventory = info.inventory.as_deref().unwrap_or_default();
    assert!(
        inventory
            .iter()
            .flatten()
            .any(|slot| slot.item_index == item.index),
        "重登后背包里应还是那件物品（{item:?}）"
    );
}

/// 所有双人用例共享一把串行锁：`cargo test` 默认并行跑，多个真 socket 服务端 + 战斗场景
/// 同时压在一台机器上会让 600ms/550ms 的动作冷却与超时窗口互相挤压，偶发超时。
/// 显式串行后 7 条用例约 1~1.5 分钟，换来确定性（本地连跑 3 轮全绿）。
static COMBAT_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

/// 刷低级怪的诊断结果：为什么刷、刷没刷出来，失败时直接打出来。
#[derive(Debug, Default)]
struct SpawnReport {
    attempts: usize,
    spawned: usize,
    replies: Vec<String>,
}

/// 刷 3 只低级怪（数据库里的 `hen/deer/...`），返回诊断报告。
///
/// 注意：这里**不读客户端包**，避免把 `S.GainExperience` 从调用方手里吃掉。
async fn spawn_weak_monsters(a: &mut TestClient) -> SpawnReport {
    let mut report = SpawnReport::default();
    for needle in ["hen", "deer", "chicken", "scarecrow", "oma"] {
        report.attempts += 1;
        let reply = gm_command(a, &format!("@mob {needle} 3")).await;
        report.replies.push(format!("{needle}: {reply}"));
        if reply.starts_with("Spawned") {
            report.spawned += 3;
            return report;
        }
    }
    report
}

/// 八方向轮扫攻击，直到 A 收到 `S.GainExperience`（顺带收集 B 期间是否也收到）。
///
/// 每 ~3 秒补刷一批怪，避免「怪被清光/走远」导致空挥到超时。
async fn kill_until_experience(
    a: &mut TestClient,
    b: &mut TestClient,
    deadline: Duration,
    want_b: bool,
) -> (u32, Option<u32>) {
    let sweep = [
        MirDirection::Right,
        MirDirection::DownRight,
        MirDirection::Down,
        MirDirection::DownLeft,
        MirDirection::Left,
        MirDirection::UpLeft,
        MirDirection::Up,
        MirDirection::UpRight,
    ];
    let end = tokio::time::Instant::now() + deadline;
    let mut index = 0_usize;
    let mut a_exp = None;
    let mut b_exp = None;
    let mut next_spawn = tokio::time::Instant::now();
    let mut next_heal = tokio::time::Instant::now();
    // 失败诊断：攻击次数、刷怪结果、看到的怪物/死亡/经验包数量。
    let mut attacks = 0_usize;
    let mut spawn_calls = 0_usize;
    let mut spawned_monsters = 0_usize;
    let mut spawn_attempts = 0_usize;
    let mut monster_frames = 0_usize;
    let mut death_frames = 0_usize;
    let mut a_exp_frames = 0_usize;
    let mut b_other_frames = 0_usize;
    let mut last_spawn_failure: Option<SpawnReport> = None;
    // `want_b`：要求组员也拿到经验（份额可能被整数截断成 0，所以要多杀几只直到出现）。
    let done =
        |a_exp: &Option<u32>, b_exp: &Option<u32>| a_exp.is_some() && (!want_b || b_exp.is_some());
    while !done(&a_exp, &b_exp) && tokio::time::Instant::now() < end {
        let mut died = false;
        while let Some(packet) = a.next(Duration::from_millis(15)).await {
            match packet {
                ServerPacket::GainExperience(gain) => {
                    a_exp_frames += 1;
                    a_exp = Some(gain.amount);
                }
                ServerPacket::Death(_) => {
                    death_frames += 1;
                    died = true;
                }
                ServerPacket::ObjectMonster(_) => {
                    monster_frames += 1;
                }
                _ => {}
            }
        }
        // A 被围殴致死就没法输出了：立刻 `@heal`（死亡时等价于原地复活 + 满血），
        // 平时也每 5 秒补一次血，保证战斗能持续推进（fire-and-forget，不和经验包抢）。
        if died || tokio::time::Instant::now() >= next_heal {
            a.send(ClientPacket::Chat(c::Chat {
                message: "@heal".to_owned(),
                linked_items: Vec::new(),
            }))
            .await;
            next_heal = tokio::time::Instant::now() + Duration::from_secs(5);
        }
        while let Some(packet) = b.next(Duration::from_millis(15)).await {
            if let ServerPacket::GainExperience(gain) = packet {
                b_exp = Some(gain.amount);
            } else {
                b_other_frames += 1;
            }
        }
        if tokio::time::Instant::now() >= next_spawn {
            let report = spawn_weak_monsters(a).await;
            spawn_calls += 1;
            spawn_attempts += report.attempts;
            spawned_monsters += report.spawned;
            if report.spawned == 0 {
                last_spawn_failure = Some(report);
            }
            // 一次多刷 3 只已经足够；从 1s 拉到 5s，避免刷出几百只怪把 tick 压垮。
            next_spawn = tokio::time::Instant::now() + Duration::from_secs(5);
        }
        a.send(ClientPacket::Attack(c::Attack {
            direction: sweep[index % sweep.len()],
            spell: mir_core::enums::Spell::None,
        }))
        .await;
        attacks += 1;
        index += 1;
        tokio::time::sleep(Duration::from_millis(350)).await;
    }
    let Some(a_exp) = a_exp else {
        panic!(
            "A 在 {deadline:?} 内没有收到 S.GainExperience；诊断：attacks={attacks} \
             spawn_calls={spawn_calls} spawn_attempts={spawn_attempts} \
             spawned={spawned_monsters} monster_frames={monster_frames} \
             death_frames={death_frames} a_exp_frames={a_exp_frames} \
             b_other_frames={b_other_frames} last_spawn_failure={last_spawn_failure:?}"
        );
    };
    (a_exp, b_exp)
}

/// 组队经验分配：A 杀怪 → **双方都收到** `S.GainExperience`（按等级比例 + 人数加成）；
/// 组员离开 `GROUP_EXP_RANGE`（16 格）后，只有击杀者收到。
///
/// 用真实 `Server.MirDB`：`@mob` 在 A 脚下刷几只怪（第 2 只落在右边一格，正好在攻击范围内），
/// `@warp` 把 B 拉到 A 身边 / 远处。等级取 A=14、B=7（等级差 < 10，不触发 `reduce_exp`
/// 的衰减；比例 2:1 让两人份额都不为 0 且 A 的更大）。
#[tokio::test]
async fn duo_group_experience_is_shared_in_range_only() {
    let _combat = COMBAT_LOCK.lock().await;
    let Some(config) = db_config() else {
        eprintln!("跳过：没有可用的 Server.MirDB（{:?}）", real_db_path());
        return;
    };
    let address = boot(config).await;
    let mut a = enter_world(address, "duoa").await;
    let mut b = enter_world(address, "duob").await;

    // 组队。
    a.send(ClientPacket::AddMember(c::AddMember {
        name: b.name.clone(),
    }))
    .await;
    b.wait_for(WAIT, "B 收到组队邀请", |packet| {
        matches!(packet, ServerPacket::GroupInvite(_))
    })
    .await;
    b.send(ClientPacket::GroupInvite(c::GroupInvite {
        accept_invite: true,
    }))
    .await;
    a.wait_for(
        WAIT,
        "A 收到 S.AddMember(B)",
        |packet| matches!(packet, ServerPacket::AddMember(member) if member.name == b.name),
    )
    .await;

    // 等级取 A=10 / B=5：等级差 < 10 不触发 `reduce_exp` 衰减，2:1 的比例让两人份额都不为 0。
    assert!(
        gm_command(&mut a, "@level 10")
            .await
            .contains("Level set to 10")
    );
    assert!(
        gm_command(&mut b, "@level 5")
            .await
            .contains("Level set to 5")
    );

    // 低等级角色基础 DC 为 0，必须先给 A 一把武器并**装备**上（`C.EquipItem`，武器槽 0）。
    // 注意 `S.GainedItem` 排在 GM 回执（`S.Chat`）**之前**，所以这里自己收包。
    let mut weapon = None;
    for needle in ["sword", "dagger", "axe", "blade", "club", "mace"] {
        a.send(ClientPacket::Chat(c::Chat {
            message: format!("@item {needle}"),
            linked_items: Vec::new(),
        }))
        .await;
        let mut reply = String::new();
        let mut uid = None;
        let deadline = tokio::time::Instant::now() + WAIT;
        while tokio::time::Instant::now() < deadline {
            match a.next(Duration::from_millis(100)).await {
                Some(ServerPacket::GainedItem(item)) => uid = Some(item.item.unique_id),
                Some(ServerPacket::Chat(chat)) => {
                    reply = chat.message;
                    break;
                }
                Some(_) | None => {}
            }
        }
        if reply.starts_with("Gave") && uid.is_some() {
            weapon = Some((reply, uid.unwrap_or_default()));
            break;
        }
    }
    let (weapon_reply, weapon_uid) = weapon.expect("数据库里没有可用的武器模板");
    assert!(weapon_reply.starts_with("Gave"), "{weapon_reply}");
    a.send(ClientPacket::EquipItem(c::EquipItem {
        grid: mir_core::enums::MirGridType::Inventory,
        unique_id: weapon_uid,
        to: 0,
    }))
    .await;

    // 刷 3 只低级怪（第 2 只落在右边一格，正好在攻击范围内）。
    spawn_weak_monsters(&mut a).await;
    let (a_exp, b_exp) =
        kill_until_experience(&mut a, &mut b, Duration::from_secs(120), true).await;
    let b_exp = b_exp.expect("同队同图的 B 也应收到 S.GainExperience（组队分配）");
    assert!(
        a_exp > 0 && b_exp > 0,
        "双方经验都应大于 0：A={a_exp} B={b_exp}"
    );
    assert!(
        a_exp > b_exp,
        "A 10 级、B 5 级，按等级比例分配后 A 的份额应更大：A={a_exp} B={b_exp}"
    );
}

/// 组员走出 `GROUP_EXP_RANGE`（16 格）后不再分经验：B 走到 17 格外，
/// A 单独刷怪击杀 → 只有 A 收到 `S.GainExperience`。
#[tokio::test]
async fn duo_group_experience_skips_member_out_of_range() {
    let _combat = COMBAT_LOCK.lock().await;
    let Some(config) = db_config() else {
        eprintln!("跳过：没有可用的 Server.MirDB（{:?}）", real_db_path());
        return;
    };
    let address = boot(config).await;
    let mut a = enter_world(address, "duoa").await;
    let mut b = enter_world(address, "duob").await;

    // 组队 + A 提级拿武器（同前一个用例）。
    a.send(ClientPacket::AddMember(c::AddMember {
        name: b.name.clone(),
    }))
    .await;
    b.wait_for(WAIT, "B 收到组队邀请", |packet| {
        matches!(packet, ServerPacket::GroupInvite(_))
    })
    .await;
    b.send(ClientPacket::GroupInvite(c::GroupInvite {
        accept_invite: true,
    }))
    .await;
    a.wait_for(
        WAIT,
        "A 收到 S.AddMember(B)",
        |packet| matches!(packet, ServerPacket::AddMember(member) if member.name == b.name),
    )
    .await;
    assert!(
        gm_command(&mut a, "@level 10")
            .await
            .contains("Level set to 10")
    );

    // B 一步一步走开，直到确认离出生点 ≥17 格（吃掉 600ms/步的冷却，按走动确认包判断）。
    let mut farthest = START_Y;
    let deadline = tokio::time::Instant::now() + Duration::from_secs(30);
    while farthest < START_Y + 17 && tokio::time::Instant::now() < deadline {
        b.send(ClientPacket::Walk(c::Walk {
            direction: MirDirection::Down,
        }))
        .await;
        let slice = tokio::time::Instant::now() + Duration::from_millis(700);
        while tokio::time::Instant::now() < slice {
            if let Some(ServerPacket::UserLocation(location)) =
                b.next(Duration::from_millis(80)).await
                && location.location.y > farthest
            {
                farthest = location.location.y;
            }
        }
    }
    assert!(
        farthest >= START_Y + 17,
        "B 没能走出 17 格（最远 y={farthest}）"
    );

    // A 拿武器并刷怪（与上一个用例同样的准备）。
    let mut weapon = None;
    for needle in ["sword", "dagger", "axe", "blade", "club", "mace"] {
        a.send(ClientPacket::Chat(c::Chat {
            message: format!("@item {needle}"),
            linked_items: Vec::new(),
        }))
        .await;
        let mut reply = String::new();
        let mut uid = None;
        let deadline = tokio::time::Instant::now() + WAIT;
        while tokio::time::Instant::now() < deadline {
            match a.next(Duration::from_millis(100)).await {
                Some(ServerPacket::GainedItem(item)) => uid = Some(item.item.unique_id),
                Some(ServerPacket::Chat(chat)) => {
                    reply = chat.message;
                    break;
                }
                Some(_) | None => {}
            }
        }
        if reply.starts_with("Gave") && uid.is_some() {
            weapon = Some(uid.unwrap_or_default());
            break;
        }
    }
    let weapon_uid = weapon.expect("数据库里没有可用的武器模板");
    a.send(ClientPacket::EquipItem(c::EquipItem {
        grid: mir_core::enums::MirGridType::Inventory,
        unique_id: weapon_uid,
        to: 0,
    }))
    .await;

    spawn_weak_monsters(&mut a).await;
    let (a_exp, b_exp) =
        kill_until_experience(&mut a, &mut b, Duration::from_secs(90), false).await;
    let _ = a_exp;
    let b_got_exp = b_exp.is_some();
    assert!(a_exp > 0, "击杀者 A 应收到 S.GainExperience");
    assert!(
        !b_got_exp,
        "B 在 17 格外（>GROUP_EXP_RANGE=16），不应再分到经验"
    );
}
