# R64 刑部独立终审

结论：**BLOCK（证据验收脚本/披露必改）**；限定功能实测为 **PASS-WITH-PARTIAL**，未发现本轮功能行为缺陷。
审计基线 `HEAD=origin/main=7ace28fb3cda28126f839651359bf1fe78f81d66`；功能 `063c6f609e2ee61fc4d3cd8c5d322dcc32c49ce2`。
证据根 `target/round64/t/attempt-1/`；各命令日志含 COMMAND、UTC_START/END、EXIT，独立 JSON 含命令/UTC/exit。

|核验项|判定|实测与证据（路径均相对证据根）|
|---|---|---|
|1 提交/范围|PASS|`scope.log`：5 文件 +905/−34；063c6f6..7ace28f 仅11文档，crates零diff；初始仅两份历史冻结JSON（另本审骨架）。输入603c6f6按有效063c6f6核验。|
|2 门禁真实性|PASS/PARTIAL|`gate-recheck.json`：205352B、SHA256 3e228cef3ce730eac76ce936679b8557308d8d4f20ed1d766dbfb7a0b92b2604，最终摘要12 OK/0 FAIL；meta EXIT=0、HEAD/UTC/bytes/sha/operator全部吻合。唯一执行者依meta署名Lead，日志不能独立证明现实身份。|
|3 独立复跑|PASS|全新 `target/round64/t/cargo-target`；lib **326/0/0**，集成 **4/0/0、3/0/0**；R64 lib7、bin1；R59/61/62/63 **16/11/5/3**，均0 ignored；fmt/clippy exit0。|
|3 临时目录异常|CLOSED|首次全lib因默认TEMP权限拒绝失败，原 `lib.log` 保留；显式TEMP/TMP置证据根tmp后 `lib-temp-recheck.log` 326通过/exit0，不删除失败证据。|
|4 树哈希|PASS|`treehash-compare.log` exit0，8行逐行相同/mismatch=0；7项均063c6f6 blob SHA；README在该提交不存在，MISSING成立。|
|4 manifest|FAIL→刷新|原17项有18字段 mismatch（8新跟踪文档的blob/tracked＋client/roadmap blob）；已刷新当前HEAD事实并增审计第18项，独立复算 mismatch=0，自身不入清单。|
|4/交付 run-checks|FAIL|首轮 **18 PASS/2 FAIL/0 SKIP，exit1**；登记审计后再跑仍 **18/2/0，exit1**，manifest/negative被固定17项断言拒绝，见 `run-checks-after.log`。|
|5 红→绿效力|PASS/PARTIAL|`red-green-recheck.json`：A lib0/5、集成0/4，B lib0/2、bin0/1、集成0/3；全为assert left/right，无桩下通过。A块红绿同SHA e928aaf4…；A集成红阶段SHA与当前9a3e8314…一致（无独立红副本）；B副本逐字节同当前b57c6318…。|
|5 C证据/历史|PASS|C报告85行/8780B/SHA256 30d08ae3c6457417b08c788e4102a6bf2b4d474325e7cefbef7abdcabbc9f416；本轮复跑计数吻合。A块后续仅注释修复，不能把整文件声称最终红绿SHA相同。|
|7 文档/禁称|PARTIAL|README、client §5.31、roadmap §十三/十四阶段一致且未提前PASS；F4换算正确，六项PARTIAL保留；未发现新增正面禁称，ignore/allow增行0、wire零diff；C6实值披露缺口见M2。|

