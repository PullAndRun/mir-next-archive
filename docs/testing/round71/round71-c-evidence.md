# R71-C 刑部独立核验

- **verdict：BLOCK；实现/测试必改 0，证据必改 1。** 五变异及最终冻结红绿独立复现成立；A中间阶段原日志缺失触及C7，Lead须先闭合证据问题或取得门下对该缺口的明确裁决，再推进收口。
- 子单 `JJC-20261027-008-sub-3cd9c7fd`；独立核验者 gpt-6.1-sol；冻结 HEAD `cedf903ef7aaccdf1c220854d9ae7c100f1e791e`。
- 核验UTC：2026-10-27 09:40–09:51；原命令、精确起止UTC/cwd/EXIT/日志SHA见[主运行账](../../../target/round71/c/attempt-1/runs.json)、[M3补跑账](../../../target/round71/c/attempt-1/m3-equivalent-runs.json)、[A红复算账](../../../target/round71/c/attempt-1/a-red-replay-runs.json)。
- 证据根：[C attempt-1](../../../target/round71/c/attempt-1/)；[逐文件bytes/SHA清单](../../../target/round71/c/attempt-1/manifest.json)与[外置摘要](../../../target/round71/c/attempt-1/manifest.sha256)。

|核验项|结论|实录与边界|
|---|---|---|
|范围/诚实|PASS|`git diff f4dfaf8..cedf903 --stat`：仅2文件+1621；协议/服务端diff空；生产+31/−0，旧行为行保留。|
|allow/ignore|PASS|新增行无ignore/allow；spellfx既有allow在144、162、280行（旧版144、162、249），均未新增。|
|freeze/卫生|PASS|两份历史freeze仍未跟踪且前后SHA不变；主树实现/测试未改，无commit/push/全量CI。|
|A红绿|PASS|核对最终测试SHA与2/6红→8/0绿；另在C副本换入f4dfaf8原生产字节，build0→RED101、2/6→恢复SHA→8/8；首败与A完全一致。|
|C物理隔离|PASS|`git -c core.autocrlf=false archive cedf903`提取到C证据内repo；生产/测试nlink均1，SHA与冻结一致；未借用A/B编译物。|
|G1链/映射|PASS|真实encode→decode149→apply→UiState→公开draw_sprites；五事件各一图，Magic1630有图，0/1620/Bolt诱饵无误图。|
|G2时钟/窗口|PASS|120ms×6帧elapsed循环，720回卷、3999映射1633；负age/4000/i64边界不画；不是Crystal严格时钟。|
|G3锚点/裁剪|PASS|格原点+当帧signed offset一次；无二次居中/双加；屏外锚点移入后裁剪；M3/M4各自可失败。|
|G4 Add/逐格|PASS|读取scene→entities→blit_add调用链及frame公式；RGB=min(255,dst+(src×alpha+127)/255)，五格与饱和断言有效。|
|G5兼容/缺帧|PASS|缺目标帧返回false，分支continue仅跳当前事件，不panic、不退0；后续仍绘；非149与其他技能保持原通用Over/居中。|
|B五变异/真红|PASS|五项均独立build0且出现Compiling、8测试RED101、恢复SHA、后绿8/8；失败集合/断言行/首败与B一致。|
|原件保全|FAIL（历史缺口）|C观察期间A 85份、B 145份现存文件均未改；但A manifest:64/76引用的两份c5c7d8中间日志不存在，C7失败attempt全保留尚不能确认。B m1..m5全部并存。|
|A回归R1–R7|PASS（审计）|最终日志分别9/2/2/4/133 passed及clippy/fmt EXIT0；本C未重跑R1–R7，不借此代Lead全量。|
|完整Crystal等价|PARTIAL|rate/DrawColour/Light、严格NextMotion、对象生命周期、真实素材及其它调用点继续承接，详见下文。|

独立T71命令：`cargo test -p mir-client --features mir-render/legacy --test round71_firewall_mapping -- --test-threads=1`；编译证明同命令去尾参数加`--no-run`。
统一环境：`CARGO_TARGET_DIR=target/round71/c/cargo-target`、`TEMP=TMP=TMPDIR=target/round71/c/tmp`（绝对值见environment.json），全程`CARGO_PROFILE_DEV_DEBUG=0`主动设定；未发生D8050，不记环境错误为红。
副本生产SHA `d7f86c29150f51aa865c2d668786bde45c1d7eb06c0172dc06e8d2d3a82343a6`（348行/14779B）；测试SHA `64ff3f597e2530ce0d32abb21eb09b36638083384f27638a0b29177ac262b422`（1590行/59751B）。
[副本审核](../../../target/round71/c/attempt-1/copy-audit.json)、[变异恢复SHA](../../../target/round71/c/attempt-1/mutations.json)、[B/C逐项对照](../../../target/round71/c/attempt-1/comparison.json)均可复查。

