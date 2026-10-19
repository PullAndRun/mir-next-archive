//! NPC 输入框（`C.NPCConfirmInput`）的端到端测试。
//!
//! 背景：`GIVENAMELIST` 这类动作用 `S.NPCRequestInput` 让客户端弹「可编辑输入框」
//! （Crystal `NPCScript.cs:863-868` 的 `[@@"`），玩家确认后客户端发 `C.NPCConfirmInput`；
//! 服务端把**输入值当成按钮 key** 再走一次 `CallNPC`。
//!
//! 这里用真实 `resource/` 数据集（`GameData::discover()`：`Server.MirDB` + `Envir/NPCs` 脚本 +
//! `resource/map/0.map` 地形）起一个真服务端，验证：
//!
//! 1. `C.CallNPC{key}` 与 `C.NPCConfirmInput{value}` 走到**同一个页面**（输入路径 == 按钮路径）；
//! 2. 输入框回执里敲一个当前页没有的名字会被拒绝（不会跑到别的页）。
//!
//! 数据集缺失时整条用例 skip 并打印原因（与 `tests/npc.rs` 的真实数据用例同一口径）。

use std::sync::Arc;
use std::time::Duration;

use mir_core::enums::{MirClass, MirGender};
use mir_protocol::codec::FrameBuffer;
use mir_protocol::packets::client as c;
use mir_protocol::{ClientPacket, ServerPacket};
use mir_server::{MemoryStore, ServerConfig, serve_forever};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

const WAIT: Duration = Duration::from_secs(15);

/// 一个 NPC 的位置与对象 ID（`S.NewMapInfo` 与 `S.ObjectNPC` 两种来源归一到这里）。
#[derive(Debug, Clone)]
struct NpcSpot {
    object_id: u32,
    name: String,
    location: mir_core::Point,
}

struct TestClient {
    stream: TcpStream,
    frames: FrameBuffer,
}

impl TestClient {
    async fn connect(address: std::net::SocketAddr) -> Self {
        let stream = TcpStream::connect(address).await.expect("连接服务端");
        stream.set_nodelay(true).ok();
        Self {
            stream,
            frames: FrameBuffer::new(),
        }
    }

    async fn send(&mut self, packet: ClientPacket) {
        self.stream
            .write_all(&packet.encode_frame())
            .await
            .expect("发送");
    }

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

    async fn wait_for(&mut self, what: &str, pred: impl Fn(&ServerPacket) -> bool) -> ServerPacket {
        let deadline = tokio::time::Instant::now() + WAIT;
        let mut seen = Vec::new();
        while tokio::time::Instant::now() < deadline {
            let Some(packet) = self.next(Duration::from_millis(200)).await else {
                continue;
            };
            if pred(&packet) {
                return packet;
            }
            if seen.len() < 16 {
                seen.push(format!("{packet:?}"));
            }
        }
        panic!("等待「{what}」超时；期间收到：{seen:?}");
    }
}

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

