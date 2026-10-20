# 音效可行性调研（第 22 轮）

日期：2026-10-20（本机实测）｜基线 HEAD `9d7dad1`｜范围：**只调研，不写源码**。
被测素材：`resource/` 里的 Mir1/Mir2 WAV 三套目录；播放后端候选：**rodio 0.22.2 +
cpal 0.17.3**（客户端已在用，见 §5）。

## 0. 结论先行

| 判定项 | 结果 |
|---|---|
| 文件头可解析 / rodio 支持 | ✅ 三目录 **2917 个 WAV 全部解析通过**（含 6 个 8-bit、4 个 32-bit float）；9 个样本解码并实际播放 |
| 事件/文件名映射 | ✅ **2812/2917** 个文件可按 `SoundList.lst` 索引或 Crystal 文件名公式定位（§1.4）；Crystal 124 个常量里 109 个有文件、核心 12/12、武器 15+8 变体全有。**但根目录 `resource/wav/sound.lst` 是 Mir1 表，必须用 Crystal 表**（§5.2） |
| 授权 | ✅ **按项目所有者裁决（2026-10-20）**：`resource/` 内资源可直接使用，不再逐目录核验（§11） |
| Windows 实际播放 | ✅ 4 个输出设备、默认「扬声器」，`rodio_open_default_sink=ok`，9 个候选各播 300ms 成功 |

**按三项判定（授权已由所有者裁决解除）：可直接接 = 2812；有素材但不可直接接 = 105；无素材 = 2 个 Crystal 文件 + 11 个事件常量。**
剩余 105 个是 `SoundList.lst` 未引用、文件名也不符合 Crystal 公式的命名文件（BGM/登录/UI 类），
需要显式映射表；24 个同名不同内容仍是工程选型风险（§9），不影响「可播放」。

---

## 1. 素材清点（可复跑）

### 1.1 按目录统计（实测输出）

```powershell
$roots = 'resource\wav',
         'resource\community\mirfiles-crystal-client\Client\Sound',
         'resource\community\gqf2008-crystal\Sound'
Get-ChildItem $roots -File -Filter *.wav |
  Group-Object DirectoryName |
  ForEach-Object { [pscustomobject]@{ Directory=$_.Name; Count=$_.Count;
      SizeMB=[math]::Round((($_.Group | Measure-Object Length -Sum).Sum)/1MB,2) } } |
  Format-Table -AutoSize
```

```text
Directory                                                                                       Count SizeMB
---------                                                                                       ----- ------
C:\...\resource\community\gqf2008-crystal\Sound                                                  1572 233.86
C:\...\resource\community\mirfiles-crystal-client\Client\Sound                                    560 279.86
C:\...\resource\wav                                                                               785 207.39
```

合计 **2917 个 WAV、721.11 MB**；按 basename 去重后 **2291 个唯一文件名**（三套目录有 534 个重名，
其中 510 个内容完全相同、**24 个同名不同内容**，见 §7 风险）。

补充：`resource/wav` 里还有 1 个 `sound.lst`（19 KB，Mir1 索引表，不是 WAV）。

### 1.2 扩展名/格式分布（文件头解析，不是看后缀）

| 目录 | 数量 | 大小 | PCM16 | 8-bit PCM | 32-bit float | 采样率分布 |
|---|---:|---:|---:|---:|---:|---|
| `resource/wav` | 785 | 207.39 MB | 778 | 3 | 4 | 44.1k×774、22.05k×5、48k×4、11.025k×1、16k×1 |
| `mirfiles-crystal-client/.../Sound` | 560 | 279.86 MB | 560 | 0 | 0 | 44.1k×524、16k×30、22.05k×6 |
| `gqf2008-crystal/Sound` | 1572 | 233.86 MB | 1569 | 3 | 0 | 44.1k×1553、22.05k×18、48k×1 |

**解析失败 0 个**（RIFF/WAVE + `fmt ` 块齐全）。格式分布汇总：PCM16 **2907**、PCM8 **6**、
IEEE float32 **4**；单声道 **2194**、双声道 **723**。

### 1.3 复跑脚本（临时，不入库）

