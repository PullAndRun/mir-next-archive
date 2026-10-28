# R77 Trap 素材准入与映射差异合同

- 主单 `JJC-20261028-004`；pre-audit / PASS-WITH-PARTIAL；生产零改。仅[合同测试](../../../crates/mir-client/tests/round77_trap_asset_admission_contract.rs)与[只读工具](../../../tools/testing/round77_asset_admission.py)新增；[验证链及偏差](README.md)。
- G1：default magic2.wzx 5968B、declared1460、slots1480；2360..2367均缺索引/越界，2360需9488..9492，NOT_READY独立成立。
- G2：2025 magic 同号八帧索引、帧头、zlib解压、连续链 `[13316944,13357626)`；解压实测 `((((w*16)+31)>>5)*4)*h`，为RGB565行四字节对齐；不等于w*h*4，Crystal语义未证。
- G3：同号不授权替代；2025尺寸66..176×48..149与Lib396..432×363不匹配，状态UNVERIFIED_SEMANTICS。
- G4：两份Magic2.Lib候选字节一致21091901B/SHA `f449aa4c96632f13454d7746ab47678c06175be02e69a6dc8fdfd036755fc0d2`、v2/count2790；2360..2367 gzip解压w*h*4=574992..627264B；邻居2357/58/59/2368/69为4×1空帧；绑定未证，AWAITING_BINDING。
- G5：内联矩阵↔Rust通用非149 Trap源码解析↔工具JSON三方交叉；target取Crystal SpellObject.cs:132–138；F1两负控100→120及8→7均唯一G5红101后恢复七绿。
- G6：状态机不允许从审计事实自动升READY；`--attempt-ready` EXIT2，`AUTO_READY_FORBIDDEN`。
- G7：R76十八帧精确集、R59的116帧冻结集合文本不变；保留R76测试SHA `8b680973…`、R59冻结指纹`e642330a…`，本轮无supersede。

| 项 | current：Rust通用非149 Trap | target：Crystal Trap | 状态 |
| --- | --- | --- | --- |
| library | Magic | Magic2 | BLOCKED_BY_ASSET_ADMISSION |
| index_range | 0..9 | 2360..2367 | DIFF_TARGET_REQUIRED |
| frame_count | 10 | 8 | DIFF_TARGET_REQUIRED |
| interval_ms | 60 | 100 | DIFF_TARGET_REQUIRED |
| duration_ms | 700 | 700 | SAME |
| blend | Over | Blend(true,0.8F) | DIFF_TARGET_REQUIRED |
| anchor | centered(CELL_WIDTH/HEIGHT) | DrawLocation(CurrentLocation*CellWidth/CellHeight) | PARTIAL |

- 准入状态按证据区分：default→NOT_READY（reason_codes=`MISSING_INDEX`/`OUT_OF_INDEX_TABLE`）；2025→UNVERIFIED_SEMANTICS；Lib→AWAITING_BINDING。后两者都不能凭同号、可解压或邻居空帧自动转READY。
- F2修正双证口径：索引越界为独立拒绝依据；2025组地址13316944>default magic2.wzl EOF10290606，差3026338B，仅为`reference_only`地址不可读参考，代码`CORPUS_ADDRESS_BEYOND_DEFAULT_WZL_EOF`，不得转写为全库不存在。
- 已撤销“未索引空间2637986B<解压RGBA4809024B⇒物理不可能”的旧论证；八帧压缩载荷114603B（含8×17B帧头114739B）远小于未索引空间；selftest `no_absence_claim`防止旧断言复活。
- 候选路径：`resource/community/mirfiles-default-crystal/Default Crystal/Data/Magic2.Lib`与`resource/community/mirfiles-crystal-patch/crystal__patch_gz__Data__Magic2.Lib`。本轮不认证；门下M1仍OPEN，由R78做真实来源/语义绑定；18→26 supersede待审。

复现用法（在仓库根分别执行；此处记录既有验证方法，D不重跑工具或cargo）：

```text
py -3 tools/testing/round77_asset_admission.py --facts
py -3 tools/testing/round77_asset_admission.py --assert
py -3 tools/testing/round77_asset_admission.py --selftest
py -3 tools/testing/round77_asset_admission.py --matrix
py -3 tools/testing/round77_asset_admission.py --attempt-ready
```

- 既有结果：facts/matrix只读；assert82项、selftest7项；attempt-ready预期EXIT2。模式互斥，合并`--matrix --selftest --assert`会EXIT2，必须分开；不是一条合并复现命令。
- A冻结/B复算digest canonical：工具EXPECTED字典、json sort_keys+compact、ensure_ascii=false；摘要 `0f66806c5b59c4f9969c27fb0cf822033567871c442f31135471320d3386c881`，B oracle300/300、0 mismatch；C2b仅抽核声明。
- PARTIAL原号照承：门下M1、2025解压语义、anchor、严格Crystal等价/设备视觉/全部技能素材，以及[历史R76及其承接](../round76/README.md)。不得称R77通过、READY、已认证、生产已修或素材已可用。
