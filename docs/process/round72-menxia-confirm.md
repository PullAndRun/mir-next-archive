# R72 门下省终态确认

- 任务 `JJC-20261027-009`；确认官 menxia（gpt-6.1-sol）；2026-10-27；HEAD=`4def86d`。
- verdict：**PASS-WITH-PARTIAL；收口成立；实现/测试/证据补救必改 0**。

| 抽核项 | 结论 | 独立复核依据与命令/EXIT |
| --- | --- | --- |
| 1 尚书验收 | PASS | 报告32行/4427B/SHA256=`3c731b8fa66f955eab3f6190fcf12a1fa0f2c4806bf964a79c34fca30ee50054`；ACCEPT-WITH-PARTIAL/必改0/限149；runs/checks/test-counts/logs均存在，测试及fmt日志SHA吻合，29/0/0；红绿同测试SHA `58f64ab2d30b69f3aea5c07a8975e988ca842bce45972499a6f93e120958baea`留存链吻合。 |
| 2 提交链/边界 | PASS | `git log --oneline -12`包含指定10提交；`git diff 2a31554 -- crates`为空，均EXIT0。 |
| 3 唯一门禁 | PASS | 原字节212729B/SHA256=`6fe99c93ee0bfaed2ede492071d5c2822415e453df37a3ec6025436410b43a3c`；ASCII行首/秒数计数12 OK/0 FAIL；meta exit0/items12/0/head2a31554吻合。 |
| 4 清单/T/T2 | PASS | bytes+SHA 18/18；tracked blob 14/14对delivery_head=`9586d46`；外置SHA匹配；T blob=`43f46d0866c4e8cb2e446922ea325552d0b49d21`、T2=`59e3d89fb90d9ddb643667061f22957f819f4c72`均在列；14次rev-parse均EXIT0。 |
| 5 freeze/属性/看板 | PASS | 两freeze为44844165B/`70845aefd4dd689a55ab0618b36bbf692a4b33ebb5ee2f3a662dedc17a3dc9de`、45219533B/`c7d896e7f5eee4706b943912c0c564d10d61c98d163143f051c2d632daecde32`；新增ignore/allow=0；父单确认前Review/todos=[]；A/B/C/D Done、todos=[]且委派结果非空；diff/list/show均EXIT0。 |
| 6 CLI终态 | PASS | `state JJC-20261027-009 Done --as menxia` EXIT0→PendingConfirm（show核confirm_by=menxia）；`confirm JJC-20261027-009 approve --as menxia` EXIT0→Done；`validate --json --as menxia` EXIT0/ok=true/tasks236/issues=[]；三次show均EXIT0/Done/todos=[]。 |

- 核验命令：`py -3 target/round72/menxia/verify.py` EXIT0；`py -3 target/round72/menxia/verify.py close` EXIT0；逐命令UTC/cwd/EXIT及日志SHA见[runs](../../target/round72/menxia/attempt-1/runs.json)、[checks](../../target/round72/menxia/attempt-1/checks.json)、[logs](../../target/round72/menxia/attempt-1/logs/)。
- 历史原号：R71-E1补救CLOSED仅指补救，R71-C7两份c5c7d8中间日志不可恢复、R69条件5 a77历史红/隔离/日志缺口均永久PARTIAL；旧BLOCK保留，新证据不冒充历史原件。
- PARTIAL承接：严格NextMotion/初始100ms取整/每Process一步/149持久场/26移除/对象寿命；4000ms仅产品瞬态窗；GAP-1/2、R70-C2 rate0.8/src×0.8×alpha/DrawColour/INVLIGHT/完整DrawBlend；GAP-3/4/6 Light3/mask/tint/shadow/场景offset/AnimationOffset。
- PARTIAL承接：GAP-5/R70-C1其余26技能/1620/其他调用点、R70-C3居中及B plan-M4精确写法残余、legacy lint债；R68/R49–R70/R61–R67原号不清零，全UI/真实素材/像素/GPU/设备/跨后端/Live音频未签核。
- 范围：仅149 PoisonCloud逐格精灵映射有限切片；本次只读抽核现有测试与变异留存链，未重跑全量门禁；确认报告留工作树，未commit/push。
- 偏差：本轮初次脚本默认GBK读取UTF-8 meta EXIT1，次次误定位根目录freeze EXIT1；修正编码/完整路径后抽核EXIT0，两次均在CLI写入前且失败记录保留；工具参数准备错误未写文件。上游尚书cwd无效预检、C2/ruling旧SHA及门禁时间勘误沿原记录保留，不改历史。