本次用 `target/audio_scan_all.py`（RIFF `fmt ` 块解析 2917 个文件，输出
`target/audio-all-roots.json`）；核心逻辑：

```python
import struct, pathlib
for p in pathlib.Path('resource/wav').glob('*.wav'):
    b = p.read_bytes()[:4096]
    assert b[:4] == b'RIFF' and b[8:12] == b'WAVE'
    i, fmt = 12, None
    while i + 8 <= len(b):
        cid, sz = b[i:i+4], struct.unpack('<I', b[i+4:i+8])[0]
        if cid == b'fmt ': fmt = b[i+8:i+8+sz]
        if cid == b'data': break
        i += 8 + sz + (sz & 1)
    audio_format, channels, rate, byte_rate, align, bits = struct.unpack('<HHIIHH', fmt[:16])
```

> 说明：`resource/` 被 `.gitignore` 排除，脚本与 JSON 都在 `target/`，**没有提交源码改动**。

### 1.4 映射覆盖率（决定「可直接接」数量）

按 Crystal `SoundManager` 的两条定位规则（`SoundManager.cs:85-92`）：

1. **查 `SoundList.lst`**（索引 → 文件名）；
2. 查不到时走**文件名公式**：`index<=20000 → {index/10:000}-{index%10}.wav`，
   `index>20000 → M{(index-20000)/10}-{index%10}.wav`。

对 2917 个文件分类（`py target/audio_mapped_counts.py` / `audio_formula.py` 复跑）：

| 映射方式 | 唯一文件名 | 物理文件数 | 说明 |
|---|---:|---:|---|
| `SoundList.lst` 直接引用且存在 | 859 | 1177 | Crystal 表 318 个 + Mir1 表 622 个（有交叉） |
| 文件名符合公式（可被索引定位） | 1329 | 1635 | 例：`1000-2.wav`→10002、`M34-1.wav`→20341 |
| **小计：有映射 / 可直接接** | **2188** | **2812** | 两项并集 |
| 无表引用且不符合公式 | 103 | **105** | BGM/登录/UI 命名文件，需显式映射；清单见 `target/audio-formula.json` 的 `still_sample` |
| 合计 | 2291 | 2917 | — |

> 105 个「无映射」样例：`Field2.wav`、`main_theme.wav`、`Game over2.wav`、`log-in-long2.wav`、
> `SelectBoxFlash.wav`、`S1-1.wav`、`bpz_injured.wav` 等。它们**能解码播放**，只是没有
> `SoundList.lst` 条目、文件名也不符合公式，需要一张显式表（或按 BGM 目录规则）才能接。

**同名不同内容 24 个**：这些名字在上表里算「有映射」，但取文件时必须按目标表指定的目录/版本，
不能随便拿一个同名文件（§9 附录 A）。

## 2. 文件头抽样（≥5 个，实测解析值）

| 文件 | 容器/编码 | 采样率 | 声道 | 位深 | 时长 | 大小 |
|---|---|---:|---:|---:|---:|---:|
| `resource/wav/1.wav` | RIFF/WAVE, PCM | 44100 | 1 | 16 | 0.40 s | 35,328 B |
| `resource/wav/10.wav` | RIFF/WAVE, PCM | 44100 | 1 | 16 | 0.57 s | 50,136 B |
| `resource/wav/M42-2.wav` | RIFF/WAVE, PCM | 44100 | 2 | 16 | 1.54 s | 272,128 B |
| `resource/wav/Field2.wav` | RIFF/WAVE, PCM | 44100 | 2 | 16 | 191.47 s | 33,776,016 B |
| `resource/wav/2101-2.wav` | RIFF/WAVE, PCM | 44100 | 2 | 16 | 1.27 s | 224,024 B |
| `resource/wav/2420-6.wav` | RIFF/WAVE, PCM（8-bit） | 11025 | 1 | 8 | 0.98 s | 10,902 B |
| `resource/wav/cyclone.wav` | RIFF/WAVE, PCM（8-bit） | 22050 | 1 | 8 | 0.97 s | 21,329 B |
| `resource/wav/火蛇_缩混1.wav` | RIFF/WAVE, IEEE float | 48000 | 2 | 32 | 9.87 s | 3,800,898 B |
| `resource/wav/狂雷紫电_缩混.wav` | RIFF/WAVE, IEEE float | 48000 | 2 | 32 | 6.06 s | 2,336,820 B |

