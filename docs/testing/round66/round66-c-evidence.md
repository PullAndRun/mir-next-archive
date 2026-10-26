# R66-C 独立核验证据（attempt-1）

- 子单：`JJC-20261027-003-sub-cf73d939`；刑部 gpt-6.1-sol；最终 verdict：**PASS-WITH-PARTIAL，当前必改 0**。
- 修复追踪：owner 已修复 F1 全集合优先级及新增用例的 Clippy 类型复杂度；[attempt-3](../../../target/round66/c/attempt-3/runs.json) A3/B2/lib1、fmt-all、clippy均 EXIT0，四组优先级探针正确。最终 support SHA `cdf0f3331252427d309b4e367a66c9fd35b87812ecb8afbbdd3ca293c72689d8`；A测试 SHA `62f36a3923d682bc01dcd8a53446c09add97d68e588e7654bbb796b66acd54eb`；B/net未变。初版BLOCK及attempt-2失败均保全。
- 下述主表、旧SHA与57项回归为修复前 attempt-1历史快照；最终A增至3项，等价完整清单58项。最终三项A变异已独立重跑，全部EXIT101且同最终A测试SHA：:252 Ok(4)/Ok(2)、:300 None/Some(NoConsumers)、:513 None/Some(GenerationMismatch)，见 [新红证据](../../../target/round66/c/attempt-3/independent-red-meta.json)。旧support配新测试亦在:407报Some(BelowReclaimed)/Some(GenerationMismatch)。首版报告见 [initial](../../../target/round66/c/attempt-1/round66-c-evidence-initial.md)。
- 基线：`f14a81dc4a1f141dd37d7229d3f623a07e9a0df5`；这是本地契约审计，不能作为 R66 最终通过或生产多消费者证明。
- 实跑 UTC：2026-10-26T21:31:18Z 起；各命令 UTC/原始 EXIT/计数见 [runs.json](../../../target/round66/c/attempt-1/runs.json)，静态命令见 [static-audit.json](../../../target/round66/c/attempt-1/static-audit.json)。
- 环境全新且绝对化：`CARGO_TARGET_DIR=<repo>/target/round66/c/cargo-target`；`TEMP=TMP=TMPDIR=<repo>/target/round66/c/tmp`；独立红用 `c/red-target`。

|必做项|判定|核验事实|
|---|---|---|
|1 范围/白名单|PASS / 披露|唯一 tracked crates diff 为 net 尾部 +138 行；既有内容前缀逐字节相同；新测试仅 A/B 三文件；无新增 ignore/allow；旧测试未改。freeze 既存未跟踪，SHA 不变，见下。|
|2 独立复跑|PASS|指定九组共 30 passed / 0 failed / 0 ignored；fmt、clippy、链接、diff-check 均 EXIT0；补充计划命令另见 runs。|
|3 G1.1|**FAIL**|正常 min/五种单类 Err/Err 不裁剪均有断言，但混合非法成员未遵守全集合错误优先级，见 F1。|
|3 G1.2/G1.3|PASS|真实71/125→min2→(2,2,4)[3,4]→(2,3,5)[3,4,5]；慢 UI 新增 view/cue3，现状 ack4 分支仅 [5]/cue1；差分1对3，水位5≠看过1..5；旧 S4 仍 e1..e4。|
|3 G1.4/G1.5|PASS|floor2 的旧代/晚加入0/空集 fail-closed且调用0，合法2/4→2；同 receiver 取走 S1/S2，返回 S2，try_recv None、command_rx Empty。|
|4 真红与SHA|PASS|A 三红、B m1/m2/m2b/m3均实际 assert + EXIT101；红副本测试 SHA 与绿相同；独立 B min→max复现 suffix 0/2。|
|5 G1.6|PASS|正常生产逻辑/wire/server零 diff；协议 diff 空；四行兼容矩阵如下，参考模型未接生产。|
|6 诚实性|FAIL / PARTIAL|自报计数/历史红实值/SHA一致，README未扩称全局能力；但 A 自报固定优先级已覆盖不成立（F1）。中文仅诊断，无中文 UI 文案断言；历史 PARTIAL 保留。|
|7 交付前静态|PASS|`py -3 tools/docs/check_links.py` EXIT0；`git diff --check` EXIT0；交付后复核记录于静态附录。|

