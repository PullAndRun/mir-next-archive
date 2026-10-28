#!/usr/bin/env python3
"""R77-A 只读素材准入审计工具（manpower / bingbu, deepseek）。

范围（与 R77 计划/尚书派发一致）：
* 只审计**本地八套 WZX/WZL**语料中的 Magic2 索引，以及 `Magic2.Lib` 候选；
* 只读：本工具从不写/移/删任何文件，也不改 `game-assets`；事实只打到 stdout；
* 结论三分：default Magic2=NOT_READY、2025 语料=UNVERIFIED_SEMANTICS、
  Magic2.Lib 双候选=AWAITING_BINDING（待认证候选，**永不自动 READY**）。
* READY 只能由人工显式 `--human-approve`（绑定文件/内联 JSON + 实存审核证据摘要）
  同时满足时才可能给出；`--attempt-ready` 是绕行负控，必定 EXIT 2。

子命令（无参数等价 `--facts`）：
  --facts                    输出实测事实 JSON（只读）
  --assert                   与内嵌期望逐项 PASS/FAIL（0/1）
  --selftest                 内置合成自测：解析规则、状态机、禁自动升 READY
  --matrix                   G5 差异矩阵：从 spellfx.rs / SpellObject.cs 独立解析
  --attempt-ready SUBJECT    绕行负控：自动升 READY 必须失败（EXIT 2）
  --human-approve PATH       人工批准路径：读绑定 JSON 文件（不写）
  --human-approve-json JSON  人工批准路径：内联绑定 JSON（不写）

退出码：0=正常完成/通过；1=读取或解析错误；2=准入拒绝（含自动升 READY）。
"""

import argparse
import hashlib
import json
import os
import struct
import sys
import zlib
from datetime import datetime, timezone

SCHEMA_VERSION = 1
SCHEMA = "round77-a/asset-admission/v1"
ROUND = "R77"
SUBTASK = "JJC-20261028-004-sub-2a7d07c6"
ROLE = "bingbu/deepseek (A)"

# ---- 二进制布局常量（WeMade type-1 WZX/WZL 与 Crystal MLibrary v2 Lib）----
WZX_HEADER_LEN = 48  # 头部 48B，末尾 u32 声明数；随后每个 u32 槽 = 帧偏移
WZL_RECORD_HEADER_LEN = 16  # fmt(1) reserved(3) w(2) h(2) ox(2) oy(2) len(4)
LIB_RECORD_HEADER_LEN = 17  # w(2) h(2) x(2) y(2) sx(2) sy(2) shadow(1) len(4)

# ---- 状态机 ----
STATES = ("NOT_READY", "UNVERIFIED_SEMANTICS", "AWAITING_BINDING", "READY")
READY = STATES[3]
READY_REQUIRES_HUMAN_APPROVAL = True
AUTO_READY_FORBIDDEN = True

TRAP_SPELL_ID = 98
TRAP_LIBRARY = "Magic2"
TRAP_BASE = 2360
TRAP_COUNT = 8
TRAP_INTERVAL_MS = 100
TRAP_FRAMES = tuple(range(TRAP_BASE, TRAP_BASE + TRAP_COUNT))