**结论**：全部是标准 RIFF/WAVE；rodio 的 `wav` feature 实际解码通过（§3）。8-bit 与 32-bit
float 属少数派（6 + 4 个），但都能解——不是「只看扩展名」得出的结论。

## 3. 播放冒烟（≥2 个候选，实测）

程序：`target/audio-smoke`（临时 crate，**未提交**；`rodio 0.22` + `cpal 0.17`，
`cargo build --release` 产物 0.86 MB）。命令：

```powershell
cargo run --offline --manifest-path target\audio-smoke\Cargo.toml
```

实测输出（`target/audio-smoke.out`，2026-10-20 23:1x）：

```text
output_devices=4
device[0]=Q27G4SLM6
device[1]=扬声器
device[2]=DELL U2414H
device[3]=扬声器
default_output_device=扬声器
rodio_open_default_sink=ok
resource/wav/1.wav: decode_ok channels=1 sample_rate=44100 duration=Some(0.40004534) play=ok(300ms)
resource/wav/10.wav: decode_ok channels=1 sample_rate=44100 duration=Some(0.56793654) play=ok(300ms)
resource/wav/M42-2.wav: decode_ok channels=2 sample_rate=44100 duration=Some(1.5424263) play=ok(300ms)
resource/wav/Field2.wav: decode_ok channels=2 sample_rate=44100 duration=Some(191.47276) play=ok(300ms)
resource/wav/2101-2.wav: decode_ok channels=2 sample_rate=44100 duration=Some(1.269728) play=ok(300ms)
resource/wav/2420-6.wav: decode_ok channels=1 sample_rate=11025 duration=Some(0.9847619) play=ok(300ms)
resource/wav/cyclone.wav: decode_ok channels=1 sample_rate=22050 duration=Some(0.9653061) play=ok(300ms)
resource/wav/火蛇_缩混1.wav: decode_ok channels=2 sample_rate=48000 duration=Some(9.872729) play=ok(300ms)
resource/wav/狂雷紫电_缩混.wav: decode_ok channels=2 sample_rate=48000 duration=Some(6.0599165) play=ok(300ms)
```

* **设备情况**：本机有 4 个输出设备、默认「扬声器」，不是无声卡环境；因此完成了「真实解码 +
  真实送混音器播放」。**未做人工听音确认**（只验证解码与混音器接受，不判断音色是否合适）。
* 无声卡/CI 的降级路径在客户端代码里已存在：`SoundPlayer::without_device` +
  `rodio_open_default_sink` 失败只 `eprintln!` 一次、`is_active=false`，窗口照常运行
  （`crates/mir-client/src/audio.rs` 模块文档与单元测试）。本次**未触发**该分支，只做代码核对。

## 4. rodio / cpal 评估

| 项 | rodio 0.22.2 | cpal 0.17.3 |
|---|---|---|
| 许可证 | **MIT OR Apache-2.0**（与 GPL-3.0 项目兼容） | **Apache-2.0** |
| Windows 额外依赖 | 无外部 DLL/运行库；经 cpal 调 **WASAPI**，`windows` crate 纯 Rust 绑定 | `windows` crate（`Win32_Media_Audio`），无安装包 |
| 依赖树（normal，含传递） | 约 **44 个 package**（大部分是 `windows-*` 与 `symphonia-*`；本机实测 `cargo tree`） | 已含在上行 |
| 预估体积增量 | 最小播放器 release 二进制 **0.86 MB**（含解码器）；接入现客户端预计 **<1 MB** | — |
| 维护状态 | RustAudio 组织，版本为当前锁定版（`Cargo.lock` 0.22.2 / cpal 0.17.3）；未做长期活跃度审计 | 同上 |

**对本项目**：`crates/mir-client` 已经依赖 rodio（features `playback` + `wav`），**没有新增
第三方依赖**；本次临时冒烟 crate 与现有依赖版本一致，证明同一组合可解码播放。

