# R74 刑部独立复审附录（T2）
裁决：**PASS-WITH-PARTIAL；E1 CLOSED；剩余必改 0（功能 0、证据 0）**；被审 HEAD `69e8a06aa4381dc6a8fd3f28764e5aafd022ae85`。
依据：[C补证](round74-c-addendum.md)与原始日志/哈希逐项核对；本次独立命令、UTC、EXIT、日志摘要见[runs](../../../target/round74/t2/attempt-1/runs.json)，静态复算见[核验](../../../target/round74/t2/attempt-1/static-audit.json)。
独立隔离：`git -c core.autocrlf=false archive 8df57ef` 解包至 t2/attempt-1/repo；1303 文件 nlink=1、symlink=0；独立 CARGO_TARGET_DIR/TEMP/TMP/TMPDIR，CARGO_PROFILE_DEV_DEBUG=0，features=`mir-render/legacy`。
精确 M3：仅 entities.rs:1916 的 `x.saturating_add(i32::from(info.offset_x))`→`saturating_sub`；y仍add、其余字节不动；[自行生成patch](../../../target/round74/t2/attempt-1/M3-independent.patch)与C原字节、B归一LF逐字节相同。
实算 patch SHA256=`73831540855d867317baabe4bc8e57eb0214e71d466f6c53b9f36f1170bade36`；post=`4f72cae4deaeeb05d125274aff5c54571591bf4644d43bda1f7512a85926f631`；pre/restored=`f8dff727ee970fbe2dc4c8d55f06c7a2587cf84df134c7c4e08a670bc8a7afc6`，见[独立证明](../../../target/round74/t2/attempt-1/independent-proof.json)。
独立实跑：基线5/0/0 EXIT0 → build EXIT0 → 全套1/4/0 RED101 → G2单跑0/1/0（4 filtered）RED101 → 原字节恢复 → 后绿5/0/0 EXIT0；未用编译错、0tests、ignored充红。
全套首败 `r74_g1_cross_library_start_indices`，测试:643:9，像素(8,2)，left/actual=`[253,5,5,255]`、right/expected=`[5,5,5,255]`，见[红日志](../../../target/round74/t2/attempt-1/logs/M3-red.log)。
指定首败 `r74_g2_cross_library_signed_offsets`，测试:797:5，像素(6,9)，left/actual=`[253,6,7,255]`、right/expected=`[5,6,7,255]`，见[G2红日志](../../../target/round74/t2/attempt-1/logs/M3-G2-red.log)。
C十份runs的日志bytes/SHA/计数均吻合；原double确为x/y各add两次（非双轴sub），实算post=`f1ec627817355a59cb926d3cfcbba975065400ae48f174212b062454f7114181`；附录已如实区分x语义与y敏感性，原C错误结论仅由补证纠正。
保全：原C报告SHA `ab7f9904a92e5670b6f9bd6e2c797bf7444cfeba649936341f4d11ac7e33c7d6`、原T报告SHA `60797b827750385bf1a45f223f1b50e416ca11467b246a2a7fcc5f2b69b7f3f3`及原patch未改；两freeze/冻结测试SHA未变；`git diff 8df57ef..HEAD -- crates tools`及主树工作差异空，无新增ignore/allow。
遗留PARTIAL完整承接[原T:26–29](round74-final-audit.md#L26)，含原号GAP/R70–R73等、严格时钟/寿命、完整DrawBlend/BlendMode、rate0.8、其余技能、真实素材/GPU/设备视觉、全UI/跨后端/Live音频；本次仅关闭R74证据E1，不升级历史结论。
范围：只新增本附录与 `target/round74/t2/attempt-1/**`；未改实现/测试/C报告/README/manifest，未commit/push，未运行 `tools/dev/ci.ps1`。
