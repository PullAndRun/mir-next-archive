# R68 门下省对口确认
verdict：**PASS-WITH-PARTIAL，收口成立；必改清单：空（0）**。基线 HEAD `3e4401bd5562a8f5ca3f8ef1574a3172ed281b65`。
范围：仅「125 飞行/570 命中逐帧偏移切片」；合成 WZL + 真实协议/渲染调用链，真实素材/设备视觉签核仍 PARTIAL。
核验1 PASS：初始 status 仅两历史 freeze JSON 未跟踪；`git diff c9e94c4..HEAD -- crates` 空，功能前至 HEAD 的 protocol/server diff 均空。
核验2 PASS：门禁 attempt-2 原字节210789B/SHA `54cc09020c6b724ffa606a22a1634cac4429d1130c73e853e55c2d9f79da3f38`；GB18030解码末尾独立计数12 OK/0 FAIL。
meta 687B/SHA `413dade505b63a5312d9b905b3661233b2312e4b11de0b22b85d43f3034e13b5`，head/EXIT0/UTC/bytes/SHA与日志一致；post-gate补全及失败attempt-1 EXIT1均已披露保留。
核验3 PASS：manifest共18项；原字节SHA `41d14de66f93a3c1a66e0b12f86cce864752d27c2e3516ad362ad51c2537a94a`，外置摘要匹配；独立六件抽核如下（完整SHA/blob见证据）。

| 样本 | bytes | SHA前缀 / blob来源 | 结果 |
|---|---:|---|---|
| README | 7659 | 825ca1fe / cb7bc76 | PASS |
| offset | 3784 | 70aaee54 / cb7bc76 | PASS |
| audit | 5065 | 566c23cf / 48fb28c | PASS |
| gate log | 210789 | 54cc0902 / 原始字节，未跟踪无blob | PASS |
| gate meta | 687 | 413dade5 / 原始字节，未跟踪无blob | PASS |
| crates/entities | 142361 | 96519746 / 6ba5d45 git_blob | PASS |

核验4 PASS：尚书验收33行/4679B/SHA `10e3146cb08f799d0719fee9c8a7b5e4d8faffac5ba75d922bf35f327582bacc`，ACCEPT-WITH-PARTIAL、必改0及限定范围相符；无整轮通过/真实素材签核正面宣称。
核验5 PASS：C evidence/addendum/final三轮报告存续，必改1/2/3全CLOSED；T披露文档批实际12文件、ci.ps1实际命令:120（:118为说明）、额外legacy clippy既有needless_return失败未计通过。
核验6 PASS：全新本attempt cargo-target/tmp（TEMP/TMP/TMPDIR同新tmp）；`cargo test -p mir-client --test round68_image_offset_wire --test round68_effect_offset_live --features mir-render/legacy`，2026-10-27 03:07:28.348–03:08:16.356Z，EXIT0，live2+wire2=4 passed/0 failed/0 ignored；`cargo fmt --all -- --check` 03:08:16.358–03:08:19.632Z，EXIT0。
PARTIAL原样承接：A 旧 manifest:394 措辞勘误；B 共享 isolated 旧日志归档限制；真实素材/设备视觉签核；全 UI offset；spellfx 居中迁移；DrawBlend/shadow/mask；R49–R67 历史遗留。
CLI（2026-10-27 UTC）：show初核主单Review、四子Done/todos空/结果非空；`state JJC-20261027-005 Done --as menxia` 03:08:44Z EXIT0→PendingConfirm（confirm_by=menxia）；`confirm JJC-20261027-005 approve --as menxia` 03:08:44Z EXIT0→Done；`validate --as menxia` EXIT0，216条任务schema v3.1通过；最终show=Done。
历史差异承接：A/B/C看板回填仍含早期SHA/BLOCK，按原始最终证据与C-final闭环确认；未篡改历史。所有CLI实际执行一次，无格式失败。
证据：[runs.json](../../target/round68/menxia/attempt-1/runs.json)、[原字节复算](../../target/round68/menxia/attempt-1/logs/static-audit.json)、[门禁计数](../../target/round68/menxia/attempt-1/logs/gate-count.json)及同目录原始命令日志。仅写本报告及指定attempt目录，未改crates/tests/manifest/gate/验收报告，未commit/push。