## 5. 事件 / 文件名映射

### 5.1 Crystal 的名词表（`Client/MirSounds/SoundList.cs`，124 个常量）

`SoundList.lst` 是 `索引: 文件名` 表（`SoundManager.cs:85-92`）。用社区里的 Crystal 完整副本
`resource/community/gqf2008-crystal/Sound/SoundList.lst`（450 条有效索引）对照：

* 124 个常量中 **111 个在表里有索引**（13 个没有：`Music`/`None`（索引 0，非文件）、
  骑乘 Horse、雪地 Walk/Run、宠物 AngryBird/Foxey/MedicalRat、`Revive` 等）；
* 111 个有索引的常量里 **109 个能找到实际文件**；
* **缺 2 个玩家可见文件**：`ZombieRevive.wav`、`pet_frog.wav`（§5.3）。

核心动作/界面音（本作 `mir-render::sound` 的 12 类）全部命中：

| 事件 | Crystal 常量 | 索引 | 文件 | 在哪个目录 |
|---|---|---:|---|---|
| 走 | `WalkGroundL` | 10001 | `1.wav` | 3 套都有 |
| 跑 | `RunGroundL` | 10003 | `3.wav` | 3 套都有 |
| 挥空 | `SwingShort` | 10050 | `50.wav` | 3 套都有 |
| 命中（身体-剑） | `StruckBodySword` | 10070 | `70.wav` | 3 套都有 |
| 受击（男/女） | `MaleFlinch`/`FemaleFlinch` | 10138/10139 | `138.wav`/`139.wav` | 3 套都有 |
| 死亡（男/女） | `MaleDie`/`FemaleDie` | 10144/10145 | `144.wav`/`145.wav` | 3 套都有 |
| 升级 | `LevelUp` | 10156 | `levelup.wav` | 仅 gqf2008 |
| 拾取 | `PetPickup` | 10520 | `pet_pickup.wav` | 仅 gqf2008 |
| 金币 | `Gold` | 10106 | `106.wav` | 3 套都有 |
| 喝药 | `ClickDrug` | 10108 | `108.wav` | 3 套都有 |
| NPC 对话 | `ButtonA` | 10103 | `103.wav` | 3 套都有 |
| 施法（火球） | 技能公式 `20000+Spell*10` | — | `M31-0.wav` | mirfiles + gqf2008 |

**武器/动作细分**：Crystal 声明的 8 种挥舞（短/木/剑/剑2/斧/棍/长/拳）与 15 种命中/护甲
变体（含 `StruckBodyAxe`/`LongStick`/`Fist` 等）**全部有文件**（109/111 覆盖的一部分）——
只是当前客户端只用了其中 1 个「剑」变体，要按武器细分仍有映射可接。

### 5.2 ⚠️ 根目录的 `sound.lst` 不是 Crystal 表

`resource/wav/sound.lst` 的索引与 Crystal **对不上**：

| 索引 | `resource/wav/sound.lst` | Crystal `SoundList.lst` |
|---:|---|---|
| 10050 | `M5-1.wav` | `50.wav`（`SwingShort`） |
| 10520 | `M52-0.wav` | `pet_pickup.wav`（`PetPickup`） |
| 10001 | **无** | `1.wav`（`WalkGroundL`） |

**证据（原文，可自行复跑）**：

```powershell
Get-Content resource\wav\sound.lst -TotalCount 8
```

```text
1:	wav\1.wav
2:	wav\2.wav
3:	wav\3.wav
4:	wav\4.wav
5:	wav\5.wav
6:	wav\6.wav
7:	wav\7.wav
8:	wav\8.wav
```

```powershell
Get-Content resource\community\gqf2008-crystal\Sound\SoundList.lst -TotalCount 8
```

```text
;Movement
10001:  1.wav
10002:  2.wav
10003:  3.wav
10004:  4.wav
10005:  5.wav
10006:  6.wav
10007:  7.wav
10008:  8.wav
```

同一 id 的对照（两条命令分别 grep 两个文件）：

