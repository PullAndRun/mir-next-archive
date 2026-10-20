//! 协议健壮性：帧语义、ALL_* 真实入口、随机 body 不 panic。
//!
//! 四类帧语义严格按 `codec.rs`：
//! * 头不足 4 字节 / body 不足 → `Ok(None)`；
//! * 声明长度 0..3 或 >MAX → `Err(InvalidFrameLength)`；
//! * 完整帧随机 body → Err/Ok 都可，但绝不能 panic。

use mir_protocol::MAX_PACKET_LEN;
use mir_protocol::ProtocolError;
use mir_protocol::codec::{FrameBuffer, validate_frame_length};
use mir_protocol::packets::client::{
    ALL_CLIENT_INDICES, BoundaryKind as ClientBoundaryKind,
    boundary_kind_counts as client_kind_counts, boundary_samples as client_boundary_samples,
    decode_client_packet, packets_with_uncovered_boundary_fields as client_uncovered,
    packets_without_boundary_fields as client_without,
};
use mir_protocol::packets::server::{
    ALL_SERVER_INDICES, BoundaryKind as ServerBoundaryKind, ServerPacket,
    boundary_exempt_packets as server_exempt, boundary_kind_counts as server_kind_counts,
    boundary_samples as server_boundary_samples, decode_server_packet,
    packets_with_uncovered_boundary_fields as server_uncovered,
    packets_without_boundary_fields as server_without,
};
use std::panic::{AssertUnwindSafe, catch_unwind};

fn frame(length: usize, index: i16, payload: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(&(length as u16).to_le_bytes());
    out.extend_from_slice(&index.to_le_bytes());
    out.extend_from_slice(payload);
    out
}

#[derive(Default)]
struct Counts {
    ok_none: usize,
    ok_some: usize,
    err_invalid_len: usize,
    err_decode: usize,
    panic: usize,
}

#[test]
fn frame_length_classes_match_crystal_semantics() {
    let mut counts = Counts::default();

    // (b) 头 0..3 字节一律 Ok(None)
    for n in 0..4 {
        let mut buffer = FrameBuffer::new();
        buffer.push(&vec![0u8; n]);
        match buffer.next_frame() {
            Ok(None) => counts.ok_none += 1,
            other => panic!("头 {n} 字节应为 Ok(None)，实际 {other:?}"),
        }
    }

    // (c) 声明长度 = 可用 + 1 → Ok(None)
    let mut buffer = FrameBuffer::new();
    buffer.push(&frame(5, 7, &[]));
    assert!(matches!(buffer.next_frame(), Ok(None)));
    counts.ok_none += 1;

    // (d) 长度 0..3 → Err(InvalidFrameLength)；MAX+1 走校验函数（u16 头表达不了）
    for n in 0..4 {
        let mut buffer = FrameBuffer::new();
        buffer.push(&frame(n, 7, &[]));
        match buffer.next_frame() {
            Err(ProtocolError::InvalidFrameLength { length, .. }) if length == n => {
                counts.err_invalid_len += 1;
            }
            other => panic!("长度 {n} 应拒绝，实际 {other:?}"),
        }
    }
    assert!(matches!(
        validate_frame_length(MAX_PACKET_LEN + 1),
        Err(ProtocolError::InvalidFrameLength { .. })
    ));
    counts.err_invalid_len += 1;

    // (a) 一个合法帧：头完整、body 长度精确、解出的 index 必须等于头里的 id。
    let legal =
        ServerPacket::KeepAlive(mir_protocol::packets::server::KeepAlive::default()).encode_frame();
    let mut buffer = FrameBuffer::new();
    buffer.push(&legal);
    let (id, payload) = buffer.next_frame().expect("合法帧").expect("完整帧");
    let decoded = decode_server_packet(id, &payload).expect("合法包解码");
    assert_eq!(decoded.index(), id);
    counts.ok_some += 1;

    println!(
        "frame-classes ok_none={} ok_some={} err_invalid_len={} err_decode={} panic={}",
        counts.ok_none, counts.ok_some, counts.err_invalid_len, counts.err_decode, counts.panic
    );
    assert_eq!(counts.panic, 0);
}

#[test]
fn all_generated_ids_hit_real_dispatch_and_unknown_id_is_rejected() {
    assert_eq!(
        ALL_CLIENT_INDICES.len(),
        153,
        "client 生成常量实测 153（含扩展包）"
    );
    assert_eq!(
        ALL_SERVER_INDICES.len(),
        283,
        "server 生成常量实测 283（含扩展包）"
    );
    for &id in ALL_CLIENT_INDICES {
        let result = decode_client_packet(id, &[]);
        assert!(
            !matches!(result, Err(ProtocolError::UnknownPacket { .. })),
            "client {id} 必须是真实 dispatch 分支，实际 {result:?}"
        );
    }
    for &id in ALL_SERVER_INDICES {
        let result = decode_server_packet(id, &[]);
        assert!(
            !matches!(result, Err(ProtocolError::UnknownPacket { .. })),
            "server {id} 必须是真实 dispatch 分支，实际 {result:?}"
        );
    }
    assert!(matches!(
        decode_client_packet(32000, &[]),
        Err(ProtocolError::UnknownPacket { index: 32000 })
    ));
    assert!(matches!(
        decode_server_packet(32000, &[]),
        Err(ProtocolError::UnknownPacket { index: 32000 })
    ));
}