# 期望冻结（--assert 用）。来源：本工具对原始文件的独立实测；
# 与 B 工部冻结事实交叉核对一致（2025 解压规则偏差 dev-1 已记录）。
EXPECTED = {
    'schema': 'round77-a/asset-admission/v1',
    'schema_version': 1,
    'round': 'R77',
    'subtask': 'JJC-20261028-004-sub-2a7d07c6',
    'role': 'bingbu/deepseek (A)',
    'read_only': True,
    'scope.production_changes': False,
    'scope.writes_files': False,
    'default_magic2.subject': 'resource/data/magic2.wzx + resource/data/magic2.wzl',
    'default_magic2.wzx.path': 'resource/data/magic2.wzx',
    'default_magic2.wzx.bytes': 5968,
    'default_magic2.wzx.sha256': '62559cdd1626b26c2566fe2922e8614dbc6d1896f9fb29584a516e3706addea3',
    'default_magic2.wzx.declared_count': 1460,
    'default_magic2.wzx.physical_slots': 1480,
    'default_magic2.wzx.offset_table_remainder_bytes': 0,
    'default_magic2.wzx.max_frame_index': 1479,
    'default_magic2.wzl.path': 'resource/data/magic2.wzl',
    'default_magic2.wzl.bytes': 10290606,
    'default_magic2.wzl.sha256': '21f46eea925c7a498961dfdabe835433e41267fa5468a99011c9c018dc8a9bd1',
    'default_magic2.wzl.append_headroom_bytes': 0,
    'default_magic2.wzl.max_record_end': 10290606,
    'default_magic2.wzl.unindexed_gap_total_bytes': 2637986,
    'default_magic2.wzl.unindexed_gap_count': 5,
    'default_magic2.wzl.unindexed_largest_gap_bytes': 2437042,
    'default_magic2.frames_2360_2367': [{'frame': 2360, 'declared': False, 'slot_in_table': False, 'needed_table_byte_range': [9488, 9492], 'beyond_wzx_bytes': 3524, 'verdict': 'MISSING_INDEX_OUT_OF_TABLE'}, {'frame': 2361, 'declared': False, 'slot_in_table': False, 'needed_table_byte_range': [9492, 9496], 'beyond_wzx_bytes': 3528, 'verdict': 'MISSING_INDEX_OUT_OF_TABLE'}, {'frame': 2362, 'declared': False, 'slot_in_table': False, 'needed_table_byte_range': [9496, 9500], 'beyond_wzx_bytes': 3532, 'verdict': 'MISSING_INDEX_OUT_OF_TABLE'}, {'frame': 2363, 'declared': False, 'slot_in_table': False, 'needed_table_byte_range': [9500, 9504], 'beyond_wzx_bytes': 3536, 'verdict': 'MISSING_INDEX_OUT_OF_TABLE'}, {'frame': 2364, 'declared': False, 'slot_in_table': False, 'needed_table_byte_range': [9504, 9508], 'beyond_wzx_bytes': 3540, 'verdict': 'MISSING_INDEX_OUT_OF_TABLE'}, {'frame': 2365, 'declared': False, 'slot_in_table': False, 'needed_table_byte_range': [9508, 9512], 'beyond_wzx_bytes': 3544, 'verdict': 'MISSING_INDEX_OUT_OF_TABLE'}, {'frame': 2366, 'declared': False, 'slot_in_table': False, 'needed_table_byte_range': [9512, 9516], 'beyond_wzx_bytes': 3548, 'verdict': 'MISSING_INDEX_OUT_OF_TABLE'}, {'frame': 2367, 'declared': False, 'slot_in_table': False, 'needed_table_byte_range': [9516, 9520], 'beyond_wzx_bytes': 3552, 'verdict': 'MISSING_INDEX_OUT_OF_TABLE'}],
    'default_magic2.index_capacity': {'declared_count': 1460, 'physical_slots': 1480, 'max_frame_index': 1479, 'frames_2360_2367_out_of_table': True, 'declared_vs_physical_mismatch': True},
    'default_magic2.verdict': 'NOT_READY',
    'default_magic2.reason_codes': ['MISSING_INDEX', 'OUT_OF_INDEX_TABLE', 'CORPUS_ADDRESS_BEYOND_DEFAULT_WZL_EOF'],
    'default_magic2.address_boundary_check': {'basis': '2025 corpus Trap group start offset vs default magic2.wzl size (reference only; NOT an absence proof)', 'reference_only': True, 'required_payload_compressed_bytes': 114603, 'required_payload_with_headers_bytes': 114739, 'lib_record_header_bytes': 136, 'wzl_bytes': 10290606, 'append_headroom_bytes': 0, 'unindexed_gap_total_bytes': 2637986, 'group_start_offset_2025': 13316944, 'group_start_beyond_default_wzl_by': 3026338, 'note': '2025 Trap 记录起始地址在默认 magic2.wzl EOF 之外，故该地址在默认库中不可直接读取；八帧压缩载荷合计仅 114603 B（含 8×17 B 帧头共 114739 B），默认库未索引空间 2637986 B 足以容纳——尺寸比较不能证明‘全库不存在’，NOT_READY 的独立依据是 WZX 索引表越界。'},
    'default_magic2.admission': {'state': 'NOT_READY', 'can_become_ready': False, 'blockers': ['MISSING_INDEX', 'OUT_OF_INDEX_TABLE', 'CORPUS_ADDRESS_BEYOND_DEFAULT_WZL_EOF']},
    'corpus_2025.subject': 'resource/community/mirfiles-shanda-2025-client/Legend of mir2025.1.29龙/data/magic2.wzx + resource/community/mirfiles-shanda-2025-client/Legend of mir2025.1.29龙/data/magic2.wzl',
    'corpus_2025.wzx.path': 'resource/community/mirfiles-shanda-2025-client/Legend of mir2025.1.29龙/data/magic2.wzx',
    'corpus_2025.wzx.bytes': 10368,
    'corpus_2025.wzx.sha256': 'fe07563ac2f486ad12bc149365ac7bbeba2af002f2981f5a05e6a0a7f990bd54',
    'corpus_2025.wzx.declared_count': 2580,
    'corpus_2025.wzx.physical_slots': 2580,
    'corpus_2025.wzx.offset_table_remainder_bytes': 0,
    'corpus_2025.wzx.max_frame_index': 2579,
    'corpus_2025.wzl.path': 'resource/community/mirfiles-shanda-2025-client/Legend of mir2025.1.29龙/data/magic2.wzl',
    'corpus_2025.wzl.bytes': 14033479,
    'corpus_2025.wzl.sha256': '8e5edc62b761418760b6de64b99625157fe5344f8502bdb7e258a293f010f66d',
    'corpus_2025.wzl.declared_equals_physical': True,
    'corpus_2025.wzl.unindexed_gap_total_bytes': 2910120,
    'corpus_2025.frames_2360_2367': [{'format': 5, 'reserved': [1, 0, 0], 'width': 158, 'height': 127, 'offset_x': -85, 'offset_y': -100, 'compressed_len': 5465, 'data_offset': 13316960, 'record_end': 13322425, 'compressed_sha256': '59ada94eb56e2332346202bb88090aae8a82d2d70c65a95ee018cc1ae536598e', 'zlib_ok': True, 'decompressed_len': 40132, 'decompressed_sha256': 'df8c8f7656953b7a314bcba79b607f36b29d2e8be167ba6fbd9a6dcad8bbbaeb', 'frame': 2360, 'index_slot_offset': 13316944, 'record_start': 13316944, 'expected_rgb565_rowaligned_bytes': 40132, 'expected_w_h_4_bytes': 80264, 'decompressed_eq_rgb565_rowaligned': True, 'decompressed_eq_w_h_4': False}, {'format': 5, 'reserved': [1, 0, 0], 'width': 176, 'height': 101, 'offset_x': -82, 'offset_y': -75, 'compressed_len': 5976, 'data_offset': 13322441, 'record_end': 13328417, 'compressed_sha256': '2ff99112ba98b8959132cc4131cec4dc7ac371e7acbc3f856fad44657ff7f1ee', 'zlib_ok': True, 'decompressed_len': 35552, 'decompressed_sha256': 'aa83222606c9643b5e7040422a5b43526902195f1b94bd4eeba72f46028c17b2', 'frame': 2361, 'index_slot_offset': 13322425, 'record_start': 13322425, 'expected_rgb565_rowaligned_bytes': 35552, 'expected_w_h_4_bytes': 71104, 'decompressed_eq_rgb565_rowaligned': True, 'decompressed_eq_w_h_4': False}, {'format': 5, 'reserved': [1, 0, 0], 'width': 148, 'height': 130, 'offset_x': -66, 'offset_y': -108, 'compressed_len': 6159, 'data_offset': 13328433, 'record_end': 13334592, 'compressed_sha256': '09add064efb88d3c3c7f748f79dcaa3b08d0ebf1e16dccf39b7f39f936802645', 'zlib_ok': True, 'decompressed_len': 38480, 'decompressed_sha256': '6811522618926aad421657e7cec2e18d13c430a9ab9166c404b44e9af57719b8', 'frame': 2362, 'index_slot_offset': 13328417, 'record_start': 13328417, 'expected_rgb565_rowaligned_bytes': 38480, 'expected_w_h_4_bytes': 76960, 'decompressed_eq_rgb565_rowaligned': True, 'decompressed_eq_w_h_4': False}, {'format': 5, 'reserved': [1, 0, 0], 'width': 150, 'height': 121, 'offset_x': -69, 'offset_y': -103, 'compressed_len': 5792, 'data_offset': 13334608, 'record_end': 13340400, 'compressed_sha256': '10d4e80ad8ac6cf44131cbe29b96fdf683bbdf9333e223285b30a7170d417fd8', 'zlib_ok': True, 'decompressed_len': 36300, 'decompressed_sha256': '297dea5737e029b5b92cb765f787d58b8401ec8d07045485596677adceb59d2c', 'frame': 2363, 'index_slot_offset': 13334592, 'record_start': 13334592, 'expected_rgb565_rowaligned_bytes': 36300, 'expected_w_h_4_bytes': 72600, 'decompressed_eq_rgb565_rowaligned': True, 'decompressed_eq_w_h_4': False}, {'format': 5, 'reserved': [1, 0, 0], 'width': 174, 'height': 98, 'offset_x': -97, 'offset_y': -101, 'compressed_len': 5646, 'data_offset': 13340416, 'record_end': 13346062, 'compressed_sha256': 'a48e861eab11e42796039285e18349c1b6fb61c2867f438ae593253bbda30941', 'zlib_ok': True, 'decompressed_len': 34104, 'decompressed_sha256': '15eb89adae59d6087cd22626485242febbc8fa06e1bc8b57b11795992236ad64', 'frame': 2364, 'index_slot_offset': 13340400, 'record_start': 13340400, 'expected_rgb565_rowaligned_bytes': 34104, 'expected_w_h_4_bytes': 68208, 'decompressed_eq_rgb565_rowaligned': True, 'decompressed_eq_w_h_4': False}, {'format': 5, 'reserved': [1, 0, 0], 'width': 150, 'height': 149, 'offset_x': -82, 'offset_y': -97, 'compressed_len': 6780, 'data_offset': 13346078, 'record_end': 13352858, 'compressed_sha256': '3b8cecc9a2f09a9b196184b953ef0b265d067901b4bc31157630d99ed7b14d3e', 'zlib_ok': True, 'decompressed_len': 44700, 'decompressed_sha256': '89012bca4abf02152c17d1645f95589d46de14e2c7dcf1963b20382f60c0d6d3', 'frame': 2365, 'index_slot_offset': 13346062, 'record_start': 13346062, 'expected_rgb565_rowaligned_bytes': 44700, 'expected_w_h_4_bytes': 89400, 'decompressed_eq_rgb565_rowaligned': True, 'decompressed_eq_w_h_4': False}, {'format': 5, 'reserved': [1, 0, 0], 'width': 108, 'height': 116, 'offset_x': -78, 'offset_y': -83, 'compressed_len': 3581, 'data_offset': 13352874, 'record_end': 13356455, 'compressed_sha256': '5c2b8e8a204c4d97fdd8cf16db5b5832f1b5933ec82738f0d26be98da0a0af30', 'zlib_ok': True, 'decompressed_len': 25056, 'decompressed_sha256': '21f07e2ca358846b2eaa81431de6d7d7c5fba4db0449d9fa0f418f5d5b8b490c', 'frame': 2366, 'index_slot_offset': 13352858, 'record_start': 13352858, 'expected_rgb565_rowaligned_bytes': 25056, 'expected_w_h_4_bytes': 50112, 'decompressed_eq_rgb565_rowaligned': True, 'decompressed_eq_w_h_4': False}, {'format': 5, 'reserved': [1, 0, 0], 'width': 66, 'height': 48, 'offset_x': -38, 'offset_y': -16, 'compressed_len': 1155, 'data_offset': 13356471, 'record_end': 13357626, 'compressed_sha256': '94dfcd654ce66f7eb8e4448fb233ac9a16150d8069435fed53c162ad1719fcbe', 'zlib_ok': True, 'decompressed_len': 6336, 'decompressed_sha256': '0e7804aaf2526f55ff7b5cfe1d549ab9f81ef9574caf55b408728ece46140de1', 'frame': 2367, 'index_slot_offset': 13356455, 'record_start': 13356455, 'expected_rgb565_rowaligned_bytes': 6336, 'expected_w_h_4_bytes': 12672, 'decompressed_eq_rgb565_rowaligned': True, 'decompressed_eq_w_h_4': False}],
    'corpus_2025.group_boundary': {'group_start_offset': 13316944, 'group_end_offset': 13357626, 'group_compressed_bytes': 40682, 'chain_contiguous': True, 'preceding_frame_2359': {'frame': 2359, 'record_start': 13310838, 'record_end': 13316944, 'width': 130, 'height': 140, 'compressed_len': 6090, 'decompressed_len': 36400}, 'preceding_record_end_equals_group_start': True, 'following_frame_2368': {'frame': 2368, 'record_start': 13357626, 'record_end': 13357642, 'width': 1, 'height': 1, 'compressed_len': 0, 'decompressed_len': None}, 'following_record_start_equals_group_end': True, 'following_is_1x1_empty_placeholder': True},
    'corpus_2025.decompressed_size_rule': {'measured': 'rgb565_rowaligned = ((((w*16)+31)>>5)*4)*h', 'deviation': 'NOT w*h*4; 16bpp RGB565 with 4-byte row alignment', 'all_eight_eq_rgb565_rowaligned': True, 'all_eight_eq_w_h_4': False},
    'corpus_2025.shape_vs_lib_candidate': {'lib_candidate_shape': '396..432 x 363 (Magic2.Lib v2, 8 frames)', 'matches': [{'frame': 2360, 'candidate': [158, 127], 'lib': [396, 363], 'match': False}, {'frame': 2361, 'candidate': [176, 101], 'lib': [432, 363], 'match': False}, {'frame': 2362, 'candidate': [148, 130], 'lib': [428, 363], 'match': False}, {'frame': 2363, 'candidate': [150, 121], 'lib': [424, 363], 'match': False}, {'frame': 2364, 'candidate': [174, 98], 'lib': [416, 363], 'match': False}, {'frame': 2365, 'candidate': [150, 149], 'lib': [412, 363], 'match': False}, {'frame': 2366, 'candidate': [108, 116], 'lib': [404, 363], 'match': False}, {'frame': 2367, 'candidate': [66, 48], 'lib': [400, 363], 'match': False}], 'all_mismatch': True},
    'corpus_2025.verdict': 'UNVERIFIED_SEMANTICS',
    'corpus_2025.reason_codes': ['SYNTAX_DECODES', 'FORMAT_5_RGB565_ROWALIGNED', 'SEMANTICS_UNPROVEN', 'SHAPE_MISMATCH_VS_LIB_CANDIDATE', 'NO_HUMAN_APPROVED_BINDING'],
    'corpus_2025.admission': {'state': 'UNVERIFIED_SEMANTICS', 'can_become_ready': False, 'same_number_substitution_proven': False, 'blockers': ['SEMANTICS_UNPROVEN', 'NO_HUMAN_APPROVED_BINDING']},
    'lib_candidates.identical_sha': True,
    'lib_candidates.candidates.0.path': 'resource/community/mirfiles-default-crystal/Default Crystal/Data/Magic2.Lib',
    'lib_candidates.candidates.0.bytes': 21091901,
    'lib_candidates.candidates.0.sha256': 'f449aa4c96632f13454d7746ab47678c06175be02e69a6dc8fdfd036755fc0d2',
    'lib_candidates.candidates.0.version': 2,
    'lib_candidates.candidates.0.count': 2790,
    'lib_candidates.candidates.0.index_table_end': 11168,
    'lib_candidates.candidates.0.first_record_offset': 11168,
    'lib_candidates.candidates.0.index_table_end_equals_first_record': True,
    'lib_candidates.candidates.0.offsets_sorted': True,
    'lib_candidates.candidates.0.structural_issue_count': 0,
    'lib_candidates.candidates.0.all_eight_gzip_ok': True,
    'lib_candidates.candidates.0.all_eight_eq_w_h_4': True,
    'lib_candidates.candidates.0.group_contiguous': True,
    'lib_candidates.candidates.0.decompressed_rule': 'w*h*4 (A8R8G8B8 RGBA)',
    'lib_candidates.candidates.0.status': 'AWAITING_BINDING',
    'lib_candidates.candidates.0.ready': False,
    'lib_candidates.candidates.0.certified': False,
    'lib_candidates.candidates.0.frames_2360_2367': [{'width': 396, 'height': 363, 'offset_x': -296, 'offset_y': -149, 'shadow_x': 0, 'shadow_y': 0, 'shadow_byte': 0, 'has_mask': False, 'compressed_len': 12909, 'data_offset': 16533867, 'record_end': 16546776, 'compressed_sha256': '2359089d8d8ed818a038db4308502049ee1505db42e4025ec214d61651907c12', 'gzip_magic_hex': '1f8b', 'gzip_ok': True, 'decompressed_len': 574992, 'decompressed_sha256': '961bc7acdadb53755b8cef97ca61a857eec5b53aaef51d7722b21467266e0e32', 'frame': 2360, 'index_slot_offset': 16533850, 'record_start': 16533850, 'expected_rgba_bytes': 574992, 'decompressed_eq_w_h_4': True, 'contiguous_with_previous': False}, {'width': 432, 'height': 363, 'offset_x': -296, 'offset_y': -149, 'shadow_x': 0, 'shadow_y': 0, 'shadow_byte': 0, 'has_mask': False, 'compressed_len': 15496, 'data_offset': 16546793, 'record_end': 16562289, 'compressed_sha256': 'a962056329745a32310c501f26f2dae6f3b4747d1087367bccae332cb6d732d3', 'gzip_magic_hex': '1f8b', 'gzip_ok': True, 'decompressed_len': 627264, 'decompressed_sha256': 'ba57c1a7f9178e34249709235284433124e7b520e90afbe44930e794e6c24bc9', 'frame': 2361, 'index_slot_offset': 16546776, 'record_start': 16546776, 'expected_rgba_bytes': 627264, 'decompressed_eq_w_h_4': True, 'contiguous_with_previous': True}, {'width': 428, 'height': 363, 'offset_x': -296, 'offset_y': -149, 'shadow_x': 0, 'shadow_y': 0, 'shadow_byte': 0, 'has_mask': False, 'compressed_len': 15501, 'data_offset': 16562306, 'record_end': 16577807, 'compressed_sha256': 'bc331a3efe1ea3aa54af818d4cb14d88ead89595cd3a9b9b36f227dd4888348e', 'gzip_magic_hex': '1f8b', 'gzip_ok': True, 'decompressed_len': 621456, 'decompressed_sha256': '200ffe8cb2c0aa1ab76755d781126139d766ab41fb7bcf688208296bc6c0b54c', 'frame': 2362, 'index_slot_offset': 16562289, 'record_start': 16562289, 'expected_rgba_bytes': 621456, 'decompressed_eq_w_h_4': True, 'contiguous_with_previous': True}, {'width': 424, 'height': 363, 'offset_x': -296, 'offset_y': -149, 'shadow_x': 0, 'shadow_y': 0, 'shadow_byte': 0, 'has_mask': False, 'compressed_len': 15477, 'data_offset': 16577824, 'record_end': 16593301, 'compressed_sha256': '239826f35555e838045becd8ff7b4197ca884e0b18b523a89d5925361ee32bd5', 'gzip_magic_hex': '1f8b', 'gzip_ok': True, 'decompressed_len': 615648, 'decompressed_sha256': 'f32c49285bdc60f2089808cc6bef4260c630864b7b65f5a4c94efe10cba2f327', 'frame': 2363, 'index_slot_offset': 16577807, 'record_start': 16577807, 'expected_rgba_bytes': 615648, 'decompressed_eq_w_h_4': True, 'contiguous_with_previous': True}, {'width': 416, 'height': 363, 'offset_x': -296, 'offset_y': -149, 'shadow_x': 0, 'shadow_y': 0, 'shadow_byte': 0, 'has_mask': False, 'compressed_len': 14729, 'data_offset': 16593318, 'record_end': 16608047, 'compressed_sha256': 'fc78331d863ec8d26b8ac094cae4d8db8a209e2bf68a0a50ef9ae4f5f2b7b505', 'gzip_magic_hex': '1f8b', 'gzip_ok': True, 'decompressed_len': 604032, 'decompressed_sha256': '6e62fdedd716442e436418dc970f206f4ee72d4bb29cd3454e2f2d5ff297ca82', 'frame': 2364, 'index_slot_offset': 16593301, 'record_start': 16593301, 'expected_rgba_bytes': 604032, 'decompressed_eq_w_h_4': True, 'contiguous_with_previous': True}, {'width': 412, 'height': 363, 'offset_x': -296, 'offset_y': -149, 'shadow_x': 0, 'shadow_y': 0, 'shadow_byte': 0, 'has_mask': False, 'compressed_len': 14198, 'data_offset': 16608064, 'record_end': 16622262, 'compressed_sha256': '1a23afd326c9083747ffd08c184737376acca83ae831168985d7af370947464c', 'gzip_magic_hex': '1f8b', 'gzip_ok': True, 'decompressed_len': 598224, 'decompressed_sha256': 'd25d6c66494a174b5da9ee8fdc0ed7c65470de0c5d20fcd65a41d00c47d49292', 'frame': 2365, 'index_slot_offset': 16608047, 'record_start': 16608047, 'expected_rgba_bytes': 598224, 'decompressed_eq_w_h_4': True, 'contiguous_with_previous': True}, {'width': 404, 'height': 363, 'offset_x': -296, 'offset_y': -149, 'shadow_x': 0, 'shadow_y': 0, 'shadow_byte': 0, 'has_mask': False, 'compressed_len': 13318, 'data_offset': 16622279, 'record_end': 16635597, 'compressed_sha256': 'ee6cca1241b622d6f9fb2b2494f8de1153a7003cb48c636ede341389b0a7a578', 'gzip_magic_hex': '1f8b', 'gzip_ok': True, 'decompressed_len': 586608, 'decompressed_sha256': '5faf284d8b31bb2c5455f3b030665c8beb01f373cc525242fe1b5cdfe4aee6f9', 'frame': 2366, 'index_slot_offset': 16622262, 'record_start': 16622262, 'expected_rgba_bytes': 586608, 'decompressed_eq_w_h_4': True, 'contiguous_with_previous': True}, {'width': 400, 'height': 363, 'offset_x': -296, 'offset_y': -149, 'shadow_x': 0, 'shadow_y': 0, 'shadow_byte': 0, 'has_mask': False, 'compressed_len': 12975, 'data_offset': 16635614, 'record_end': 16648589, 'compressed_sha256': 'f4370fd22b481d601fa7c66870068a3c132a234b3c8cdb6a7eff362bfa08cbdf', 'gzip_magic_hex': '1f8b', 'gzip_ok': True, 'decompressed_len': 580800, 'decompressed_sha256': 'f2e32154631f04c9f67228506d3012a56f78e7068e5db903bf9e8d5b8dfa6aec', 'frame': 2367, 'index_slot_offset': 16635597, 'record_start': 16635597, 'expected_rgba_bytes': 580800, 'decompressed_eq_w_h_4': True, 'contiguous_with_previous': True}],
    'lib_candidates.candidates.0.neighbors': [{'frame': 2357, 'width': 4, 'height': 1, 'compressed_len': 23, 'decompressed_len': 16, 'is_4x1_empty': True}, {'frame': 2358, 'width': 4, 'height': 1, 'compressed_len': 23, 'decompressed_len': 16, 'is_4x1_empty': True}, {'frame': 2359, 'width': 4, 'height': 1, 'compressed_len': 23, 'decompressed_len': 16, 'is_4x1_empty': True}, {'frame': 2368, 'width': 4, 'height': 1, 'compressed_len': 23, 'decompressed_len': 16, 'is_4x1_empty': True}, {'frame': 2369, 'width': 4, 'height': 1, 'compressed_len': 23, 'decompressed_len': 16, 'is_4x1_empty': True}],
    'lib_candidates.candidates.1.path': 'resource/community/mirfiles-crystal-patch/crystal__patch_gz__Data__Magic2.Lib',
    'lib_candidates.candidates.1.bytes': 21091901,
    'lib_candidates.candidates.1.sha256': 'f449aa4c96632f13454d7746ab47678c06175be02e69a6dc8fdfd036755fc0d2',
    'admission.state_machine': {'states': ['NOT_READY', 'UNVERIFIED_SEMANTICS', 'AWAITING_BINDING', 'READY'], 'ready_requires_human_approval': True, 'auto_ready_forbidden': True, 'forbidden_transition': 'any -> READY without explicit --human-approve with matching binding/evidence', 'audit_exit0_does_not_mean_ready': True},
    'admission.ready_evidence_requirements': {'spell': 98, 'library': 'Magic2', 'base': 2360, 'count': 8, 'interval_ms': 100, 'binding_fields': ['schema_version', 'spell', 'library', 'base', 'count', 'interval_ms', 'crystal_sha256', 'wzx_sha256', 'wzl_sha256', 'review_evidence_path', 'review_evidence_sha256'], 'note': 'synthetic known fixture binds algorithm only; real binding needs independent menxia review; no real binding provided in this round'},
    'admission.same_number_substitution_proven': False,
    'admission.matrix': [{'subject': 'default-magic2', 'source': 'resource/data/magic2.wzx + resource/data/magic2.wzl', 'state': 'NOT_READY', 'can_become_ready': False, 'ready_eligible_with_human_approval': False, 'blockers': ['MISSING_INDEX', 'OUT_OF_INDEX_TABLE', 'CORPUS_ADDRESS_BEYOND_DEFAULT_WZL_EOF'], 'evidence': 'wzx.sha256 + wzl.sha256'}, {'subject': 'corpus-2025', 'source': 'resource/community/mirfiles-shanda-2025-client/Legend of mir2025.1.29龙/data/magic2.wzx + resource/community/mirfiles-shanda-2025-client/Legend of mir2025.1.29龙/data/magic2.wzl', 'state': 'UNVERIFIED_SEMANTICS', 'can_become_ready': False, 'ready_eligible_with_human_approval': False, 'blockers': ['SEMANTICS_UNPROVEN', 'SHAPE_MISMATCH_VS_LIB_CANDIDATE', 'NO_HUMAN_APPROVED_BINDING'], 'evidence': 'wzx.sha256 + wzl.sha256 + per-frame decompressed sha256'}, {'subject': 'lib-candidate', 'source': 'resource/community/mirfiles-default-crystal/Default Crystal/Data/Magic2.Lib', 'state': 'AWAITING_BINDING', 'can_become_ready': False, 'ready_eligible_with_human_approval': True, 'blockers': ['NO_HUMAN_APPROVED_BINDING', 'NO_REVIEW_EVIDENCE'], 'evidence': 'candidate.sha256 + per-frame decompressed sha256'}],
    'admission.default_state': 'NOT_READY',
    'admission.corpus_2025_state': 'UNVERIFIED_SEMANTICS',
    'admission.lib_candidate_state': 'AWAITING_BINDING',
    'admission.all_can_become_ready': False,
    'admission.readiness_claimed': False,
}