**F1 必改（A，非生产行为缺陷）：** [minimum_confirmed](../../../crates/mir-client/tests/support/round66_watermark.rs#L51) 在单个循环中按成员返回首个错误，违反 [计划:51](../../process/round66-plan.md#L51) 的任一代不匹配优先于任一低水位、任一低水位优先于任一越尾。
- 最小复现：`rustc target/round66/c/priority_probe.rs -o target/round66/c/priority_probe.exe` 后运行 exe；编译/运行 EXIT0，探针直接引用工作树 support，未改实现或测试。
- `minimum_confirmed(7,2,4,&[(7,0),(6,4)])` 实际 `Err(BelowReclaimed)`，应为 `Err(GenerationMismatch)`；反序返回后者。
- `minimum_confirmed(7,2,4,&[(7,5),(7,0)])` 实际 `Err(BeyondTail)`，应为 `Err(BelowReclaimed)`；反序返回后者。
- 处置要求：原 owner 修正全集合校验顺序并新增交叉非法输入与反序断言；保留首败，新 attempt 重做 SHA/红绿/C核验；C 未代修。门下“组合断言可选”不豁免计划明确的语义。

|独立命令（均 `cargo test -p mir-client`）|passed|与 A/B 对照|
|---|---:|---|
|`--test round66_watermark_contract -- --nocapture`|2|A=2，一致|
|`--test round66_consumer_boundary -- --nocapture`|2|B=2，一致|
|`--lib r66_ -- --nocapture`|1|B=1，一致|
|`--test round65_projectile_lifecycle`|5|B=5，一致|
|`--lib r65_`|5|A=5，一致；filtered由326至327，新增G1.5所致|
|`--test round63_projectile_wire` / `--test round62_projectile_wire`|3 / 5|C新增独立回归|
|`--test round64_event_reclaim` / `--test round64_projectile_ack`|4 / 3|C新增独立回归|

- 其余：`--bin mir-client r65_` 1 passed、`--lib ui::projectile::tests::` 26 passed（补充后共57项）；`cargo fmt --all -- --check` EXIT0。`cargo fmt -p mir-client -- --check`、`cargo clippy -p mir-client --all-targets -- -D warnings` 均 EXIT0，与 A/B一致。0 tests/编译失败/既有失败未算通过。
- 红原件逐项：A m1 :248 `Ok(4)/Ok(2)`；m2 :296 `None/Some(NoConsumers)`；m3 :418 `None/Some(GenerationMismatch)`；B m1 :190 `0/2`、m2/m2b :205 `1/3`、m3 变异 net :9916 `command_rx` 非 Empty（布尔 assert 无左右字段）。
- A/B `sha256.txt` 全清单复算无错；红副本 A 测试 SHA `1de8162d047667062fe2b862cecb2af816879000a584eab102d9540b03a55902`，B `727d25f5a0dac0c475c6de9fd09f9bd32a5419707fdd57bace9ffabdefe76d52`，均等于绿。
- 独立红：`git archive f14a81d` → [red-copy](../../../target/round66/c/red-copy/) 覆盖 A/B 四文件，仅 support min→max；原测试字节不变；UTC 21:32:12–21:33:00Z，EXIT101，运行1项/失败1项，:190 suffix left0/right2；非编译错或 skip。见 [independent-red-meta.json](../../../target/round66/c/attempt-1/independent-red-meta.json)。

|G1.6 兼容对象|源码/测试与边界|
|---|---|
|单 app UI|[app:210](../../../crates/mir-client/src/app.rs#L210)仅非零前进单水位；[net:3070](../../../crates/mir-client/src/net.rs#L3070)校代后裁；单成员参考等于原水位，未调用参考模型。|
|已存在 clone|[G1.3:496](../../../crates/mir-client/tests/round66_consumer_boundary.rs#L496)旧 S4 值独立；clone不注册、不钉住权威表；裁后晚读仍漏历史。|
|同步 waiter|[net:3853](../../../crates/mir-client/src/net.rs#L3853)、[3869](../../../crates/mir-client/src/net.rs#L3869)同Receiver竞争；[G1.5:9850](../../../crates/mir-client/src/net.rs#L9850)证明不ack，未实现广播/fan-out。|
|未来显式消费者|[模型:39](../../../crates/mir-client/tests/support/round66_watermark.rs#L39)仅测试假设集合；[G1.1:248](../../../crates/mir-client/tests/round66_watermark_contract.rs#L248)快4慢2→2、慢0阻止裁剪；注册/退出/租约/补发/生产接线仍PARTIAL。|

|工作树文件|行 / B|SHA256|
|---|---|---|
|[support](../../../crates/mir-client/tests/support/round66_watermark.rs)|65 / 3210|`38942f158f933b518cbef4115f56ac6f476b1c4594292852f5da3b1febe0834a`|
|[A测试](../../../crates/mir-client/tests/round66_watermark_contract.rs)|460 / 17208|`1de8162d047667062fe2b862cecb2af816879000a584eab102d9540b03a55902`|
|[B测试](../../../crates/mir-client/tests/round66_consumer_boundary.rs)|501 / 20922|`727d25f5a0dac0c475c6de9fd09f9bd32a5419707fdd57bace9ffabdefe76d52`|
|[net](../../../crates/mir-client/src/net.rs)|9917 / 404857|`927178e2a557d17cebfbf73e65dc6815a14d5325ae2889975b0c3c942670aa32`|

披露：UI冻结1000ms、初读drain出生cue并记view基数均落实；仅新 view/cue比较，无命中时钟或真实可听外推。A README“ack后reclaimed推进到2”措辞应理解为保持已存在floor2，不证明新增回收。
- 两个 freeze 从初始status即为 `??`，因此“status不出现”字面条件不满足；与派发前 SHA/字节完全相同且未暂存/覆盖，保全证据充分，不另列实现必改。
- C首次 runner 根目录计算错误，在仓库父目录生成失败日志；已原样移入 `target/round66/c/harness-initial-cwd-error`，未计作红/绿；随后正确 cwd 从全新 C target 实跑。
- R49–R65 按原轮原编号承接，尤其 R61 P01–P12/R60 P-1–P-6、R62四项、R63 P1–P7、R64六项与R65 G2；全局最低水位/生产多消费者、容量/LRU、服务端回收、饱和恢复、素材音频/Live仍 PARTIAL。
- C未 commit/push，未运行 ci.ps1，未改工作树实现/测试；缺陷均退原owner修复，最终复核必改0，可交Lead；仍须Lead唯一门禁及D/尚书/门下后续验收，不宣称R66最终通过。