```powershell
Select-String -Path resource\wav\sound.lst -Pattern '^\s*(10001|10010|10050|10520)\s*:'
Select-String -Path resource\community\gqf2008-crystal\Sound\SoundList.lst -Pattern '^\s*(10001|10010|10050|10520)\s*:'
```

```text
# resource/wav/sound.lst 实际输出
10010:  wav\M1-1.wav
10050:  wav\M5-1.wav
10520:  wav\M52-0.wav
# 10001 无匹配（该索引在根目录表里不存在）

# Crystal SoundList.lst 实际输出
10001:  1.wav
10010:  10.wav
10050:  50.wav
10520:  pet_pickup.wav
```

**更直观的差异**：同一 `10010`，根目录表给 `M1-1.wav`（怪物 1 的变体 1），Crystal 表给
`10.wav`（第 10 个基础动作音）。

**结论**：根目录表从 `1.wav` 起编号（`1:`、`2:`…），并把 `10050` 指向 `M5-1.wav`；
Crystal 表从 `10001` 起按动作编号（`10001`=走）并把 `10050` 指向 `50.wav`。两者不是同一套
索引，**混用会让每个音效都放错**。文件名本身也有交叉（两套都有 `1.wav`），所以只检查
「文件存在」发现不了这个错。

Mir1 表自身：753 条有效映射、677 个唯一文件；三套目录合起来仍缺 **60 条引用**（57 个唯一文件），
另有 **180 个 WAV 不在 Mir1 表里**（其中一部分由 Crystal 的 `SoundList.lst` 引用，不是全都无主）。

### 5.3 无素材清单

* Crystal 语义缺失（2 个文件）：`ZombieRevive.wav`、`pet_frog.wav`；
* 事件常量层缺失（11 个，不全是 WAV）：骑乘 `HorseWalkL/R`/`HorseRun`、雪地
  `WalkSnowL/R`/`RunSnowL/R`、宠物 `PetAngryBird`/`PetFoxey`/`PetMedicalRat`、
  `Revive`（另有 `Music`/`None` 索引 0，不是文件）；
* Mir1 表缺失的 60 条（57 个唯一文件名）：以 `M*.wav` 为主
  （`M1-1/M1-2/M5-1/M10-1/M100-2/…`），另有 `1500-3`、`2100-3`、`2140-1`、`2150-1`、
  `290-4`、`9220-4`、`game-over2.wav` 等；完整清单见 `target/audio-mapping2.json`。

## 6. 三类数量（按三项判定；授权已由所有者裁决解除）

| 类别 | 数量 | 说明 |
|---|---:|---|
| **可直接接** | **2812 个 WAV**（2188 个唯一名） | 文件头/rodio 全过；可按 `SoundList.lst` 或文件名公式定位（§1.4）；9 个抽样实际播放通过。**其中 24 个同名不同内容需按目标表选版本**（§9） |
| **有素材但不可直接接** | **105 个 WAV**（103 个唯一名） | 无 `SoundList.lst` 条目且不符合公式（BGM/登录/UI 命名文件），需显式映射后才能接（§1.4 清单） |
| **无素材** | **57 个被引用文件名** + **11 个事件常量** | 见 §5.3；其中 Crystal 直接可见的是 `ZombieRevive.wav`、`pet_frog.wav` 两个文件 |

> 结论：**判据从四项变三项后，可直接接从 0 跳到 2812/2917**；授权不再是阻塞项。
> 残余工程风险是：24 个同名不同内容（选错会播错版本）、105 个命名文件需手工映射、
> 2 个 Crystal 文件缺失、Mir1 表不能当 Crystal 表用（§5.2/§8）。
> 发布包是否携带 `resource/` 仍是打包策略问题（`package.ps1` 当前不打包素材），不属授权判定。

## 7. 接进来要几步

1. **授权（已解除）**：按项目所有者裁决（2026-10-20），`resource/` 内资源可直接使用，
   **不再逐目录核验**；`SOURCES.md` 仅保留为溯源记录（§11）。