REPO_ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))

DEFAULT_WZX = os.path.join(REPO_ROOT, "resource", "data", "magic2.wzx")
DEFAULT_WZL = os.path.join(REPO_ROOT, "resource", "data", "magic2.wzl")
CORPUS_2025_GLOB_DIR = os.path.join(REPO_ROOT, "resource", "community", "mirfiles-shanda-2025-client")
LIB_CANDIDATE_PATHS = (
    os.path.join(REPO_ROOT, "resource", "community", "mirfiles-default-crystal",
                 "Default Crystal", "Data", "Magic2.Lib"),
    os.path.join(REPO_ROOT, "resource", "community", "mirfiles-crystal-patch",
                 "crystal__patch_gz__Data__Magic2.Lib"),
)
SPELLOBJECT_CS = os.path.join(REPO_ROOT, "github", "Crystal", "Client", "MirObjects", "SpellObject.cs")
SPELLFX_RS = os.path.join(REPO_ROOT, "crates", "mir-client", "src", "ui", "spellfx.rs")
R76_CONTRACT_MD = os.path.join(REPO_ROOT, "docs", "testing", "round76", "round76-trap-contract.md")
R59_TEST_RS = os.path.join(REPO_ROOT, "crates", "mir-client", "tests", "round59_projectile_assets.rs")