|条件|状态|实现/测试证据|
|---|---|---|
|C1|CLOSED-with-limitation|最终红日志均assert失败，含left/right/行；A块与B副本同SHA；A集成红SHA依历史记录交叉核对。|
|C2|CLOSED|net.rs:4592 ack在InGame gate之后、send之前return；net.rs:9292真实TCP零写＋Turn正对照；Crystal包仅Spell/Source/Destination。|
|C3|CLOSED|net.rs:3030/3046：through<=reclaimed no-op、超尾夹取、total不减、空表不变量；A四集成＋五单测通过。|
|C4|PARTIAL|consume算术逻辑未改，cursor-dropped且dropped=total-len；A证明未确认后缀保留、B证明裁后再push不重放；未见精确“消费半→确认半→再push”UI组合场景。|
|C5|CLOSED|B重连/裁空/中途消费者3集成，A旧代拒绝；新代空快照UI延迟首批非空复位是已声明语义，不外推多消费者。|
|C6|PARTIAL|实值由B集成断言独立复跑确认：首次ack后 reclaimed=2、未确认len=0、total=2；再push未确认态=2/1/3，再ack=3/0/3；原README/C报告未列齐实值。|
|C7|CLOSED|C原始独立target声明与日志路径一致；本审另建t/cargo-target，未复用Lead二进制。|
|C8|CLOSED/PARTIAL|本审命令/UTC/exit/哈希证据齐；原红阶段日志本体不含SHA，以同attempt旁证关联，保留证据来源限制。|
|C9|CLOSED|阶段文档为门禁通过/终审待定/未PASS；本审保留服务端未做、多消费者未证。|
|C10|CLOSED|本审未运行ci.ps1；门禁meta署名Lead，门禁后crates零改动，功能HEAD仍有效。|
|M1|CLOSED|计划原文仍泛称ack，尚书派发显式覆盖；net.rs:707与app.rs:199明确本地进程内非wire，文档同步。|
|M2|CLOSED|桩下通过=0，A/B最终红不用首次编译失败充数，SHA与assert已核。|
|M3|CLOSED|net.rs字段/consume注释已改；total==len/dropped==0只在未回收条件下成立，未遗留无条件假陈述；F4文档区分0-based下标与1-based水位。|
|M4|CLOSED/PARTIAL|app.rs:2679为生产UI接收点；net.rs:3881另有同步wait_for_snapshot接收器，故仅可称“一个UI消费者”，不能称全仓只有一个Snapshot接收点；多消费者承接PARTIAL。|
|M5|CLOSED|app.rs宿主 r64_ack_sent_only_when_watermark_advances 实跑1例；私有next_projectile_ack seam，非0 tests。|
|M6|CLOSED|新增变体需考虑的枚举方法/handle_command两处位于net.rs:731/4312；既有session match :3996通配转发未改；非“全仓只有两个match”，会话循环/快照节奏零改动。|

审计必改项（此处编号为本审A-M，区别原门下M1–M6）：
- **A-M1 / 中等 / 阻塞**：`docs/testing/round64/run-checks.sh:14/18/28` 写死17路径/数量/负控循环。复现：登记18项后执行 `bash docs/testing/round64/run-checks.sh --verify manifest` → `Error: artifact count`；`--verify negative`同失败。建议Lead授权维护该脚本，纳入audit路径并覆盖18项负控，刷新manifest后重跑20/20；本审无脚本写权限，未代改。
- **A-M2 / 中等 / 披露必改**：门下C6要求README/报告分列reclaimed、未确认len、total实值；当前README仅符号与换算，C报告亦无三实值。复现：读取README:13–35、C报告:47–50；`--nocapture`也不打印三值。建议将本审C6已验证的2/0/2、2/1/3、3/0/3补进README，保留测试断言出处并刷新manifest。
- **A-M3 / 轻微 / 覆盖补证**：C4字面规定的半消费组合场景未出现；建议后续只补可失败的精确场景，涉及测试改动须按原门禁规则重新验证，不以当前分项证明宣称该场景已实跑。

PARTIAL承接：单UI消费者；多消费者/克隆快照全局最低水位未证；事件表未限容且backlog可增长；known_positions容量/LRU未实现；服务端回收协议未做；SkipFrames/ActionFeed、素材/设备可听及R61 P01–P12依原编号保留。
门禁日志含历史编码乱码，原字节摘要仍可核；本审仅确认保存证据一致性，不伪称重新执行全量门禁。
边界：仅写本报告、final-manifest.json与target/round64/t/**；未改实现/测试/受保护脚本，未commit/push，未跑ci.ps1或全量build。