2. **选主版本/去重**：三套目录 534 个重名，510 个内容相同、**24 个同名不同内容**
   （如 `100.wav`、`300-1.wav`、`M34-1.wav`、`M38-0.wav`）。建议固定一个规范根目录
   （现在客户端顺序是 `resource/wav` → mirfiles → gqf2008），并记录每个重名的取舍。
3. **用对索引表**：Crystal 语义用 `gqf2008.../SoundList.lst`，不要用根目录 Mir1 的
   `sound.lst`；把 `mir-render::sound` 从 12 类事件扩到 124 个常量（已覆盖 8 种挥舞 /
   15 种命中变体）。**这一条是第 23 轮播放层的核心设计输入**。
4. **补 2 个缺口文件**：`ZombieRevive.wav`、`pet_frog.wav`（可从完整 Crystal 客户端补；
   本次三套都没有）；105 个命名文件（BGM/UI）用显式表接。
5. **播放链路**：客户端已有 rodio + 100ms 节流 + 每地图 BGM + 无声卡降级，无需新增依赖；
   只需把「武器类型/地块/性别」选择逻辑接到新映射（当前简化为单文件）。接口建议：
   `mir_render::sound::SoundId` / `sound_for_event` / `spell_sound` / `music_for_map`，
   `mir_client::audio::SoundLibrary`（`resource/wav` → mirfiles → gqf2008 顺序）与
   `SoundPlayer::play/play_event/play_spell`；新增映射放 `mir-render`（纯数据、可单测），
   播放仍只放 `mir-client`。
6. **验证**：`cargo test -p mir-client` 的音频单测 + 本文件的播放冒烟脚本（临时 crate 可删）；
   发版前确认 `package.ps1` 是否携带 `resource/`（当前不打包）。

## 8. 风险

| 风险 | 级别 | 说明 / 缓解 |
|---|---|---|
| ~~授权未确认~~ → **已由所有者裁决解除** | — | 2026-10-20 起 `resource/` 内资源可直接使用，无需逐项核验；仅保留来源记录（§11） |
| 同名不同内容（24 个） | 中 | 同一事件可能播不同版本；固定目录顺序 + 记录选主版本（`target/audio-conflicts.json`）；**选文件必须按目标表指定的文件名/目录，不能随便拿同名文件** |
| Mir1 `sound.lst` 冒充 Crystal 表 | **高（映射陷阱）** | 索引语义不同（10010→`M1-1` vs `10.wav`；10050→`M5-1` vs `50.wav`）；接错会每个音效都放错且难查。必须用 `gqf2008-crystal/Sound/SoundList.lst`，代码注释也要写明（§5.2） |
| 105 个命名文件无自动映射 | 中 | BGM/登录/UI 类（`Field2.wav`、`main_theme.wav`…）；需显式表或按目录规则，别指望公式 |
| 缺 2 个 Crystal 文件（`ZombieRevive`/`pet_frog`） | 低 | 换成同类音或补文件；不影响核心 12 类 |
| 8-bit/32-bit float 少数派（10 个） | 低 | rodio 实测可解；但换解码器/转码工具链时要回归 |
| 体积 721 MB / 2917 文件 | 中 | 全量随包不现实；按事件裁剪（核心 12 类 + 武器变体 < 数十 MB），BGM 单独按地图 |
| BGM 覆盖 | 中 | `mirfiles` 只有 11 个 `300xx.wav`，与文档曾说「30 首」不一致；按地图缺失时要有静音回退（现有 `music_for_map` 已做） |
| 语义漂移（文件名相同但含义不同） | 中 | 不同客户端版本可能重排 M 编号；以 `SoundList.cs` 常量名 + 试听为准，不只看文件名 |

> 风险表保留「法律风险已解除」这一行是为了留下决策记录；**工程风险（映射陷阱 + 24 同名 +
> 105 无映射）没有消失**，第 23 轮实现时必须处理。

## 9. 附录 A：24 个同名不同内容清单（SHA-256 前 12 位）

三套目录 **534 个同名**，其中 **510 个内容完全相同、24 个内容不同**（下表）。`Crystal 社区`
列在两套社区目录内容一致时合并；不一致时已分别列出。完整哈希见 `target/audio-conflicts.json`。