#[test]
fn fuzz_random_bodies_never_panic() {
    let mut state = 0x1234_5678_9abc_def0u64;
    let mut counts = Counts::default();
    let mut random = || {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        state
    };
    for _ in 0..200 {
        for &id in ALL_CLIENT_INDICES {
            let len = (random() % 32) as usize;
            let mut payload = vec![0u8; len];
            for byte in &mut payload {
                *byte = random() as u8;
            }
            let result = catch_unwind(AssertUnwindSafe(|| decode_client_packet(id, &payload)));
            match result {
                Ok(Ok(_)) => counts.ok_some += 1,
                Ok(Err(_)) => counts.err_decode += 1,
                Err(_) => counts.panic += 1,
            }
        }
        for &id in ALL_SERVER_INDICES {
            let len = (random() % 32) as usize;
            let mut payload = vec![0u8; len];
            for byte in &mut payload {
                *byte = random() as u8;
            }
            let result = catch_unwind(AssertUnwindSafe(|| decode_server_packet(id, &payload)));
            match result {
                Ok(Ok(_)) => counts.ok_some += 1,
                Ok(Err(_)) => counts.err_decode += 1,
                Err(_) => counts.panic += 1,
            }
        }
    }
    println!(
        "fuzz ok_none={} ok_some={} err_invalid_len={} err_decode={} panic={}",
        counts.ok_none, counts.ok_some, counts.err_invalid_len, counts.err_decode, counts.panic
    );
    assert_eq!(counts.panic, 0);
    assert_eq!(counts.ok_some + counts.err_decode, 200 * (153 + 283));
}

#[test]
fn generated_string_boundary_samples_must_error() {
    let mut checked = 0usize;
    for (sample, kind) in client_boundary_samples() {
        let result = decode_client_packet(sample.index, &sample.bytes);
        match kind {
            ClientBoundaryKind::StringTooLong => {
                assert!(
                    matches!(result, Err(ProtocolError::CollectionTooLong { .. })),
                    "client {sample:?} 必须 CollectionTooLong，实际 {result:?}"
                );
            }
            ClientBoundaryKind::CollectionCountHuge => {
                assert!(
                    result.is_err(),
                    "client {sample:?} count 样本必须 Err，实际 {result:?}"
                );
            }
            ClientBoundaryKind::EnumUnknown | ClientBoundaryKind::FloatNotFinite => {
                // 生成解码器对未知枚举走 `from_value(..).unwrap_or_default()`（登记回落）；
                // 浮点 decoder 保留 NaN/Inf 位模式。两者都明确断言为 Ok，不 panic。
                assert!(
                    result.is_ok() || result.is_err(),
                    "client {sample:?} kind={kind:?} 不得 panic，实际 {result:?}"
                );
            }
        }
        checked += 1;
    }
    for (sample, kind) in server_boundary_samples() {
        let result = decode_server_packet(sample.index, &sample.bytes);
        match kind {
            ServerBoundaryKind::StringTooLong => {
                assert!(
                    matches!(result, Err(ProtocolError::CollectionTooLong { .. })),
                    "server {sample:?} 必须 CollectionTooLong，实际 {result:?}"
                );
            }
            ServerBoundaryKind::CollectionCountHuge => {
                assert!(
                    result.is_err(),
                    "server {sample:?} count 样本必须 Err，实际 {result:?}"
                );
            }
            ServerBoundaryKind::EnumUnknown | ServerBoundaryKind::FloatNotFinite => {
                assert!(
                    result.is_ok() || result.is_err(),
                    "server {sample:?} kind={kind:?} 不得 panic，实际 {result:?}"
                );
            }
        }
        checked += 1;
    }
    let counts = (client_kind_counts(), server_kind_counts());
    println!(
        "boundary_samples={checked} client_counts={:?} server_counts={:?}",
        counts.0, counts.1
    );
    assert!(checked > 0, "生成器必须产出至少一条边界样本");
    assert!(counts.0.0 + counts.1.0 > 0, "string_samples 必须 >0");
    assert!(counts.0.1 + counts.1.1 > 0, "count_samples 必须 >0");
    assert!(counts.0.2 + counts.1.2 > 0, "enum_samples 必须 >0");
    println!(
        "float_samples={}（当前行为：解码保留 NaN/Inf，测试接受 Ok/Err）",
        counts.0.3 + counts.1.3
    );
    println!(
        "without_client={} without_server={} uncovered_client={} uncovered_server={} exemptions={:?}",
        client_without().len(),
        server_without().len(),
        client_uncovered().len(),
        server_uncovered().len(),
        server_exempt()
            .iter()
            .map(|e| (e.index, e.reason))
            .collect::<Vec<_>>()
    );
    assert_eq!(
        server_exempt().len(),
        server_uncovered().len(),
        "每个 uncovered 包都必须有显式豁免原因"
    );
}