/// 走完登录流程进入世界，返回客户端与进图时的 NPC 列表。
async fn enter_world(address: std::net::SocketAddr, account: &str) -> (TestClient, Vec<NpcSpot>) {
    let mut client = TestClient::connect(address).await;
    client
        .wait_for("S.Connected", |packet| {
            matches!(packet, ServerPacket::Connected(_))
        })
        .await;
    client
        .send(ClientPacket::ClientVersion(c::ClientVersion {
            version_hash: vec![0u8; 16],
        }))
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
        .wait_for("S.NewAccount", |packet| {
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
        .wait_for("S.LoginSuccess", |packet| {
            matches!(packet, ServerPacket::LoginSuccess(_))
        })
        .await;
    client
        .send(ClientPacket::NewCharacter(c::NewCharacter {
            name: "输入框测试".to_owned(),
            gender: MirGender::Male,
            class: MirClass::Warrior,
        }))
        .await;
    let index = match client
        .wait_for("S.NewCharacterSuccess", |packet| {
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

    // 进世界后服务端连发一串包（`S.UserInformation`、`S.NewMapInfo`、`S.ObjectNPC`、任务列表…），
    // 顺序不固定，所以这里**统一收**：一旦拿到 `UserInformation` 再多收 2 秒补齐 NPC 列表。
    let mut got_user = false;
    let mut grace_until: Option<tokio::time::Instant> = None;
    let mut npcs: Vec<NpcSpot> = Vec::new();
    let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
    while tokio::time::Instant::now() < deadline {
        let grace_done = grace_until.is_none_or(|at| tokio::time::Instant::now() >= at);
        if got_user && !npcs.is_empty() && grace_done {
            break;
        }
        let Some(packet) = client.next(Duration::from_millis(200)).await else {
            continue;
        };
        match packet {
            ServerPacket::UserInformation(_) => {
                got_user = true;
                grace_until = Some(tokio::time::Instant::now() + Duration::from_secs(2));
            }
            ServerPacket::NewMapInfo(info) => {
                for npc in &info.info.npcs {
                    npcs.push(NpcSpot {
                        object_id: npc.object_id,
                        name: npc.name.clone(),
                        location: npc.location,
                    });
                }
            }
            ServerPacket::ObjectNPC(npc) => {
                let known = npcs.iter().any(|spot| spot.object_id == npc.object_id);
                if !known {
                    npcs.push(NpcSpot {
                        object_id: npc.object_id,
                        name: npc.name,
                        location: npc.location,
                    });
                }
            }
            _ => {}
        }
    }
    assert!(got_user, "应当进入世界（S.UserInformation）");
    (client, npcs)
}

/// 从 `#SAY` 渲染文本里抠出第一个按钮 key（`<买卖/@BuySell>` → `@BuySell`）。
fn first_button_key(page: &[String]) -> Option<String> {
    for line in page {
        let Some(start) = line.find("/@") else {
            continue;
        };
        let rest = &line[start + 2..];
        let end = rest.find(['>', ' ']).unwrap_or(rest.len());
        if end == 0 {
            continue;
        }
        return Some(rest[..end].to_owned());
    }
    None
}

fn page_lines(packet: &ServerPacket) -> Option<Vec<String>> {
    match packet {
        ServerPacket::NPCResponse(response) => Some(response.page.clone()),
        _ => None,
    }
}

#[tokio::test]
async fn npc_confirm_input_reaches_the_same_page_as_the_button() {
    let Some(config) = config_with_real_data() else {
        eprintln!(
            "跳过：resource/ 里没有可用的真实数据集（Server.MirDB + Envir/NPCs + map/0.map）"
        );
        return;
    };
    let address = boot(config).await;
    let (mut client, npcs) = enter_world(address, "npcinput").await;
    assert!(!npcs.is_empty(), "比奇省应该刷出 NPC");

    // 逐个试 NPC：把玩家用 GM `@map` 挪到 NPC 旁边，再点 `@MAIN`，直到某个 NPC 有可用主页。
    let mut npc = None;
    let mut main_page = Vec::new();
    for candidate in npcs.iter().take(12) {
        let at = candidate.location;
        for offset in [1, -1, 2] {
            let command = format!("@map 0 {} {}", at.x, at.y + offset);
            client
                .send(ClientPacket::Chat(c::Chat {
                    message: command,
                    linked_items: Vec::new(),
                }))
                .await;
        }
        client
            .send(ClientPacket::CallNPC(c::CallNPC {
                object_id: candidate.object_id,
                key: "@MAIN".to_owned(),
            }))
            .await;
        if let Some(page) = client
            .next(Duration::from_millis(600))
            .await
            .as_ref()
            .and_then(page_lines)
        {
            if !page.is_empty() {
                npc = Some(candidate.clone());
                main_page = page;
                break;
            }
        }
    }
    let npc = npc.expect("至少有一个 NPC 的 @MAIN 页能出内容");
    eprintln!("使用 NPC {}（object_id={}）", npc.name, npc.object_id);
    let key = first_button_key(&main_page).expect("主页里应至少有一个按钮");

    // 1) 按钮路径：`C.CallNPC{key}`。
    client
        .send(ClientPacket::CallNPC(c::CallNPC {
            object_id: npc.object_id,
            key: key.clone(),
        }))
        .await;
    let button_page = client
        .wait_for("按钮路径的 S.NPCResponse", |packet| {
            matches!(packet, ServerPacket::NPCResponse(_))
        })
        .await;
    let button_page = page_lines(&button_page).unwrap_or_default();

    // 回到主页（`@MAIN` 永远允许）。
    client
        .send(ClientPacket::CallNPC(c::CallNPC {
            object_id: npc.object_id,
            key: "@MAIN".to_owned(),
        }))
        .await;
    let _ = client
        .wait_for("回到主页", |packet| {
            matches!(packet, ServerPacket::NPCResponse(_))
        })
        .await;

    // 2) 输入框路径：`C.NPCConfirmInput{value}` —— 值被当成按钮 key 再走一次 `CallNPC`。
    let value = key.trim_start_matches('@').to_owned();
    client
        .send(ClientPacket::NPCConfirmInput(c::NPCConfirmInput {
            npcid: npc.object_id,
            page_name: "[@MAIN]".to_owned(),
            value: value.clone(),
        }))
        .await;
    let input_page = client
        .wait_for("输入框路径的 S.NPCResponse", |packet| {
            matches!(packet, ServerPacket::NPCResponse(_))
        })
        .await;
    let input_page = page_lines(&input_page).unwrap_or_default();
    assert_eq!(
        input_page, button_page,
        "输入框回执 `{value}` 应走到与按钮 `{key}` 相同的页面"
    );

    // 3) 敲一个当前页没有的名字：被拒绝，不会再出一页新内容。
    client
        .send(ClientPacket::CallNPC(c::CallNPC {
            object_id: npc.object_id,
            key: "@MAIN".to_owned(),
        }))
        .await;
    let _ = client
        .wait_for("回到主页", |packet| {
            matches!(packet, ServerPacket::NPCResponse(_))
        })
        .await;
    client
        .send(ClientPacket::NPCConfirmInput(c::NPCConfirmInput {
            npcid: npc.object_id,
            page_name: "[@MAIN]".to_owned(),
            value: "ZZ_NO_SUCH_PAGE".to_owned(),
        }))
        .await;
    let stray = client.next(Duration::from_millis(800)).await;
    assert!(
        !matches!(stray, Some(ServerPacket::NPCResponse(_))),
        "不存在的名字不应执行任何页面，实际收到：{stray:?}"
    );
}

/// 仓库根（测试的工作目录是 crate 目录）。
fn repo_root() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// 真实数据集 + 只开一个 GM 账号的配置；数据集缺失返回 `None` 并打印缺了什么。
fn config_with_real_data() -> Option<ServerConfig> {
    let data = match mir_server::gamedata::GameData::discover() {
        Ok(data) => data,
        Err(error) => {
            eprintln!("发现数据集失败：{error}");
            return None;
        }
    };
    let Some(envir) = data.envir_dir().map(std::path::Path::to_path_buf) else {
        eprintln!("数据集不在文件系统上（没有 Envir 目录）");
        return None;
    };
    if !envir.join("NPCs").is_dir() {
        eprintln!("{} 下没有 NPCs 目录", envir.display());
        return None;
    }
    let map = repo_root().join("resource/map/0.map");
    if !map.exists() {
        eprintln!("缺少地形文件 {}", map.display());
        return None;
    }
    Some(ServerConfig {
        map_root: repo_root().join("resource/map").display().to_string(),
        language_dir: repo_root().join("config/lang").display().to_string(),
        load_game_data: true,
        db_path: None,
        gm_accounts: vec!["npcinput".to_owned()],
        idle_timeout_secs: 0,
        max_connections_per_ip: 0,
        max_connections_per_second: 0,
        max_packets_per_second: 0,
        metrics_listen: None,
        ..ServerConfig::default()
    })
}