| 文件 | `resource/wav`（大小 / sha256[:12]） | Crystal 社区（大小 / sha256[:12]） | 份数 |
|---|---|---|---|---:|
| `100.wav` | 265842 B `b80222a6ac8a` | 724992 B `81e6bb9fcea2`（gqf2008+mirfiles 相同） | 3 |
| `300-1.wav` | 166736 B `9e7b7a7b2d3f` | 63172 B `827047753591`（gqf2008） | 2 |
| `300-2.wav` | 90724 B `f667108e4abc` | 35268 B `066910031058`（gqf2008） | 2 |
| `900-1.wav` | 239148 B `785ad633ef1d` | 352644 B `4f6fb9041ffd`（gqf2008） | 2 |
| `900-5.wav` | 103582 B `f29fd340ff79` | 429768 B `d672ad6a0639`（gqf2008） | 2 |
| `940-1.wav` | 92560 B `31e44c672ac4` | 47470 B `46ff9daec9d8`（gqf2008） | 2 |
| `940-2.wav` | 58372 B `bc825710e6fe` | 78842 B `24bb6abd954f`（gqf2008） | 2 |
| `M34-1.wav` | 135790 B `b3d1e37744d0` | 319532 B `895c3855f35b`（gqf2008+mirfiles 相同） | 3 |
| `M38-0.wav` | 379984 B `1eb49d868181` | 194112 B `8453281bbfa2`（gqf2008+mirfiles 相同） | 3 |
| `M38-1.wav` | 105222 B `375b48a10ec4` | 417148 B `ed3d987e8952`（gqf2008+mirfiles 相同） | 3 |
| `M40-0.wav` | 167246 B `82fe21b005f5` | 247772 B `05e08c3aa3ad`（gqf2008+mirfiles 相同） | 3 |
| `M41-0.wav` | 305828 B `64b3b54e1550` | 137046 B `994e9aaff6e5`（gqf2008+mirfiles 相同） | 3 |
| `M42-0.wav` | 251052 B `cff0424ae5f1` | 529484 B `24a591936683`（gqf2008+mirfiles 相同） | 3 |
| `M44-0.wav` | 282144 B `e8d4fa183f7d` | 300168 B `68777b6f11b1`（gqf2008+mirfiles 相同） | 3 |
| `M45-0.wav` | 309186 B `e5bf128713af` | 193252 B `cd2eb01afc69`（gqf2008+mirfiles 相同） | 3 |
| `M46-1.wav` | 558484 B `456c03d9e346` | 433728 B `8732dbc4b932`（gqf2008+mirfiles 相同） | 3 |
| `M47-0.wav` | 212468 B `f845df86683e` | 339388 B `3d0ddb8e36b6`（gqf2008+mirfiles 相同） | 3 |
| `M49-0.wav` | 151328 B `2736da89bade` | 309186 B `e5bf128713af`（gqf2008+mirfiles 相同） | 3 |
| `M51-0.wav` | 144032 B `4785b31c7749` | 200320 B `398095d66b5c`（gqf2008） | 2 |
| `M52-0.wav` | 181988 B `c667e0623b17` | 312976 B `88b32d835723`（gqf2008+mirfiles 相同） | 3 |
| `M52-2.wav` | 507848 B `49afc2e340b3` | 458896 B `8c5b3925f716`（gqf2008+mirfiles 相同） | 3 |
| `M53-0.wav` | 476324 B `91f262342218` | 241824 B `ecfa8bcf94fe`（gqf2008+mirfiles 相同） | 3 |
| `M55-0.wav` | 430692 B `17115c89283d` | 189640 B `ff8f3df4c71a`（gqf2008） | 2 |
| `M7-2.wav` | 32938 B `a0321e0aadc9` | 347020 B `38d4d34454a9`（gqf2008+mirfiles 相同） | 3 |

复跑：

```powershell
py target\audio_conflicts.py      # 输出统计与 target/audio-conflicts.json
py target\audio_conflict_table.py # 生成上表（target/audio-conflict-table.json）
```

## 10. 未做 / 未实测