# 期望注册的语料：default + 2025。Lib 候选单独列。
EIGHT_WZX_HINT = (
    "default", "shanda-mirt", "shanda-mir", "shanda-story",
    "shanda-return", "shanda-sec", "shanda-2025", "arcadia",
)


class ToolError(Exception):
    """读取/解析错误：EXIT 1。"""


class AdmissionError(Exception):
    """准入拒绝：EXIT 2（携带稳定 code）。"""

    def __init__(self, code: str, problems=None):
        super().__init__(code)
        self.code = code
        self.problems = list(problems or [])


def utc_now() -> str:
    return datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")


def sha256_bytes(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def read_bytes(path: str) -> bytes:
    if not os.path.isfile(path):
        raise ToolError(f"missing file: {path}")
    try:
        with open(path, "rb") as handle:  # 只读打开（本工具唯一的文件打开方式）
            return handle.read()
    except OSError as exc:  # pragma: no cover - 环境损坏时才触发
        raise ToolError(f"cannot read {path}: {exc}") from exc


def rel(path: str) -> str:
    return os.path.relpath(path, REPO_ROOT).replace(os.sep, "/")


def find_2025_pair():
    """定位 2025 语料 magic2.wzx/wzl（目录名含 CJK，固定 glob 单收）。"""
    if not os.path.isdir(CORPUS_2025_GLOB_DIR):
        raise ToolError(f"missing corpus dir: {CORPUS_2025_GLOB_DIR}")
    hits = []
    for name in sorted(os.listdir(CORPUS_2025_GLOB_DIR)):
        wzx = os.path.join(CORPUS_2025_GLOB_DIR, name, "data", "magic2.wzx")
        wzl = os.path.join(CORPUS_2025_GLOB_DIR, name, "data", "magic2.wzl")
        if os.path.isfile(wzx) and os.path.isfile(wzl):
            hits.append((wzx, wzl))
    if len(hits) != 1:
        raise ToolError(f"expected exactly one 2025 magic2 pair, got {len(hits)}")
    return hits[0]


# ---------------------------------------------------------------------------
# 独立解析器
# ---------------------------------------------------------------------------
def parse_wzx(data: bytes):
    """返回 (declared_count, offsets, remainder)。槽 i 的表字节=48+4i。"""
    if len(data) < WZX_HEADER_LEN:
        raise ToolError(f"wzx too short: {len(data)}")
    declared = struct.unpack_from("<I", data, 44)[0]
    table = data[WZX_HEADER_LEN:]
    slots = len(table) // 4
    offsets = [struct.unpack_from("<I", table, 4 * i)[0] for i in range(slots)]
    return declared, offsets, len(table) % 4


def parse_wzl_record(wl: bytes, off: int):
    """解析 16B 记录头 + zlib 载荷；越界返回带 error 的 dict。"""
    if off + WZL_RECORD_HEADER_LEN > len(wl):
        return {"error": "record_header_beyond_file", "offset": off}
    width, height = struct.unpack_from("<HH", wl, off + 4)
    ox, oy = struct.unpack_from("<hh", wl, off + 8)
    comp_len = struct.unpack_from("<I", wl, off + 12)[0]
    rec = {
        "format": wl[off],
        "reserved": list(wl[off + 1:off + 4]),
        "width": width,
        "height": height,
        "offset_x": ox,
        "offset_y": oy,
        "compressed_len": comp_len,
        "data_offset": off + WZL_RECORD_HEADER_LEN,
        "record_end": off + WZL_RECORD_HEADER_LEN + comp_len,
    }
    if rec["record_end"] > len(wl):
        rec["error"] = "record_payload_beyond_file"
        return rec
    comp = wl[rec["data_offset"]:rec["record_end"]]
    rec["compressed_sha256"] = sha256_bytes(comp)
    rec["zlib_ok"] = False
    try:
        raw = zlib.decompress(comp)
    except zlib.error:
        return rec
    rec["zlib_ok"] = True
    rec["decompressed_len"] = len(raw)
    rec["decompressed_sha256"] = sha256_bytes(raw)
    return rec


def parse_lib(data: bytes):
    """MLibrary v2：version/count + count*4 偏移表 + 每帧 17B 头 + gzip 载荷。"""
    if len(data) < 8:
        raise ToolError(f"lib too short: {len(data)}")
    version, count = struct.unpack_from("<ii", data, 0)
    if count <= 0:
        raise ToolError(f"lib count invalid: {count}")
    if 8 + 4 * count > len(data):
        raise ToolError("lib index table beyond file")
    offsets = [struct.unpack_from("<i", data, 8 + 4 * i)[0] for i in range(count)]
    return version, count, offsets


def parse_lib_frame(data: bytes, off: int):
    if off <= 0 or off + LIB_RECORD_HEADER_LEN > len(data):
        return {"error": "record_header_beyond_file", "offset": off}
    width, height, ox, oy = struct.unpack_from("<hhhh", data, off)
    shadow_x, shadow_y = struct.unpack_from("<hh", data, off + 8)
    shadow = data[off + 12]
    comp_len = struct.unpack_from("<i", data, off + 13)[0]
    rec = {
        "width": width,
        "height": height,
        "offset_x": ox,
        "offset_y": oy,
        "shadow_x": shadow_x,
        "shadow_y": shadow_y,
        "shadow_byte": shadow,
        "has_mask": (shadow >> 7) == 1,
        "compressed_len": comp_len,
        "data_offset": off + LIB_RECORD_HEADER_LEN,
        "record_end": off + LIB_RECORD_HEADER_LEN + comp_len,
    }
    if comp_len < 0 or rec["record_end"] > len(data):
        rec["error"] = "record_payload_beyond_file"
        return rec
    comp = data[rec["data_offset"]:rec["record_end"]]
    rec["compressed_sha256"] = sha256_bytes(comp)
    rec["gzip_magic_hex"] = comp[:2].hex()
    rec["gzip_ok"] = False
    try:
        raw = zlib.decompress(comp, 16 + zlib.MAX_WBITS)
    except zlib.error:
        return rec
    rec["gzip_ok"] = True
    rec["decompressed_len"] = len(raw)
    rec["decompressed_sha256"] = sha256_bytes(raw)
    return rec


def rgb565_rowaligned_bytes(width: int, height: int) -> int:
    """16bpp RGB565 行 4 字节对齐：((((w*16)+31)>>5)*4)*h；**不是** w*h*4。"""
    return (((width * 16) + 31) >> 5) * 4 * height


def record_intervals(offsets, wzl_bytes):
    """可完整解析的 WZL 记录区间 [off, off+16+comp_len)；不可解析的偏移被跳过。"""
    wzl_len = len(wzl_bytes)
    intervals = []
    skipped = []
    for off in offsets:
        if off <= 0 or off + WZL_RECORD_HEADER_LEN > wzl_len:
            skipped.append(off)
            continue
        comp_len = struct.unpack_from("<I", wzl_bytes, off + 12)[0]
        end = off + WZL_RECORD_HEADER_LEN + comp_len
        if end > wzl_len:
            skipped.append(off)
            continue
        intervals.append((off, end))
    return intervals, skipped


def coverage_gaps(offsets, wzl_bytes):
    """整条记录区间并集之外的空洞（只统计 [0, wzl_len)）。"""
    wzl_len = len(wzl_bytes)
    covered, _ = record_intervals(offsets, wzl_bytes)
    covered.sort()
    gaps = []
    total = 0
    cursor = 0
    for start, end in covered:
        if start > cursor:
            gaps.append({"start": cursor, "end": start, "bytes": start - cursor})
            total += start - cursor
        cursor = max(cursor, end)
    if cursor < wzl_len:
        gaps.append({"start": cursor, "end": wzl_len, "bytes": wzl_len - cursor})
        total += wzl_len - cursor
    return total, gaps


def merge_intervals(offsets, wzl_len: int):
    covered = []
    for off in offsets:
        if off <= 0 or off + WZL_RECORD_HEADER_LEN > wzl_len:
            continue
        covered.append((off, off + WZL_RECORD_HEADER_LEN))
    covered.sort()
    merged = []
    for start, end in covered:
        if merged and start <= merged[-1][1]:
            merged[-1] = (merged[-1][0], max(merged[-1][1], end))
        else:
            merged.append((start, end))
    return merged


# ---------------------------------------------------------------------------
# 事实收集
# ---------------------------------------------------------------------------
def default_magic2_facts():
    wzx_bytes = read_bytes(DEFAULT_WZX)
    wzl_bytes = read_bytes(DEFAULT_WZL)
    declared, offsets, remainder = parse_wzx(wzx_bytes)
    slots = len(offsets)
    frames = []
    for frame in TRAP_FRAMES:
        table_byte = WZX_HEADER_LEN + 4 * frame
        need = (table_byte, table_byte + 4)
        beyond = table_byte + 4 - len(wzx_bytes) if table_byte + 4 > len(wzx_bytes) else 0
        frames.append({
            "frame": frame,
            "declared": frame < declared,
            "slot_in_table": frame < slots,
            "needed_table_byte_range": [need[0], need[1]],
            "beyond_wzx_bytes": beyond,
            "verdict": "MISSING_INDEX_OUT_OF_TABLE" if frame >= slots else "CHECK",
        })
    gaps_total, gaps = coverage_gaps(offsets, wzl_bytes)
    max_record_end = 0
    for off in offsets:
        if off <= 0 or off + WZL_RECORD_HEADER_LEN > len(wzl_bytes):
            continue
        comp_len = struct.unpack_from("<I", wzl_bytes, off + 12)[0]
        if off + WZL_RECORD_HEADER_LEN + comp_len > len(wzl_bytes):
            continue
        max_record_end = max(max_record_end, off + WZL_RECORD_HEADER_LEN + comp_len)
    return {
        "subject": rel(DEFAULT_WZX) + " + " + rel(DEFAULT_WZL),
        "wzx": {
            "path": rel(DEFAULT_WZX),
            "bytes": len(wzx_bytes),
            "sha256": sha256_bytes(wzx_bytes),
            "declared_count": declared,
            "physical_slots": slots,
            "offset_table_remainder_bytes": remainder,
            "max_frame_index": slots - 1,
        },
        "wzl": {
            "path": rel(DEFAULT_WZL),
            "bytes": len(wzl_bytes),
            "sha256": sha256_bytes(wzl_bytes),
            "append_headroom_bytes": len(wzl_bytes) - max_record_end,
            "max_record_end": max_record_end,
            "unindexed_gap_total_bytes": gaps_total,
            "unindexed_gap_count": len(gaps),
            "unindexed_largest_gap_bytes": max((g["bytes"] for g in gaps), default=0),
        },
        "frames_2360_2367": frames,
        "index_capacity": {
            "declared_count": declared,
            "physical_slots": slots,
            "max_frame_index": slots - 1,
            "frames_2360_2367_out_of_table": all(f >= slots for f in TRAP_FRAMES),
            "declared_vs_physical_mismatch": declared != slots,
        },
        "verdict": "NOT_READY",
        "reason_codes": [
            "MISSING_INDEX",
            "OUT_OF_INDEX_TABLE",
            "CORPUS_ADDRESS_BEYOND_DEFAULT_WZL_EOF",
        ],
    }


def corpus_2025_facts(lib_dims):
    wzx_path, wzl_path = find_2025_pair()
    wzx_bytes = read_bytes(wzx_path)
    wzl_bytes = read_bytes(wzl_path)
    declared, offsets, remainder = parse_wzx(wzx_bytes)
    slots = len(offsets)
    frames = []
    prev_end = None
    group_ok = True
    for frame in TRAP_FRAMES:
        slot_off = offsets[frame]
        rec = parse_wzl_record(wzl_bytes, slot_off)
        rec["frame"] = frame
        rec["index_slot_offset"] = slot_off
        rec["record_start"] = slot_off
        rec["expected_rgb565_rowaligned_bytes"] = (
            rgb565_rowaligned_bytes(rec["width"], rec["height"]) if "width" in rec else None
        )
        rec["expected_w_h_4_bytes"] = (
            rec["width"] * rec["height"] * 4 if "width" in rec else None
        )
        rec["decompressed_eq_rgb565_rowaligned"] = (
            rec.get("zlib_ok") is True
            and rec["decompressed_len"] == rec["expected_rgb565_rowaligned_bytes"]
        )
        rec["decompressed_eq_w_h_4"] = (
            rec.get("zlib_ok") is True
            and rec["decompressed_len"] == rec["expected_w_h_4_bytes"]
        )
        if prev_end is not None and slot_off != prev_end:
            group_ok = False
        prev_end = rec["record_end"]
        frames.append(rec)
    family = {"width": None}
    # 组边界：前帧 2359 尾、后帧 2368 头（1x1 空占位）
    def boundary(frame: int):
        rec = parse_wzl_record(wzl_bytes, offsets[frame])
        return {
            "frame": frame,
            "record_start": offsets[frame],
            "record_end": rec.get("record_end"),
            "width": rec.get("width"),
            "height": rec.get("height"),
            "compressed_len": rec.get("compressed_len"),
            "decompressed_len": rec.get("decompressed_len"),
        }
    preceding = boundary(TRAP_BASE - 1)
    following = boundary(TRAP_BASE + TRAP_COUNT)
    group_start = offsets[TRAP_BASE]
    group_end = frames[-1]["record_end"]
    preceding_ok = preceding["record_end"] == group_start
    following_ok = following["record_start"] == group_end and following["width"] == 1 and following["height"] == 1
    wzl_gaps_total, _ = coverage_gaps(offsets, wzl_bytes)
    dim_matches = [
        {"frame": f["frame"], "candidate": [f["width"], f["height"]],
         "lib": [lib_dims[i][0], lib_dims[i][1]],
         "match": [f["width"], f["height"]] == [lib_dims[i][0], lib_dims[i][1]]}
        for i, f in enumerate(frames)
    ]
    return {
        "subject": rel(wzx_path) + " + " + rel(wzl_path),
        "wzx": {
            "path": rel(wzx_path),
            "bytes": len(wzx_bytes),
            "sha256": sha256_bytes(wzx_bytes),
            "declared_count": declared,
            "physical_slots": slots,
            "offset_table_remainder_bytes": remainder,
            "max_frame_index": slots - 1,
        },
        "wzl": {
            "path": rel(wzl_path),
            "bytes": len(wzl_bytes),
            "sha256": sha256_bytes(wzl_bytes),
            "declared_equals_physical": declared == slots,
            "unindexed_gap_total_bytes": wzl_gaps_total,
        },
        "frames_2360_2367": frames,
        "group_boundary": {
            "group_start_offset": group_start,
            "group_end_offset": group_end,
            "group_compressed_bytes": group_end - group_start,
            "chain_contiguous": group_ok,
            "preceding_frame_2359": preceding,
            "preceding_record_end_equals_group_start": preceding_ok,
            "following_frame_2368": following,
            "following_record_start_equals_group_end": following_ok,
            "following_is_1x1_empty_placeholder": following["compressed_len"] == 0,
        },
        "decompressed_size_rule": {
            "measured": "rgb565_rowaligned = ((((w*16)+31)>>5)*4)*h",
            "deviation": "NOT w*h*4; 16bpp RGB565 with 4-byte row alignment",
            "all_eight_eq_rgb565_rowaligned": all(f.get("decompressed_eq_rgb565_rowaligned") for f in frames),
            "all_eight_eq_w_h_4": all(f.get("decompressed_eq_w_h_4") for f in frames),
        },
        "shape_vs_lib_candidate": {
            "lib_candidate_shape": "396..432 x 363 (Magic2.Lib v2, 8 frames)",
            "matches": dim_matches,
            "all_mismatch": all(not m["match"] for m in dim_matches),
        },
        "verdict": "UNVERIFIED_SEMANTICS",
        "reason_codes": [
            "SYNTAX_DECODES",
            "FORMAT_5_RGB565_ROWALIGNED",
            "SEMANTICS_UNPROVEN",
            "SHAPE_MISMATCH_VS_LIB_CANDIDATE",
            "NO_HUMAN_APPROVED_BINDING",
        ],
        "admission": {
            "state": "UNVERIFIED_SEMANTICS",
            "can_become_ready": False,
            "same_number_substitution_proven": False,
            "blockers": ["SEMANTICS_UNPROVEN", "NO_HUMAN_APPROVED_BINDING"],
        },
    }


def lib_candidate_facts():
    candidates = []
    for path in LIB_CANDIDATE_PATHS:
        data = read_bytes(path)
        version, count, offsets = parse_lib(data)
        issues = []
        for i, off in enumerate(offsets):
            nxt = offsets[i + 1] if i + 1 < count else len(data)
            if off <= 0 or off + LIB_RECORD_HEADER_LEN > len(data):
                issues.append({"frame": i, "reason": "offset_out_of_bounds", "offset": off})
                continue
            comp_len = struct.unpack_from("<i", data, off + 13)[0]
            if comp_len < 0 or off + LIB_RECORD_HEADER_LEN + comp_len > nxt:
                issues.append({"frame": i, "reason": "payload_overlaps_next", "offset": off})
            if data[off + 17:off + 19] != b"\x1f\x8b":
                issues.append({"frame": i, "reason": "gzip_magic_missing", "offset": off})
        frames = []
        prev_end = None
        for frame in TRAP_FRAMES:
            rec = parse_lib_frame(data, offsets[frame])
            rec["frame"] = frame
            rec["index_slot_offset"] = offsets[frame]
            rec["record_start"] = offsets[frame]
            rec["expected_rgba_bytes"] = (
                rec["width"] * rec["height"] * 4 if "width" in rec else None
            )
            rec["decompressed_eq_w_h_4"] = (
                rec.get("gzip_ok") is True
                and rec["decompressed_len"] == rec["expected_rgba_bytes"]
            )
            if prev_end is not None and offsets[frame] != prev_end:
                rec["contiguous_with_previous"] = False
            else:
                rec["contiguous_with_previous"] = prev_end is not None
            prev_end = rec["record_end"]
            frames.append(rec)
        neighbors = []
        for frame in (TRAP_BASE - 3, TRAP_BASE - 2, TRAP_BASE - 1,
                      TRAP_BASE + TRAP_COUNT, TRAP_BASE + TRAP_COUNT + 1):
            rec = parse_lib_frame(data, offsets[frame])
            neighbors.append({
                "frame": frame,
                "width": rec.get("width"),
                "height": rec.get("height"),
                "compressed_len": rec.get("compressed_len"),
                "decompressed_len": rec.get("decompressed_len"),
                "is_4x1_empty": rec.get("width") == 4 and rec.get("height") == 1,
            })
        candidates.append({
            "path": rel(path),
            "bytes": len(data),
            "sha256": sha256_bytes(data),
            "version": version,
            "count": count,
            "index_table_end": 8 + 4 * count,
            "first_record_offset": offsets[0],
            "index_table_end_equals_first_record": 8 + 4 * count == offsets[0],
            "offsets_sorted": all(offsets[i] < offsets[i + 1] for i in range(count - 1)),
            "structural_issue_count": len(issues),
            "structural_issues_first10": issues[:10],
            "frames_2360_2367": frames,
            "neighbors": neighbors,
            "all_eight_gzip_ok": all(f.get("gzip_ok") for f in frames),
            "all_eight_eq_w_h_4": all(f.get("decompressed_eq_w_h_4") for f in frames),
            "group_contiguous": all(f["contiguous_with_previous"] for f in frames[1:]),
            "decompressed_rule": "w*h*4 (A8R8G8B8 RGBA)",
            "status": "AWAITING_BINDING",
            "status_zh": "待认证候选",
            "ready": False,
            "certified": False,
            "notes": [
                "syntactically complete candidate only; READY requires human-approved "
                "binding + review evidence sha; never auto READY",
            ],
        })
    identical = (
        len(candidates) == 2
        and candidates[0]["sha256"] == candidates[1]["sha256"]
        and candidates[0]["bytes"] == candidates[1]["bytes"]
    )
    return {"candidates": candidates, "identical_sha": identical}


# ---------------------------------------------------------------------------
# 绑定 / 状态机（READY 仅人工批准路径；禁自动升）
# ---------------------------------------------------------------------------
BINDING_FIELDS = (
    "schema_version", "spell", "library", "base", "count", "interval_ms",
    "crystal_sha256", "wzx_sha256", "wzl_sha256",
    "review_evidence_path", "review_evidence_sha256",
)


def _as_json_obj(raw: str) -> dict:
    try:
        value = json.loads(raw)
    except json.JSONDecodeError as exc:
        raise AdmissionError("BINDING_JSON_INVALID", [str(exc)]) from exc
    if not isinstance(value, dict):
        raise AdmissionError("BINDING_JSON_INVALID", ["binding must be an object"])
    return value


def _evidence_check(repo_root: str, binding: dict, read_evidence):
    path = str(binding.get("review_evidence_path") or "")
    if not path:
        return ["EVIDENCE_MISSING"]
    full = path if os.path.isabs(path) else os.path.join(repo_root, path)
    try:
        if read_evidence is None:
            data = read_bytes(full)
        else:
            data = read_evidence(full)
    except ToolError:
        return ["EVIDENCE_MISSING"]
    want = str(binding.get("review_evidence_sha256") or "")
    if sha256_bytes(data) != want:
        return ["EVIDENCE_SHA_MISMATCH"]
    return []


def validate_binding(binding: dict, crystal_sha: str, wzx_sha: str, wzl_sha: str,
                     read_evidence=None):
    """返回问题码列表；空列表=绑定字段/摘要全部匹配（仍不等于图像语义已证）。"""
    problems = []
    for field in BINDING_FIELDS:
        if field not in binding or binding[field] in (None, ""):
            problems.append(f"MISSING_FIELD:{field}")
    if problems:
        return problems
    if binding["schema_version"] != SCHEMA_VERSION:
        problems.append("BAD_FIELD:schema_version")
    if binding["spell"] != TRAP_SPELL_ID:
        problems.append("BAD_FIELD:spell")
    if binding["library"] != TRAP_LIBRARY:
        problems.append("BAD_FIELD:library")
    if binding["base"] != TRAP_BASE:
        problems.append("BAD_FIELD:base")
    if binding["count"] != TRAP_COUNT:
        problems.append("BAD_FIELD:count")
    if binding["interval_ms"] != TRAP_INTERVAL_MS:
        problems.append("BAD_FIELD:interval_ms")
    if binding["crystal_sha256"] != crystal_sha:
        problems.append("BAD_SHA:crystal_sha256")
    if binding["wzx_sha256"] != wzx_sha:
        problems.append("BAD_SHA:wzx_sha256")
    if binding["wzl_sha256"] != wzl_sha:
        problems.append("BAD_SHA:wzl_sha256")
    problems.extend(_evidence_check(REPO_ROOT, binding, read_evidence))
    return problems


def decide_admission(base_state: str, human_approve: bool, binding_problems, subject: str):
    """状态机唯一出口：READY 只能在人工批准 + 绑定/证据全部匹配时给出。"""
    if base_state == "READY":  # 防御：不存在自动进入 READY 的路径
        if not (READY_REQUIRES_HUMAN_APPROVAL and human_approve and not binding_problems):
            raise AdmissionError("AUTO_READY_FORBIDDEN", [subject])
        return READY
    if not human_approve:
        if binding_problems:
            return base_state, False, binding_problems
        return base_state, False, ["NEVER_AUTO_READY"]
    if binding_problems:
        return base_state, False, binding_problems
    return READY, True, []


def attempt_ready(subject: str):
    """绕行负控：不经过人工批准路径直接要求 READY，必定拒绝。"""
    raise AdmissionError("AUTO_READY_FORBIDDEN", [subject])


def human_approve(binding_raw: str, subject: str, read_evidence=None):
    if subject != "lib-candidate":
        raise AdmissionError("SUBJECT_NOT_APPROVABLE", [subject])
    binding = _as_json_obj(binding_raw)
    lib = lib_candidate_facts()
    cand = lib["candidates"][0]
    crystal_sha = sha256_bytes(read_bytes(SPELLOBJECT_CS))
    problems = validate_binding(
        binding, crystal_sha, cand["sha256"], cand["sha256"], read_evidence
    )
    state, can, remaining = decide_admission(
        "AWAITING_BINDING", True, problems, subject
    )
    return {
        "subject": subject,
        "state": state,
        "can_become_ready": can,
        "candidate_sha256": cand["sha256"],
        "binding_sha256": sha256_bytes(json.dumps(binding, sort_keys=True).encode("utf-8")),
        "problems": remaining,
        "note": "human approval checks binding/evidence integrity; it does not itself prove image semantics",
    }


# ---------------------------------------------------------------------------
# admission 汇总（facts 输出）
# ---------------------------------------------------------------------------
def _matrix_row(subject, source, state, can_become_ready, ready_eligible, blockers, evidence):
    return {
        "subject": subject,
        "source": source,
        "state": state,
        "can_become_ready": can_become_ready,
        "ready_eligible_with_human_approval": ready_eligible,
        "blockers": blockers,
        "evidence": evidence,
    }


def admission_facts(default, corpus, lib):
    matrix = [
        _matrix_row(
            "default-magic2", default["subject"], "NOT_READY", False, False,
            ["MISSING_INDEX", "OUT_OF_INDEX_TABLE", "CORPUS_ADDRESS_BEYOND_DEFAULT_WZL_EOF"],
            "wzx.sha256 + wzl.sha256",
        ),
        _matrix_row(
            "corpus-2025", corpus["subject"], "UNVERIFIED_SEMANTICS", False, False,
            ["SEMANTICS_UNPROVEN", "SHAPE_MISMATCH_VS_LIB_CANDIDATE",
             "NO_HUMAN_APPROVED_BINDING"],
            "wzx.sha256 + wzl.sha256 + per-frame decompressed sha256",
        ),
        _matrix_row(
            "lib-candidate", lib["candidates"][0]["path"], "AWAITING_BINDING", False, True,
            ["NO_HUMAN_APPROVED_BINDING", "NO_REVIEW_EVIDENCE"],
            "candidate.sha256 + per-frame decompressed sha256",
        ),
    ]
    return {
        "state_machine": {
            "states": list(STATES),
            "ready_requires_human_approval": READY_REQUIRES_HUMAN_APPROVAL,
            "auto_ready_forbidden": AUTO_READY_FORBIDDEN,
            "forbidden_transition": "any -> READY without explicit --human-approve "
                                     "with matching binding/evidence",
            "audit_exit0_does_not_mean_ready": True,
        },
        "ready_evidence_requirements": {
            "spell": TRAP_SPELL_ID,
            "library": TRAP_LIBRARY,
            "base": TRAP_BASE,
            "count": TRAP_COUNT,
            "interval_ms": TRAP_INTERVAL_MS,
            "binding_fields": list(BINDING_FIELDS),
            "note": "synthetic known fixture binds algorithm only; real binding needs "
                    "independent menxia review; no real binding provided in this round",
        },
        "same_number_substitution_proven": False,
        "matrix": matrix,
        "default_state": "NOT_READY",
        "corpus_2025_state": "UNVERIFIED_SEMANTICS",
        "lib_candidate_state": "AWAITING_BINDING",
        "all_can_become_ready": any(row["can_become_ready"] for row in matrix),
        "readiness_claimed": False,
    }


# ---------------------------------------------------------------------------
# F1 差异矩阵：当前 Rust 行为 vs Crystal 目标。全部字段从生产源码独立解析，
# 测试（`round77_trap_asset_admission_contract.rs` G5）与 `--matrix` 输出互证。
# ---------------------------------------------------------------------------
MATRIX_FIELDS = ("library", "index_range", "frame_count", "interval_ms",
                 "duration_ms", "blend", "anchor")


def _read_text(path):
    return read_bytes(path).decode("utf-8")


def _parse_i64_const(text, name):
    prefix = "pub const " + name + ": i64 = "
    start = text.find(prefix)
    if start < 0:
        raise ToolError("缺常量 " + name)
    start += len(prefix)
    end = text.find(";", start)
    return int(text[start:end].strip().replace("_", ""))


def _parse_u32_assign(text, name):
    prefix = name + " = "
    start = text.find(prefix)
    if start < 0:
        raise ToolError("缺赋值 " + name)
    start += len(prefix)
    end = text.find(";", start)
    return int(text[start:end].strip().replace("_", ""))


def _crystal_trap_block(crystal):
    start = crystal.find("case Spell.Trap:")
    if start < 0:
        raise ToolError("Crystal SpellObject.cs 缺 case Spell.Trap:")
    return crystal[start:start + 400]


def rust_matrix_values():
    spellfx = _read_text(SPELLFX_RS)
    frames = _parse_i64_const(spellfx, "MAGIC_FX_FRAMES")
    frame_ms = _parse_i64_const(spellfx, "MAGIC_FX_FRAME_MS")
    cast_ms = _parse_i64_const(spellfx, "CAST_MS")
    magic_hit = "`magic.wzl` 精灵" in spellfx
    generic_over = "scene.draw_effect(frame, kind, frame_index, x, y);" in spellfx
    centered = ("mir_render::CELL_WIDTH" in spellfx
                and "mir_render::CELL_HEIGHT" in spellfx)
    if not (magic_hit and generic_over and centered):
        raise ToolError("spellfx.rs 通用命中路径锚点缺失")
    return {"library": "Magic", "frames": frames, "frame_ms": frame_ms,
            "cast_ms": cast_ms, "blend": "Over",
            "anchor": "centered (CELL_WIDTH/CELL_HEIGHT)"}


def crystal_matrix_values():
    crystal = _read_text(SPELLOBJECT_CS)
    block = _crystal_trap_block(crystal)
    library = "Magic2" if "BodyLibrary = Libraries.Magic2;" in block else None
    blend = "Blend = true;" in block
    base = _parse_u32_assign(block, "DrawFrame")
    count = _parse_u32_assign(block, "FrameCount")
    interval = _parse_u32_assign(block, "FrameInterval")
    draw_start = crystal.find("public override void Draw()")
    if draw_start < 0:
        raise ToolError("Crystal SpellObject.cs 缺 Draw()")
    draw_block = crystal[draw_start:draw_start + 800]
    blend_alpha = "0.8F" if "BodyLibrary.DrawBlend(" in draw_block else None
    if library is None or not blend or blend_alpha is None:
        raise ToolError("Crystal Trap 目标字段解析失败")
    return {"library": library, "base": base, "count": count,
            "interval": interval, "blend": blend, "blend_alpha": blend_alpha}


def crystal_diff_matrix():
    rust = rust_matrix_values()
    crystal = crystal_matrix_values()
    base = crystal["base"]
    count = crystal["count"]
    rows = [
        ("library", rust["library"], crystal["library"],
         "BLOCKED_BY_ASSET_ADMISSION"),
        ("index_range", "0..%d" % (rust["frames"] - 1),
         "%d..%d" % (base, base + count - 1), "DIFF_TARGET_REQUIRED"),
        ("frame_count", str(rust["frames"]), str(count),
         "DIFF_TARGET_REQUIRED"),
        ("interval_ms", str(rust["frame_ms"]), str(crystal["interval"]),
         "DIFF_TARGET_REQUIRED"),
        ("duration_ms", str(rust["cast_ms"]), str(rust["cast_ms"]), "SAME"),
        ("blend", rust["blend"], "Blend(true, %s)" % crystal["blend_alpha"],
         "DIFF_TARGET_REQUIRED"),
        ("anchor", rust["anchor"],
         "DrawLocation (CurrentLocation*CellWidth/CellHeight)", "PARTIAL"),
    ]
    return {
        "schema": "round77-a/crystal-diff-matrix/v1",
        "round": ROUND,
        "subtask": SUBTASK,
        "read_only": True,
        "source": {
            "rust": rel(SPELLFX_RS),
            "crystal_spellobject": rel(SPELLOBJECT_CS),
        },
        "derived": {"rust": rust, "crystal": crystal},
        "fields": list(MATRIX_FIELDS),
        "rows": [dict(zip(("field", "current", "target", "status"), row))
                 for row in rows],
    }


def build_facts():
    default = default_magic2_facts()
    lib = lib_candidate_facts()
    lib_frames = lib["candidates"][0]["frames_2360_2367"]
    lib_dims = [(f["width"], f["height"]) for f in lib_frames]
    verd = {f["frame"]: f["verdict"] for f in default["frames_2360_2367"]}
    if all(v == "MISSING_INDEX_OUT_OF_TABLE" for v in verd.values()) is False:
        raise ToolError("default trap frames unexpectedly indexed")
    corpus = corpus_2025_facts(lib_dims)
    # 地址边界参考（F2 返工）：不改写为「空间不足/全库不存在」的证据。
    # NOT_READY 的独立依据是 WZX 索引表越界；八帧压缩载荷合计约 114.6KB，
    # 默认库 2,637,986B 未索引空间完全足以容纳，尺寸比较不构成不存在证明。
    lib_frames = lib["candidates"][0]["frames_2360_2367"]
    wzl_bytes = default["wzl"]["bytes"]
    trap_payload = sum(f["compressed_len"] for f in lib_frames)
    trap_headers = len(lib_frames) * LIB_RECORD_HEADER_LEN
    default["address_boundary_check"] = {
        "basis": "2025 corpus Trap group start offset vs default magic2.wzl size "
                 "(reference only; NOT an absence proof)",
        "reference_only": True,
        "required_payload_compressed_bytes": trap_payload,
        "required_payload_with_headers_bytes": trap_payload + trap_headers,
        "lib_record_header_bytes": trap_headers,
        "wzl_bytes": wzl_bytes,
        "append_headroom_bytes": default["wzl"]["append_headroom_bytes"],
        "unindexed_gap_total_bytes": default["wzl"]["unindexed_gap_total_bytes"],
        "group_start_offset_2025": corpus["group_boundary"]["group_start_offset"],
        "group_start_beyond_default_wzl_by": (
            corpus["group_boundary"]["group_start_offset"] - wzl_bytes
        ),
        "note": "2025 Trap 记录起始地址在默认 magic2.wzl EOF 之外，故该地址在默认库中"
                "不可直接读取；八帧压缩载荷合计仅 %d B（含 %d×%d B 帧头共 %d B），"
                "默认库未索引空间 %d B 足以容纳——尺寸比较不能证明‘全库不存在’，"
                "NOT_READY 的独立依据是 WZX 索引表越界。"
                % (trap_payload, len(lib_frames), LIB_RECORD_HEADER_LEN,
                   trap_payload + trap_headers,
                   default["wzl"]["unindexed_gap_total_bytes"]),
    }
    default["admission"] = {
        "state": "NOT_READY",
        "can_become_ready": False,
        "blockers": list(default["reason_codes"]),
    }
    return {
        "schema": SCHEMA,
        "schema_version": SCHEMA_VERSION,
        "round": ROUND,
        "subtask": SUBTASK,
        "role": ROLE,
        "generated_utc": utc_now(),
        "read_only": True,
        "scope": {
            "statement": "eight local WZX/WZL corpora only; Magic2.Lib two candidates are "
                         "unbound candidates pending human approval / R78 binding; "
                         "this tool never changes production assets",
            "eight_wzx_hint": list(EIGHT_WZX_HINT),
            "production_changes": False,
            "writes_files": False,
        },
        "default_magic2": default,
        "corpus_2025": corpus,
        "lib_candidates": lib,
        "admission": admission_facts(default, corpus, lib),
        "lib_frames_reference": lib_frames,
    }


def expected_without_volatile(facts):
    """--assert 稳定比较：去掉 generated_utc 等易变字段。"""
    clone = json.loads(json.dumps(facts))
    clone.pop("generated_utc", None)
    return clone


# ---------------------------------------------------------------------------
# --assert / --selftest
# ---------------------------------------------------------------------------
def _compare(label, actual, expected, failures):
    if actual != expected:
        failures.append({"label": label, "expected": expected, "actual": actual})
        print(f"FAIL {label} expected={expected!r} actual={actual!r}")
    else:
        print(f"PASS {label}")


def assert_expected():
    if not EXPECTED:
        print("FAIL expected table is empty; tool not frozen")
        return 1
    failures = []
    facts = build_facts()

    def at(*path):
        node = facts
        for key in path:
            node = node[int(key)] if isinstance(node, list) else node[key]
        return node

    for label, expected in EXPECTED.items():
        actual = at(*label.split("."))
        _compare(label, actual, expected, failures)
    print(f"ASSERT {'PASS' if not failures else 'FAIL'} items={len(EXPECTED)} "
          f"failures={len(failures)}")
    return 0 if not failures else 1


def selftest():
    problems = []
    checks = []

    def check(name, fn):
        try:
            fn()
            checks.append((name, True))
            print(f"SELFTEST PASS {name}")
        except AssertionError as exc:
            problems.append((name, str(exc)))
            checks.append((name, False))
            print(f"SELFTEST FAIL {name}: {exc}")

    def rgb565():
        assert rgb565_rowaligned_bytes(3, 5) == 40, rgb565_rowaligned_bytes(3, 5)
        assert rgb565_rowaligned_bytes(158, 127) == 40132
        assert rgb565_rowaligned_bytes(158, 127) != 158 * 127 * 4

    def state_never_auto_ready():
        for base in ("NOT_READY", "UNVERIFIED_SEMANTICS", "AWAITING_BINDING"):
            for problems in ([], ["BAD_SHA:crystal_sha256"]):
                state, can, _ = decide_admission(base, False, problems, "x")
                assert state == base, (base, state)
                assert can is False
        try:
            attempt_ready("default-magic2")
        except AdmissionError as exc:
            assert exc.code == "AUTO_READY_FORBIDDEN"
        else:  # pragma: no cover
            raise AssertionError("attempt_ready must refuse")
        for subject in ("default-magic2", "corpus-2025", "lib-candidate"):
            try:
                attempt_ready(subject)
            except AdmissionError as exc:
                assert exc.code == "AUTO_READY_FORBIDDEN", exc.code
            else:  # pragma: no cover
                raise AssertionError(subject)

    def state_ready_only_human():
        state, can, remaining = decide_admission("AWAITING_BINDING", True, [], "synthetic")
        assert state == "READY", state
        assert can is True and remaining == []
        state, can, remaining = decide_admission(
            "AWAITING_BINDING", True, ["EVIDENCE_MISSING"], "synthetic")
        assert state == "AWAITING_BINDING" and can is False
        assert "EVIDENCE_MISSING" in remaining
        # 防御：即使进入 READY，也必须有 human_approve
        try:
            decide_admission("READY", False, [], "synthetic")
        except AdmissionError as exc:
            assert exc.code == "AUTO_READY_FORBIDDEN"
        else:  # pragma: no cover
            raise AssertionError("READY without approval must refuse")

    def binding_integrity():
        crystal = sha256_bytes(b"\x00crystal")
        wzx = sha256_bytes(b"\x00wzx")
        wzl = sha256_bytes(b"\x00wzl")
        evidence = b"review evidence bytes"
        binding = {
            "schema_version": 1, "spell": 98, "library": "Magic2", "base": 2360,
            "count": 8, "interval_ms": 100,
            "crystal_sha256": crystal, "wzx_sha256": wzx, "wzl_sha256": wzl,
            "review_evidence_path": "/synthetic/review.md",
            "review_evidence_sha256": sha256_bytes(evidence),
        }
        reader = lambda _path: evidence
        assert validate_binding(binding, crystal, wzx, wzl, reader) == []
        for field in BINDING_FIELDS:
            broken = dict(binding)
            broken.pop(field)
            issues = validate_binding(broken, crystal, wzx, wzl, reader)
            assert f"MISSING_FIELD:{field}" in issues, (field, issues)
        wrong = dict(binding, crystal_sha256="0" * 64)
        assert "BAD_SHA:crystal_sha256" in validate_binding(wrong, crystal, wzx, wzl, reader)
        no_evidence = dict(binding, review_evidence_path="/synthetic/missing.md")
        assert "EVIDENCE_MISSING" in validate_binding(no_evidence, crystal, wzx, wzl, None)
        bad_sha = dict(binding, review_evidence_sha256="0" * 64)
        assert "EVIDENCE_SHA_MISMATCH" in validate_binding(bad_sha, crystal, wzx, wzl, reader)

    def read_only_source():
        source = read_bytes(os.path.abspath(__file__)).decode("utf-8")
        # 用拼接构造标记，避免自匹配；只检测写入类 API。
        opener = "op" + "en("
        forbidden = (("os", ".", "remove"), ("os", ".", "rename"), ("os", ".", "replace"),
                     ("shut", "il."), ("os", ".", "makedirs"), ("os", ".", "mkdir"),
                     ("write_", "bytes"))
        for parts in forbidden:
            token = "".join(parts)
            assert token not in source, f"read-only breach marker: {token}"
        for line in source.splitlines():
            if opener in line:
                assert '"rb"' in line, f"non-read-only open: {line.strip()}"

    def matrix_derivable():
        matrix = crystal_diff_matrix()
        fields = [row["field"] for row in matrix["rows"]]
        assert fields == list(MATRIX_FIELDS), fields
        by_field = {row["field"]: row for row in matrix["rows"]}
        interval = by_field["interval_ms"]
        assert interval["current"] == "60", interval
        assert interval["target"] == "100", interval
        assert interval["status"] == "DIFF_TARGET_REQUIRED", interval
        assert by_field["frame_count"]["target"] == "8", by_field["frame_count"]
        assert by_field["index_range"]["target"] == "2360..2367", by_field["index_range"]
        assert by_field["duration_ms"]["status"] == "SAME", by_field["duration_ms"]
        diffs = sum(1 for row in matrix["rows"]
                    if row["status"] == "DIFF_TARGET_REQUIRED")
        assert diffs >= 4, diffs

    def no_absence_claim():
        source = read_bytes(os.path.abspath(__file__)).decode("utf-8")
        # 拼接构造标记，避免自匹配。
        for parts in (("TRAP_BYTES_", "PHYSICALLY_ABSENT"),
                      ("PHYSICAL_BOUNDARY_", "INSUFFICIENT"),
                      ("physically ", "absent")):
            token = "".join(parts)
            assert token not in source, "F2 已撤销的措辞复活: " + token

    check("rgb565_rowaligned_rule", rgb565)
    check("state_never_auto_ready", state_never_auto_ready)
    check("ready_only_with_human_approval", state_ready_only_human)
    check("binding_integrity", binding_integrity)
    check("read_only_source", read_only_source)
    check("matrix_derivable", matrix_derivable)
    check("no_absence_claim", no_absence_claim)
    passed = sum(1 for _, ok in checks if ok)
    print(f"SELFTEST {'PASS' if not problems else 'FAIL'} checks={len(checks)} "
          f"passed={passed} failed={len(problems)}")
    return 0 if not problems else 1


# ---------------------------------------------------------------------------
# CLI
# ---------------------------------------------------------------------------
def build_arg_parser():
    parser = argparse.ArgumentParser(
        description="R77-A read-only Trap asset admission audit (never writes files)")
    group = parser.add_mutually_exclusive_group()
    group.add_argument("--facts", action="store_true", help="emit measured facts JSON")
    group.add_argument("--assert", dest="assert_check", action="store_true",
                       help="compare against frozen expected")
    group.add_argument("--selftest", action="store_true",
                       help="run built-in self tests")
    group.add_argument("--matrix", action="store_true",
                       help="emit G5 Crystal diff matrix derived from production sources")
    group.add_argument("--attempt-ready", metavar="SUBJECT",
                       help="bypass negative control: must fail EXIT 2")
    group.add_argument("--human-approve", metavar="BINDING_JSON_PATH",
                       help="explicit human approval path (reads binding JSON)")
    group.add_argument("--human-approve-json", metavar="JSON",
                       help="explicit human approval path with inline binding JSON")
    parser.add_argument("--subject", default="lib-candidate",
                        help="approval subject (default lib-candidate)")
    return parser


def main(argv=None):
    # 路径含 CJK 目录名：统一用 UTF-8 输出，避免 Windows 控制台代码页写入非 UTF-8 字节。
    try:
        sys.stdout.reconfigure(encoding="utf-8", errors="backslashreplace")
    except (AttributeError, OSError):  # pragma: no cover - 极旧解释器
        pass
    parser = build_arg_parser()
    args = parser.parse_args(argv)
    try:
        if args.attempt_ready is not None:
            attempt_ready(args.attempt_ready)
        if args.human_approve is not None or args.human_approve_json is not None:
            if args.human_approve is not None:
                raw = read_bytes(args.human_approve).decode("utf-8")
            else:
                raw = args.human_approve_json
            result = human_approve(raw, args.subject)
            print(json.dumps(result, ensure_ascii=False, indent=1))
            if result["state"] != READY:
                raise AdmissionError("READY_REFUSED", result["problems"])
            return 0
        if args.assert_check:
            return assert_expected()
        if args.selftest:
            return selftest()
        if args.matrix:
            print(json.dumps(crystal_diff_matrix(), ensure_ascii=False, indent=1))
            return 0
        facts = build_facts()
        print(json.dumps(facts, ensure_ascii=False, indent=1))
        return 0
    except AdmissionError as exc:
        payload = {"state": "REFUSED", "error": exc.code, "problems": exc.problems,
                   "ready": False}
        print(json.dumps(payload, ensure_ascii=False, indent=1))
        return 2
    except ToolError as exc:
        print(json.dumps({"error": "TOOL_ERROR", "detail": str(exc)}, ensure_ascii=False))
        return 1


if __name__ == "__main__":
    sys.exit(main())
