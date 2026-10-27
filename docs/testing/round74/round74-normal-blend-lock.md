# R74 NORMAL 跨库锁与 rate/Mode 差异合同

状态：pre-audit；PASS-WITH-PARTIAL 候选，D 文档批、待独立终审；范围与证据见[索引](README.md)。

## G1–G5 契约摘要

|锁|合同与边界|
|---|---|
|G1|走Magic/Swing/Bolt生产入口：Magic→Magic/start0、Swing→Magic2/start0、Bolt→Magic2/start20；Magic0与Magic2 0/20/21异色/异尺寸wrong-library诱饵，独立整帧expected。|
|G2|两库signed offset仅一次；apply_offset true/false、四侧裁剪、极端坐标、完全屏外与缺帧返回；不把缺帧当合法黑帧。|
|G3|同scene逐步Magic Add→Magic2 Add→Magic Over→Magic2 Add，每步整帧对独立oracle，锁跨库缓存与offset隔离。|
|G4|直接RGBA源[101,53,7,128]叠底[10,20,30,255]，CPU四舍五入得[61,47,34,255]；覆盖a=0/1/127/128/254/255，仅锁rate1，不施加0.8。|
|G5|缺/空帧不污染后继Magic/Bolt与旧sprite Over路径；G5注释措辞非阻塞，独立整帧断言保留，见[裁定](../../process/round74-lead-ruling.md#L13)。|

## Crystal 锚点与差异

- [Effect.cs:8–61](../../../github/Crystal/Client/MirObjects/Effect.cs#L8) 声明Library/Rate/Light，Rate默认1、Light默认6；NextFrame用整数Duration/Count排期。
- [Effect.cs:81–96](../../../github/Crystal/Client/MirObjects/Effect.cs#L81)：`Time <= NextFrame`先return，严格`>`才推进；Owner.SkipFrames可额外加帧，随后按Start重排期。C2只修反向注释，严格NextMotion/寿命仍PARTIAL。
- [MLibrary.cs:658–711](../../../github/Crystal/Client/MirGraphics/MLibrary.cs#L658)：DrawBlend签名含`offSet=false, rate=1`，先offset再裁剪，只保存oldBlend并以默认参数恢复；[748–801](../../../github/Crystal/Client/MirGraphics/MLibrary.cs#L748)含section尾部、缩放Draw及DrawTinted mask，不是独立描边等价证据。
- [DXManager.cs:353–384](../../../github/Crystal/Client/MirGraphics/DXManager.cs#L353)：缓存bool/rate/Mode，分支与AlphaBlendFactor（实际RenderState.BlendFactor）见下表。
- 题面`github/Crystal/Client/MirScenes/DXManager.cs:353-384`不存在；实读路径为上面的MirGraphics目录，不创建错误链接。
- 题面Oracle文档`github/Crystal/Client/MirGraphics/Oracle.cs:7-40`本地不存在，无法作为Light/INVLIGHT已核锚点；Light以Effect:20为据，INVLIGHT以DXManager:372–375为据，Oracle来源缺口继续披露。

|分支/参数|Crystal实读|本轮可裁定内容|
|---|---|---|
|NORMAL，rate1|SourceAlpha / One；SetBlend另写BlendFactor|CPU Add回归锁，非D3D设备状态验证。|
|NORMAL，rate0.8|NORMAL分支因子没有选BlendFactor|不能由设置BlendFactor推导src×0.8×alpha；该近似未实施。|
|INVLIGHT|BlendFactor / InverseSourceColor，BlendOperation.Add|CPU API无Mode；设备状态与Light效果PARTIAL。|
|DrawBlend整体|rate/offset/colour与状态保存恢复，另有DrawTinted/mask路径|Next API无rate/Mode；完整DrawBlend/BlendMode/DXManager等价保持PARTIAL。|

## 验收边界

- A/B/C测试SHA一致；B独立Python解析18整帧，C独立Node解析4个G1整帧及G4取整；两者覆盖数分别披露，详见[C报告](round74-c-evidence.md#L37)。
- 合成WZL RGB565只有alpha0/255，G4半透明来自直接RGBA；game-assets未入库，真实素材/GPU/设备视觉未签核。
- PARTIAL原号及完整承接见[索引台账](README.md#披露与-partial-台账)：GAP-1/2、R70-C2、GAP-5/R70-C1双记、R70-C3、R71时钟/对象、R72 B plan-M4、R73-T1、GAP-3/4/6；R71-C7永久PARTIAL、E1仅补救CLOSED，R69条件5及R68/R49–R70/R61–R67照承。
- 其他技能、严格NextMotion/对象寿命、全UI/跨后端/Live音频仍PARTIAL；本合同不作Crystal等价或整轮验收结论。