* 未对 2917 个文件逐一试听（只做文件头解析 + 9 个冒烟；语义正确性需人工抽听，本次**未人工听音**）。
* 未测试无音频设备分支（本机有 4 个输出设备）；代码级降级路径存在，未在 CI 容器复现。
* 未做长期维护活跃度评估（只核对锁定版本与许可证）。
* 未把临时冒烟 crate 提交（在 `target/audio-smoke/`，可随时删除）；未修改任何源码。

## 11. 附录 B：授权政策与历史记录

### 11.0 授权政策（项目所有者裁决，2026-10-20）

> **“resource 目录中的资源无视资源授权，全部可以使用。”**

* 生效范围：`resource/` 下的全部素材（音效、贴图/图库、地图、字体等）；
* 判定含义：**授权不再是「可用」的判定项**，不需要逐目录核验；`SOURCES.md`/来源记录仅用于
  溯源，不再是阻塞项；
* 与本文件的关系：§6「可直接接」已按三项重算（2812/2917），风险表只剩工程风险；
* 历史记录保留在 §11.1–§11.3：那是裁决前的实际情况（主目录确实没有来源记录、社区目录
  记得更清楚），对以后做「素材授权状态」章节仍有参考价值。

### 11.1 历史事实（裁决前检索，保留）

* `docs/assets/index.md` §目录约定：`resource/community/` 每个子目录是一个 GitHub 仓库快照，
  **`SOURCES.md` 记录出处**；索引库有 `resource_sources` 表，注释是「知道素材从哪来，
  避免版权/来源不明的文件混进来」。
* 同文 §字体：只对 **Noto Sans SC** 明确写了「SIL OFL 1.1，可自由分发」。
* 全文**没有**给 `resource/data`（图库/客户端数据）、`resource/map`（地图）或 `resource/wav`
  （音效）一个整体授权结论；也没有「素材授权状态」专章。

### 11.2 历史文件/记录（裁决前）

| 目录 | 来源记录 | 授权记录 |
|---|---|---|
| `resource/data/` | **无** `SOURCES.md`/`LICENSE`/README | **无**（文档也没有） |
| `resource/map/` | **无** `SOURCES.md`/`LICENSE`/README | **无** |
| `resource/wav/` | 仅 `sound.lst`，**无**来源/授权文件 | **无** |
| `resource/community/` | 521 个子目录：**519 个有 `SOURCES.md`**，2 个没有（`capitainblack-mir2`、`mirfiles-data-hdassets`） | 有 `SOURCES.md` 的 519 个里 **514 个写了授权状态、5 个没有状态行**（都是 Kenney / rubberduck / clint-bellanger 类素材） |
| `resource/fonts/` | 文档明确 Noto Sans SC | **SIL OFL 1.1（可自由分发）** |

### 11.3 社区目录的历史授权状态分布（裁决前，自 `SOURCES.md` 原文统计）

| 状态 | 目录数 |
|---|---:|
| `未核验（按用户指示：开发阶段不因授权阻断采集，由用户人工核验）` | 412 |
| `未确认` | 43 |
| `未核验` | 25 |
| `未确认（仓库无 LICENSE）` | 20 |
| `未确认（仓库无 LICENSE 时由用户人工核验）` | 8 |
| `未确认（仓库无 LICENSE）。按"本地研究/重制参考"使用…分发前需确认权利` | 2 |
| 明确许可：`AGPL-3.0` / `Apache-2.0` / `MIT` 各 1；`Repository code MIT; legacy game artwork rights unconfirmed` 1 | 4 |
| 有 `SOURCES.md` 但无授权状态行 | 5 |
| 无 `SOURCES.md` | 2 |

**结论（历史）**：贴图/地图/音效不是「音频单独有问题」——裁决前整个项目的游戏素材授权
是同一个待办：来源大多有记录（社区快照 519/521 有 `SOURCES.md`），但授权状态绝大多数是
「未核验/未确认」；只有字体（OFL）以及 3 个明确 MIT/Apache/AGPL 的代码/素材仓库算已确认。
**该待办已由 2026-10-20 所有者裁决解除**，上述统计仅作为历史事实与溯源参考保留。