|独立变异|前绿/编译/红/后绿 EXIT|红passed/failed|首败像素actual → expected|B对照|
|---|---|---|---|---|
|M1 1630→1620|0/0/101/0；绿均8/8|2/6|(384,224) [248,128,128,255]→[0,0,0,255]，811:5|集合/断言行/首败一致|
|M2 `%6`→clamp|0/0/101/0；绿均8/8|7/1|(379,260) [0,252,248,255]→[0,0,0,255]，909:9、age720|集合/断言行/首败一致|
|M3 锚点减半尺寸（补跑）|0/0/101/0；绿均8/8|2/6|(380,228) [248,0,0,255]→[0,0,0,255]，811:5|集合/断言行/首败一致|
|M4 offset true→false|0/0/101/0；绿均8/8|2/6|(384,224) [248,0,0,255]→[0,0,0,255]，811:5|集合/断言行/首败一致|
|M5 blend→draw_effect|0/0/101/0；绿均8/8|2/6|(384,224) [248,0,0,255]→[0,0,0,255]，811:5|集合/断言行/首败一致|

- 五项结论：**独立复现成立**。M2只败G2 boundaries；其余失败集合F={G1 wire_mapping、G2 frame_boundaries、G3 anchor_offsets/offscreen_crop、G4 add_per_cell、G5 missing_frame}；对应行811/909/1011/1193/1343/1528。
- M5按B实录换draw_effect，同时关闭offset与Add，属耦合变异；不能单凭其红声称隔离了混合因子，Add结论另由源码公式、G4背景饱和及整帧oracle支持。
- 总计28次独立Cargo调用：14次绿（每次8/0/0）、7次断言红（每次running8/ignored0）、7次编译EXIT0；含额外M3一轮和A旧码红复算。无0tests/skip/编译失败冒充红。
- A独立复算首败：811:5，(381,229) actual=[0,0,0,255] expected=[248,0,0,255]，与red-final2文本一致；A两份最终test-sha均为上述冻结值。

独立oracle抽核：`py -3 target/round71/c/attempt-1/oracle.py` EXIT0，UTC 09:44:15；[算式与结果](../../../target/round71/c/attempt-1/oracle.json)。仅从契约与合成夹具字面量另写Python，不读生产混合函数，也不读/复用B oracle程序或整帧导出。
RGB565展开：R=((v>>11)&31)<<3，G=((v>>5)&63)<<2，B=(v&31)<<3；0xF800→[248,0,0,255]，与测试822行字面量一致。
G1北格原点(384,224)+(-3,5)=(381,229)，独算(381,229)/(382,229)/(383,231)/(381,261)均[248,0,0,255]；全黑背景Add保持该值，非黑WZL alpha255、黑色alpha0。
G4背景[40,50,60]+红→[255,50,60,255]，与1357行字面量一致；屏外原点(-48,224)+(50,-30)先移到(2,194)，黄色0xFFE0→[248,252,0,255]后裁剪，与G3预期一致。额外alpha128纯算术探针不冒称G1具半透明素材覆盖。

PARTIAL承接：实际复用R70无调光rate=1 Add；本轮未新增rate API，`src×0.8×alpha`近似也未实施，默认D3D BlendFactor不证明完整等价；DrawColour/INVLIGHT/Light3/mask/tint/shadow仍未闭合。
严格NextMotion、初始100ms取整、每Process最多一步均未传播；来源写实为`NextMotion = CMain.Time + FrameInterval`；4000ms仅瞬态事件窗口，149持久对象存续/26移除尚未迁移。
真实Magic1630–1635素材存在性/设备视觉未验证；非149旧通用行为保留，1620起手未迁；全UI/其它技能调用点与R68/R69及R49–R67历史未结项不清零。

偏差披露：M3首轮误用格内居中`+(CELL-size)/2`，仍真红但不作为B精确复现；后续独立新增m3-equivalent执行器、patch、四日志，原件全部保留。首轮执行器启动后曾改M3表达式，已载入进程不受影响；保留按实际patch重建的verify-executed-snapshot.py，补跑执行器全程固定，测试字节始终未改。
A既有夹具对齐、错误expected、clippy/fmt迭代及HEAD漂移披露已核读；本C未掩盖或覆盖原日志。C自身原始日志与补跑原件不覆盖；累计JSON为实时流水账，最终清单锁定字节。
freeze-before SHA `70845aefd4dd689a55ab0618b36bbf692a4b33ebb5ee2f3a662dedc17a3dc9de`；freeze SHA `c7d896e7f5eee4706b943912c0c564d10d61c98d163143f051c2d632daecde32`，前后相同。
流程偏差：收口主树HEAD已由外部推进至`16644d28df142a0b99264c9700674ff701fdea97`，C未提交；全部Cargo仍用冻结cedf903副本，源/测试SHA复核不变。delegate-result实际EXIT0并留原回执；随后Python打印✅遇GBK UnicodeEncodeError，仅清单收尾中断，以新脚本续做，不重复成功CLI。
实现/测试未发现必改缺陷。证据必改E1：A manifest:64/76引用red-c5c7d8.log、green-c5c7d8.log均不存在；`py -3 target/round71/c/attempt-1/missing-index.py` EXIT0复现exists=false，详见[缺失记录](../../../target/round71/c/attempt-1/missing-index.json)。须追加勘误并追回真实原件；如无法追回，保留缺口并交门下明确裁决C7，不得伪造补历史日志或称全历史已核验。流程回填与EXIT见[CLI回执](../../../target/round71/c/attempt-1/edict-result.json)。
