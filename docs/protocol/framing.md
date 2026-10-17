# Crystal 协议：帧格式（Wire Framing）

> 参照实现：`github\Crystal`（**只读**，不可修改）。
> 本文件逐字节描述 Crystal（Legend of Mir 2，C#）在 TCP 上收发的帧格式，作为 Rust 重写的实现契约。
> 配套文档：[packet-ids.md](packet-ids.md)（全部编号）、[packets-complex.md](packets-complex.md)（非纯线性的包体）。

## 目录

- [0. 结论速览](#0-结论速览)
- [1. 包头布局](#1-包头布局)
- [2. 长度字段语义](#2-长度字段语义)
- [3. 发送：`Packet.GetPacketBytes()` 精确语义](#3-发送packetgetpacketbytes-精确语义)
- [4. 接收：`Packet.ReceivePacket()` 精确语义](#4-接收packetreceivepacket-精确语义)
- [5. 字节序、对齐与填充](#5-字节序对齐与填充)
- [6. 类型映射表：`BinaryReader`/`BinaryWriter` → 线上字节](#6-类型映射表binaryreaderbinarywriter--线上字节)
- [7. 字符串：7-bit 编码长度前缀（LEB128）+ UTF-8](#7-字符串7-bit-编码长度前缀leb128--utf-8)
- [8. 压缩包（`Compressed => true`）](#8-压缩包compressed--true)
- [9. 加密：没有](#9-加密没有)
- [10. 尺寸上限与边界情况](#10-尺寸上限与边界情况)
- [11. 逐字节示例](#11-逐字节示例)
- [12. Rust 实现建议](#12-rust-实现建议)
- [13. 源码索引](#13-源码索引)

---

## 0. 结论速览

| 项目 | 结论 | 依据 |
| --- | --- | --- |
| 传输层 | 裸 TCP（`TcpClient`），**无 TLS、无加密、无握手、无校验和** | `Server/MirNetwork/MirConnection.cs:87`、`Client/MirNetwork/Network.cs:49` |
| 字节序 | **小端**（`BinaryReader`/`BinaryWriter`/`BitConverter` 在 x86/x64 上的行为） | `Shared/Packet.cs:19-20`、`:75-79` |
| 包头 | 固定 **4 字节**：`uint16 length` + `int16 index` | `Shared/Packet.cs:17`、`:71-79` |
| `length` 含义 | **整帧总长，含 4 字节包头** | `Shared/Packet.cs:79`（写入 `ms.Length`，此时 ms 内已含包头） |
| 对齐/填充 | **没有**。字段严格顺序排列，无 padding、无保留字节、无结束符 | `Shared/Packet.cs:88-89`（`BinaryWriter` 直写） |
| 最大帧 | `length` 是 `uint16` → **65535 字节**（包体最多 65531） | `Shared/Packet.cs:19`、`:79` |
| 包体长度下限 | 0（空负载包 → 整帧 4 字节） | 例：`Shared/ServerPackets.cs:3063-3073`（`TeleportIn`） |
| 编号类型 | `short`（有符号 16 位），值域 0…278 / 0…152 | `Shared/Enums.cs:1434`、`:1717` |
| 字符串 | `Write7BitEncodedInt(UTF-8 字节数)` + UTF-8 字节；空串 = 单字节 `00` | §7 |
| 压缩 | **仅 `NPCGoods` 一个包**，gzip（RFC 1952） | `Shared/ServerPackets.cs:3077`、`Shared/Functions/Functions.cs:486-509` |
| 一次发送 | 多个包**直接拼接**后一次 `BeginSend`，无分隔符 | `Server/MirNetwork/MirConnection.cs:275-284` |

一帧的结构（`N = length - 4` 为包体字节数）：

```
偏移 0            偏移 2          偏移 4
+----------------+--------------+-------------------------------------+
| length = N + 4 | index        | payload（N 字节）                    |
| u16  小端      | i16  小端    | 由具体包 ReadPacket/WritePacket 决定 |
+----------------+--------------+-------------------------------------+
   这 4 字节计入 length ──┘
```

---

## 1. 包头布局

| 偏移 | 大小 | 类型 | 名称 | 说明 |
| --- | --- | --- | --- | --- |
| 0 | 2 | `uint16`（小端） | `length` | **整帧**字节数，含这 4 字节包头；解析用 `BitConverter.ToUInt16(rawBytes, 0)`（`Shared/Packet.cs:19`） |
| 2 | 2 | `int16`（小端） | `index` | 包编号；解析用 `BitConverter.ToInt16(rawBytes, 2)`（`Shared/Packet.cs:20`） |
| 4 | `length - 4` | — | `payload` | 包体，由具体包的 `ReadPacket`/`WritePacket` 决定 |

源码原文（写入端，`Shared/Packet.cs:69-83`）：

```csharp
using (var ms = new MemoryStream())
{
    ms.SetLength(2);
    ms.Seek(2, SeekOrigin.Begin);
    using (var writer = new BinaryWriter(ms))
    {
        writer.Write(Index);        // int16 → 2 字节小端，落在偏移 2
        writer.Write(mem);          // byte[] → 原样字节，无长度前缀，落在偏移 4
        ms.Seek(0, SeekOrigin.Begin);
        writer.Write((ushort)ms.Length);  // 回填偏移 0 的长度
        data = ms.ToArray();
    }
}
```

要点：

- `ms.SetLength(2)` 只是**预留** 2 字节长度位，随后 `Seek(2)` 把游标移到偏移 2，先写编号再写包体，最后回到 0 回填长度——所以长度天然包含包头。
- `writer.Write(Index)` 用的是 `BinaryWriter.Write(short)` 重载（`Index` 属性类型是 `short`，`Shared/Packet.cs:9`），因此编号是 **2 字节有符号小端**，不是变长整数。
- 包头里**没有**压缩位、没有加密位、没有 flags、没有序列号、没有 CRC。接收方只能靠 `index` 查 `Compressed` 属性来得知该包是否压缩（见 §8）。
- `Packet` 上另外两个"属性"**不参与线上编码**：`Observable`（`Shared/Packet.cs:7`，只控制服务端是否把包转发给观察者，见 `MirConnection.cs:231-240`）和 `Compressed`（只影响包体的编码方式）。

---

## 2. 长度字段语义

1. `length` = `4 + payload.Length`，**包含** 2 字节长度字段本身和 2 字节编号。
2. 接收端按 `length` 从缓冲区切出整帧，剩余部分作为下一帧继续处理（`Shared/Packet.cs:47-48`）。
3. `length` 是 `ushort`，因此单帧上限 **65535**，包体上限 **65531**。
4. 写入端**不做溢出检查**：`writer.Write((ushort)ms.Length)`（`Shared/Packet.cs:79`）把 `long` 强转 `ushort`，超过 65535 会静默截断。例：包体 70000 字节 → `ms.Length = 70004` → 写入 `(ushort)70004 = 4468`，接收方只取 4468 字节，剩下 6 万多字节被当成后续帧 → 整体错位。**Rust 必须在编码时显式拒绝 > 65531 的包体**（或按协议层拆包），不要复制这个截断行为。
5. TCP 是字节流，一次 `BeginReceive` 可能拿到半个包，也可能拿到好几个包；收端用**可增长缓冲区**拼接（`Server/MirNetwork/MirConnection.cs:161-171`、`Client/MirNetwork/Network.cs:128-142`），每次 socket 读取的缓冲区是 8 KB（`MirConnection.cs:53`、`Network.cs:22`），这只是读取块大小，与帧上限无关。

---

## 3. 发送：`Packet.GetPacketBytes()` 精确语义

源码：`Shared/Packet.cs:53-86`。执行顺序严格如下：

1. `if (Index < 0) return new byte[0];`（`:55`）——编号非法的包**静默丢弃**（线上不会出现负数编号，此分支实际是死代码，但语义上"返回空数组"要保留：拼接时等于不存在）。
2. 新建 `MemoryStream`，用 `BinaryWriter` 调 `WritePacket(writer)` 写出**包体**（`:59-64`）。包体里**不含**长度和编号。
3. `mem = Compressed ? CompressPacket(ms.ToArray()) : ms.ToArray();`（`:66`）——若该包 `Compressed => true`，则对**整个包体**做 gzip 压缩，后续写入的是压缩后的字节。
4. 新建第二个 `MemoryStream`：先 `SetLength(2)` + `Seek(2)` 预留长度位（`:71-72`），写 `Index`（2 字节）与 `mem`（原样字节，`BinaryWriter.Write(byte[])` 不加长度前缀），再 `Seek(0)` 回填 `(ushort)ms.Length`（`:75-79`）。
5. `return ms.ToArray();`（`:81`）——返回**完整一帧** `[len][index][payload]`。虽然返回类型是 `IEnumerable<byte>`，但内容是已完全物化的 `byte[]`，没有惰性求值。

补充：

- 发送方**不**保留任何状态（无序列号、无重传、无压缩上下文），同一个包对象重复调用 `GetPacketBytes()` 结果相同（gzip 输出对同一 .NET 运行时版本是确定的，但**不同版本/不同压缩实现之间不保证字节相同**，见 §8）。
- 网络层把队列里所有包的结果**直接相加**成一个大 `byte[]` 再 `BeginSend`（`Server/MirNetwork/MirConnection.cs:275-284`）；客户端同理（`Client/MirNetwork/Network.cs:237-246`）。所以"一次 send = 一帧"是不成立的，接收端必须按长度切分。
- 服务端连接建立后**第一件事**是入队 `new S.Connected()`（`Server/MirNetwork/MirConnection.cs:97`），即客户端连上后收到的第一帧是 `index = 0`、`length = 4`。

---

## 4. 接收：`Packet.ReceivePacket()` 精确语义

签名（`Shared/Packet.cs:11`）：`public static Packet ReceivePacket(byte[] rawBytes, out byte[] extra)`

逐行语义（`Shared/Packet.cs:11-51`）：

| 步骤 | 源码 | 行为 |
| --- | --- | --- |
| 1 | `extra = rawBytes;`（`:13`） | **先把整个缓冲当作"剩余"**。注意 `extra` 是 `out` 参数，这一行立即写回调用方变量，所以后面即使抛异常，调用方缓冲区也保持不变 |
| 2 | `if (rawBytes.Length < 4) return null;`（`:17`） | 不足一个包头 → 返回 `null`，`extra` 仍是原缓冲（等待更多数据） |
| 3 | `length = BitConverter.ToUInt16(rawBytes, 0)`（`:19`） | 读 `uint16` 小端 |
| 4 | `id = BitConverter.ToInt16(rawBytes, 2)`（`:20`） | 读 `int16` 小端 |
| 5 | `if (length > rawBytes.Length \|\| length < 2) return null;`（`:22`） | 帧还没到齐（`length` 大于现有字节）→ 返回 `null` 等数据；`length < 2` 也返回 `null` 且**不消费任何字节**（见下文"卡死"） |
| 6 | `p = IsServer ? GetClientPacket(id) : GetServerPacket(id)`（`:26`） | **方向由 `Packet.IsServer` 决定**：服务端置 `true`，于是去查**客户端包表**；客户端置 `false`，查**服务端包表**（`Server.MirForms/Program.cs:14`、`Client/Program.cs:38`） |
| 7 | `if (p == null) { extra = new byte[0]; return null; }`（`:27-33`） | **未知编号 → 丢弃整个接收缓冲区**，返回 `null`（不抛异常、不断连）。注释（`:29-30`）说明这是为了防止服务端在脏数据上死循环 |
| 8 | 构造 `MemoryStream`（`:35-38`） | 非压缩：`new MemoryStream(rawBytes, 4, length - 4)`（偏移 4、长度 `length-4`，**不复制**）；压缩：先 `DecompressPacket(rawBytes[4..(length - 4)])` 再包成流（见 §8，**这里少切 4 字节**） |
| 9 | `p.ReadPacket(reader)`（`:40`） | 从包体流里读字段。**不检查是否读完**——包体尾部多余字节被静默忽略 |
| 10 | `catch (Exception e) { throw new InvalidDataException(); }`（`:42-45`） | 任何解码异常（`EndOfStreamException`、`ArgumentOutOfRangeException`、`OverflowException`…）都被**改写**成 `InvalidDataException`，原始异常丢失 |
| 11 | `extra = new byte[rawBytes.Length - length]; Buffer.BlockCopy(...)`（`:47-48`） | 成功时把 `[length, end)` 之间的字节**复制**成新数组作为剩余缓冲 |

### 4.1 行为矩阵

| # | 情况 | 返回值 | `extra` | 调用方（C#）后果 |
| --- | --- | --- | --- | --- |
| 1 | `rawBytes.Length < 4` | `null` | 原缓冲 | 等下一次 socket 读取再拼 |
| 2 | `length > rawBytes.Length`（帧未到齐） | `null` | 原缓冲 | 等数据 |
| 3 | `length < 2` | `null` | 原缓冲 | **永不前进**：数据会一直堆在缓冲区；服务端靠"5 秒内 > `MaxPacket`(50) 次 socket 读取就断连"兜底（`MirConnection.cs:151-202`、`Server/Settings.cs:71`） |
| 4 | `length` ∈ {2, 3}，编号未知 | `null` | **空数组**（丢弃全部） | 缓冲被清空，静默吃掉数据 |
| 5 | `length` ∈ {2, 3}，编号已知 | 抛 `InvalidDataException` | 原缓冲 | 服务端：断连 + 封 IP 24h（`MirConnection.cs:173-181`）；客户端：异常从 socket 回调抛出（`Network.cs:138` 无 try/catch） |
| 6 | 编号未知（`length ≥ 4`） | `null` | **空数组**（丢弃全部） | 缓冲被清空；**不报错、不断连** |
| 7 | 解码成功 | 包对象 | `rawBytes[length..]` | 循环继续解析下一帧 |
| 8 | `ReadPacket` 内部异常 | 抛 `InvalidDataException` | 原缓冲（未生效） | 同 #5 |

### 4.2 关键推论（Rust 必须做出明确选择）

- **`length` 的理论下限是 4**（2 字节长度 + 2 字节编号），但 C# 只检查 `length < 2`，因此 2、3 属于"畸形长度"，必然走进 #4 或 #5。Rust 侧应直接把 `length < 4` 判为协议错误。
- **未知编号的默认策略是"丢光当前缓冲区"而不是断连**。这是有意的容错设计（`Shared/Packet.cs:29-30` 的注释），但代价是流被静默破坏。Rust 建议：**记录日志并断开连接**，或严格照抄"清空缓冲区"；两者都能与现有客户端/服务端互通（因为正常客户端不会发未知编号）。
- **包体不要求读满**：多出来的字节被忽略，因此"包体比预期长"是兼容的，"比预期短"才会抛异常。
- **接收方不对包体做长度一致性校验**：即使 `length` 比 `ReadPacket` 实际消费的多，也不会报错。（`BinaryReader` 只在**读不到**所需字节时抛 `EndOfStreamException`。）
- **`extra` 在 C# 中是"剩余字节"**：服务端把它写回 `_rawData`（`MirConnection.cs:170`），客户端同样（`Network.cs:138`），所以 `while ((p = ReceivePacket(_rawData, out _rawData)) != null)` 会连续解析缓冲区里的所有完整帧。
- **`IsServer` 的命名容易误导**：`Server.MirForms/Program.cs:14` 设 `Packet.IsServer = true`，之后 `ReceivePacket` 用 `GetClientPacket(id)` 解析——即"服务端解析客户端包"。Rust 侧应直接用两个独立的解析函数（`parse_client_packet` / `parse_server_packet`），不要照搬这个布尔量。

---

## 5. 字节序、对齐与填充

| 问题 | 答案 | 依据 |
| --- | --- | --- |
| 字节序 | **小端**（little-endian） | `BinaryReader`/`BinaryWriter` 全系列整型都是小端；`BitConverter.ToUInt16/ToInt16` 在 Windows x86/x64 上是小端。协议里**没有**任何大端字段 |
| 结构体对齐 | **无**。不存在 `[StructLayout]` 序列化，也没有按 2/4/8 对齐补空字节 | 所有包都是逐个 `reader.ReadXxx()` 顺序读 |
| 填充 / 保留字节 | **无** | 包体紧接包头，末尾没有任何填充 |
| 布尔 | 1 字节，`false` 写 `0x00`、`true` 写 `0x01`；**读**时只要非 0 就是 `true`（不校验） | §6 |
| 字符串 | 变长，前置 7-bit 长度；**无** `\0` 结束符、**无** BOM、**无**定长补零 | §7 |
| 枚举 | 按底层类型写（绝大多数是 `byte`，`ServerPacketIds`/`ClientPacketIds` 是 `short`）。C# 侧常写成 `writer.Write((byte)Type)`，因此上位机必须按 1 字节读 | 例 `Shared/ServerPackets.cs:3102` |
| `DateTime` | 8 字节 `i64` = `DateTime.ToBinary()`（低 62 位 ticks，高 2 位 Kind） | 例 `Shared/ServerPackets.cs:273`（读）/ `:279`（写） |
| `Color` | 4 字节 `i32` ARGB 位模式（`Color.FromArgb(int)` / `.ToArgb()`） | 例 `Shared/ServerPackets.cs:639` |
| `Point` | 8 字节 = 两个 `i32`（X 先，Y 后） | 例 `Shared/ServerPackets.cs:643` |

> 小端、无对齐、无填充意味着 Rust 侧可以直接用一个"顺序读取器"（`read_u16/read_i32/read_f32/...`）实现，不需要 `#[repr(C)]` 的 `bytemuck` 技巧；也不要用 `serde` 的二进制格式（会自带变长整数与前缀语义）。

---

## 6. 类型映射表：`BinaryReader`/`BinaryWriter` → 线上字节

下表中"使用次数"是 `Shared/ServerPackets.cs` 与 `Shared/ClientPackets.cs` 中 `reader.ReadXxx(` 的出现次数（去注释后统计），可据此判断哪些类型真正在协议里出现过。

| C# 读取调用 | 线上字节 | 类型 / 有符号性 | 使用次数 S / C | 备注 |
| --- | --- | --- | --- | --- |
| `ReadBoolean()` | 1 | `bool`，写端 `0x00`/`0x01` | 119 / 29 | 读端 `value != 0` 即 `true`；**任何非 0 字节都算 true** |
| `ReadByte()` | 1 | `byte`（无符号 0…255） | 147 / 50 | 最常用于枚举与标志位 |
| `ReadSByte()` | 1 | `sbyte`（有符号 −128…127） | 2 / 1 | 只出现在少数地方：`SpellToggle.canUse`（`Shared/ClientPackets.cs:1505`）、`MailSent` / `ParcelCollected` 的 `Result`（`Shared/ServerPackets.cs:5941`、`:5977`）、物品属性 `Stats[Luck]` / `Stats[AttackSpeed]`（`Shared/Data/ItemData.cs:117-118`） |
| `ReadInt16()` | 2 | `short` 小端（−32768…32767） | 11 / 2 | 例 `Weapon`、`Armour`、`MountType`、`MarriedDays` |
| `ReadUInt16()` | 2 | `ushort` 小端 | 36 / 7 | 例 `Level`、`Image`、`Music`；`Poison`/`LevelEffects`/`Weather` 是 **位标志枚举**，也走 u16 |
| `ReadInt32()` | 4 | `int` 小端 | 211 / 65 | `count` 前缀、坐标 X/Y、HP/MP、时间戳等 |
| `ReadUInt32()` | 4 | `uint` 小端 | 102 / 20 | `ObjectID`、`TargetID`、`Gold`、`Credit` |
| `ReadInt64()` | 8 | `long` 小端 | 28 / 2 | `Experience`、`ShockTime`、`DateTime.ToBinary()` |
| `ReadUInt64()` | 8 | `ulong` 小端 | 24 / 41 | `UniqueID`、`ChatItemId`、邮件 5×u64 |
| `ReadSingle()` | 4 | `float` 小端 IEEE-754 binary32 | 6 / 0 | 只出现在服务端包（`Rate` 等） |
| `ReadString()` | 1+N 起 | 7-bit 长度前缀 + UTF-8 | 57 / 37 | 见 §7 |
| `ReadBytes(n)` | n | 原始字节 | 0 / 2 | **只在客户端包里出现**（`ClientVersion.VersionHash`、`ReportIssue.Image`） |

从未使用的读取方法（整个 `Shared` 目录 0 次）：`ReadDouble()`、`ReadDecimal()`、`ReadChar()`、`ReadChars()`、`Read7BitEncodedInt()`（由 `ReadString` 内部使用）。因此协议里**没有** 8 字节浮点与定长宽字符。

### 6.1 写入端对应关系

| C# 写入调用 | 线上字节 | 说明 |
| --- | --- | --- |
| `Write(bool)` | 1 | `0x01` / `0x00` |
| `Write(byte)` | 1 | |
| `Write(sbyte)` | 1 | |
| `Write(short)` / `Write(ushort)` | 2 | 小端 |
| `Write(int)` / `Write(uint)` | 4 | 小端 |
| `Write(long)` / `Write(ulong)` | 8 | 小端 |
| `Write(float)` | 4 | IEEE-754 小端 |
| `Write(string)` | 1+N 起 | LEB128 长度 + UTF-8；**`null` 会抛 `ArgumentNullException`**（协议里所有字符串字段都初始化为 `string.Empty`，见 `Shared/ClientPackets.cs:177-178`） |
| `Write(byte[])` | n | 原样字节，**无**长度前缀（`Shared/Packet.cs:76` 就用它写整段包体/压缩体） |
| `Write((byte)enumValue)` | 1 | 显式强转枚举 → 1 字节；`(ushort)enumValue` → 2 字节；`(sbyte)enumValue` → 1 字节 |

> 统计结果：写入端出现的显式强转主要是 `(byte)`（130 次）、`(ushort)`（9 次）、`(sbyte)`（1 次，另有 1 处写作 `(Byte)`）。**这些强转决定了字段宽度**，Rust 侧务必逐包核对（例如 `writer.Write((byte)Type)` 是 1 字节，而字段本身可能是 `int`）。

### 6.2 复合值类型

| 类型 | 线上字节 | 编码 |
| --- | --- | --- |
| `Point` | 8 | `X: i32`、`Y: i32`（小端）。源码里通常写作 `new Point(reader.ReadInt32(), reader.ReadInt32())`，**求值顺序**是 X 先于 Y |
| `Color` | 4 | `i32` 的 ARGB 位模式（`0xAARRGGBB`，即 `A<<24` 按位或 `R<<16` 按位或 `G<<8` 按位或 `B`）。读端 `Color.FromArgb(int)` |
| `DateTime` | 8 | `i64 DateTime.ToBinary()`：低 62 位是 ticks（自 0001-01-01 起的 100ns 数），高 2 位是 Kind（0=Unspecified、1=Utc、2=Local）。**不要**用 Unix 秒 |
| `TimeSpan` / `Guid` / `decimal` | — | 协议里**没有**直接出现（`Guid` 只在存档/数据库层） |

---

## 7. 字符串：7-bit 编码长度前缀（LEB128）+ UTF-8

`BinaryWriter.Write(string)` 的线上格式（Crystal 里所有字符串字段都用它，共 57 + 37 处）：

```
[string] = [7-bit encoded int: UTF-8 字节数][UTF-8 字节]     ← 没有 \0 结尾
```

### 7.1 前缀编码（.NET `Write7BitEncodedInt`）

`int` 按 **7 位一组、小端序**输出，每字节最高位（bit7）表示"后面还有字节"：

```
do { b = (byte)(value | 0x80); write(b); value >>= 7; } while (value != 0);
最后再 write((byte)value);
```

读取端 `.NET BinaryReader.ReadString()` 先 `Read7BitEncodedInt()`（最多 5 字节，超过则抛 `FormatException`），再按得到的字节数读 UTF-8。

| UTF-8 字节数 V | 前缀字节数 | 前缀（十六进制） | 边界说明 |
| --- | --- | --- | --- |
| `V == 0` | 1 | `00` | **空串就是一个 `0x00` 字节**（不是 0 字节、也不是 2 字节） |
| `1 ≤ V ≤ 127` | 1 | `01`…`7F` | |
| `128 ≤ V ≤ 16383` | 2 | `80 01` … `FF 7F` | **≥128 字节必须用 2 字节前缀**：`V=128` → `80 01`；`V=300` → `AC 02` |
| `16384 ≤ V ≤ 2097151` | 3 | `80 80 01` … | |
| `2097152 ≤ V ≤ 268435455` | 4 | | |
| `268435456 ≤ V ≤ int.MaxValue` | 5 | | |

关键点（Rust 实现常错的地方）：

1. **前缀计的是 UTF-8 字节数，不是字符数**。`"传奇"` 是 2 个字符但 6 个字节 → 前缀 `06`，随后是 `E4 BC A0 E5 A5 87`。
2. **空串 = 1 字节 `00`**；写端只要收到 `null` 会抛 `ArgumentNullException`，读端读到 `00` 就得到 `""`（`string.Empty`），二者不可混用。
3. **≥ 128 字节时前缀变 2 字节**，这也意味着中文/长公告（NPC 对话、公告、聊天）几乎总会落在 2 字节前缀区间；任何"假定 1 字节长度前缀"的实现都会在长文本上挂掉。
4. **没有 4 字节对齐、没有 BOM、没有 `\0` 结束符**，长度前缀是唯一的分界依据。
5. 解码非法 UTF-8 时 .NET 默认使用替换回退，得到 `U+FFFD` 而不是抛异常；Rust 可用 `String::from_utf8_lossy` 对齐（若想严格，则 `from_utf8` + 报错，但会与参考实现的行为不一致）。
6. 字节数不够（流里剩下的字节少于前缀声明的长度）时，`ReadString` 抛 `EndOfStreamException` → 被 `ReceivePacket` 改写成 `InvalidDataException`（§4 步骤 10）。

### 7.2 `ReadBytes(n)` 的细节

`ClientVersion.VersionHash = reader.ReadBytes(reader.ReadInt32())`（`Shared/ClientPackets.cs:16`）、`ReportIssue.Image = reader.ReadBytes(reader.ReadInt32())`（`Shared/ClientPackets.cs:2490`）：

- 线上是 **`i32` 长度 + 该长度的原始字节**（长度不含自身 4 字节）。
- `BinaryReader.ReadBytes(n)` 在**剩余字节不足时不会抛异常**，而是返回一个更短的数组——这是它与 `ReadString` 的重要差别。
- `n` 为负数 → `ArgumentOutOfRangeException`；`n` 极大 → `new byte[n]` 触发 `OutOfMemoryException`。两者都会被 `ReceivePacket` 转成 `InvalidDataException`（服务端因此会断连）。**Rust 侧必须先校验 `n ≤ 剩余字节数`**，不要照抄"先分配后读"。

---

## 8. 压缩包（`Compressed => true`）

### 8.1 哪些包被压缩

全协议只有**一个**：

| 方向 | 包 | Index | 源码 |
| --- | --- | --- | --- |
| Server → Client | `NPCGoods` | 102 (`0x0066`) | `Shared/ServerPackets.cs:3074-3106`，`public override bool Compressed => true;` 在 `:3077` |

- 基类默认 `public virtual bool Compressed => false;`（`Shared/Packet.cs:8`）。
- `ClientPackets.cs` 里没有任何 `Compressed` 覆写；也没有任何包覆写 `CompressPacket`/`DecompressPacket`（`Shared/Packet.cs:91-92` 是唯一的实现，全仓库无其它 override）。
- **压缩不是包头里的标志位**，而是按 `index` 查表得到的编译期属性；因此 Rust 侧应维护一张 `fn is_compressed(index: i16) -> bool`，目前只有 `102` 返回 `true`。

### 8.2 压缩实现

`Shared/Functions/Functions.cs:486-509`：

```csharp
public static byte[] CompressBytes(byte[] raw)
{
    using (var memory = new MemoryStream())
    {
        using (var gzip = new GZipStream(memory, CompressionMode.Compress, true))
            gzip.Write(raw, 0, raw.Length);
        return memory.ToArray();
    }
}

public static byte[] DecompressBytes(byte[] gzip)
{
    using (var stream = new GZipStream(new MemoryStream(gzip), CompressionMode.Decompress))
    using (var memory = new MemoryStream())
    {
        stream.CopyTo(memory, 4096);
        return memory.ToArray();
    }
}
```

- 流类型：**`System.IO.Compression.GZipStream`**，压缩模式 `CompressionMode.Compress`（未指定 `CompressionLevel` → 默认 `Optimal`）。
- 格式：**标准 gzip 成员（RFC 1952）**——10 字节固定头（`1F 8B 08 00` + MTIME + XFL + OS）+ deflate 数据 + 8 字节尾（`CRC32` + `ISIZE`）。
- 解压用同名 `GZipStream` 的 `Decompress` 模式 + `CopyTo`，读到流结束。
- gzip 头里的 MTIME 由 .NET 固定写 0，因此**同一个 .NET 版本**对同一输入输出稳定；但**跨运行时/跨实现（含 Rust 的 `flate2`）压缩结果不保证逐字节相同**。所以"字节兼容"的正确含义是：**gzip 格式可互通、解压后包体逐字节相同**，而不是压缩流逐字节相同。

### 8.3 关键陷阱：读端少用了 4 字节

压缩包的读取路径（`Shared/Packet.cs:35-37`）：

```csharp
using var ms = p.Compressed ?
    new MemoryStream(p.DecompressPacket(rawBytes[4..(length - 4)])) :
    new MemoryStream(rawBytes, 4, length - 4);
```

C# 的 `x[a..b]` 是**左闭右开**，`rawBytes[4..(length - 4)]` 的元素个数是 `(length-4) - 4 = length - 8`；而未压缩分支的 `new MemoryStream(rawBytes, 4, length - 4)` 取的是 `length - 4` 字节。即：

> **压缩包在解压前，压缩负载的最后 4 字节（gzip 尾部的 `ISIZE`）被丢弃了。**

- 这不是笔误的猜测，而是可推导的事实：`length` 是整帧长度，`[4, length)` 才是完整负载；写成 `[4, length-4)` 必然少 4 字节。
- 之所以线上还能正常工作：gzip 的 deflate 数据块此时已经完整，缺的 4 字节属于尾部校验字段，`.NET` 的 `GZipStream` 不会因此判定失败（Crystal 现网就是这样跑的）。
- **对 Rust 的影响（两种都可行，建议选 B）**：
  - A（逐字节对齐参考实现）：对压缩负载做 `payload[..payload.len()-4]` 后再交给 gzip 解码器，并要求解码器**不校验**尾部 CRC/ISIZE（`flate2::Decompress` 裸 deflate 模式最稳）。
  - B（推荐，兼容性更好）：先尝试完整 `payload` 的 gzip 解码，失败再回退到 `payload[..len-4]`。这样既能吃下 Crystal 服务端/客户端的现有帧，也能吃下其它正确实现发来的帧。
- **发送方向**：Crystal 服务端写出的是**完整** gzip 成员（含 8 字节尾），由 Crystal 客户端在读端砍掉 4 字节。因此 Rust 服务端也应输出**完整标准 gzip 成员**（`flate2::write::GzEncoder`，Cargo 工作区已依赖 `flate2 = "1"`）。

### 8.4 其它细节

- `length` 字段记录的是**压缩后**的大小 + 4：`mem = CompressPacket(ms.ToArray())` 之后才计算 `ms.Length`（`Shared/Packet.cs:66-79`）。
- 解压后的包体交给同一个 `BinaryReader` 读取，`NPCGoods` 的包体布局见 [packets-complex.md](packets-complex.md)。
- 解压流里若还有多余字节，同样被忽略（不校验读满）。
- gzip 多成员（concatenated members）在 .NET 与 `flate2` 上都会继续拼接解码，协议本身不依赖这一特性。

---

## 9. 加密：没有

**结论：Crystal 在 socket 上不做任何加密、不做任何混淆（obfuscation）、不做 TLS。协议就是明文裸 TCP。**

证据（逐条可核对）：

| # | 证据 | 位置 |
| --- | --- | --- |
| 1 | 服务端把 socket 收到的原始字节**直接**交给 `Packet.ReceivePacket`，中间没有任何变换 | `Server/MirNetwork/MirConnection.cs:137`（`EndReceive`）→ `:161-166`（拼接缓冲区）→ `:170-171`（`while ((p = Packet.ReceivePacket(_rawData, out _rawData)) != null)`） |
| 2 | 服务端发送时，把各包的 `GetPacketBytes()` **原样相加**后 `BeginSend` | `Server/MirNetwork/MirConnection.cs:275-284`（`data.AddRange(p.GetPacketBytes());` → `BeginSend(data)`） |
| 3 | 客户端两个方向同样没有变换 | `Client/MirNetwork/Network.cs:128-142`（收）、`:237-246`（发） |
| 4 | 传输层是 `new TcpClient()` + `BeginConnect/BeginReceive/BeginSend`；全仓库搜索 `SslStream`、`AuthenticateAs*`、`NetworkStream`、`RSACrypto*`、`HMAC` **0 命中** | `Server/MirNetwork/MirConnection.cs:87-121`、`Client/MirNetwork/Network.cs:49-50`、`:100`、`:155` |
| 5 | 全仓库搜索 `Encrypt\|Decrypt\|CryptoStream\|Aes\|Rijndael\|Blowfish\|RC4\|TEA\|Xor` 只命中 `AutoPatcherAdminV2/Settings.cs`（补丁器把**密码存盘**时的加密），`Shared` / `Server` / `Client` 的网络代码 **0 命中**；唯一的 MD5 用途是**客户端二进制文件的版本校验哈希**（`Client/MirScenes/LoginScene.cs:141-145`、`Server/Settings.cs:366-367`），与传输加密无关 | 全仓库 grep |
| 6 | 最强证据：账号密码就是**明文字符串**直接写在包里 | `Shared/ClientPackets.cs:170-190`（`Login`：`AccountID = reader.ReadString(); Password = reader.ReadString();`，`:183` 写端 `writer.Write(Password)` 于 `:188`）；`Shared/ClientPackets.cs:68-87`（`NewAccount`，`:71` / `:81`） |
| 7 | 包头里没有 IV / 密钥交换 / 序号字段，`length`+`index` 就是全部控制信息 | `Shared/Packet.cs:17-20` |

对 Rust 重写的含义：

- 直接 `TcpListener::accept` → `AsyncRead/AsyncWrite`，**不要**加 TLS 或自定义混淆，否则无法与现有 Crystal 客户端互通。
- 登录态是"客户端发明文账号密码，服务端自己校验"，密码安全只能靠部署层（VPN/反向代理）解决；Rust 版若想加 TLS，必须同时改客户端，这属于协议破坏性变更。
- 因为无加密，协议可以被随意抓包/重放：Rust 服务端应考虑限速、`MaxPacket` 类似的反滥用措施（见 §10）。

---

## 10. 尺寸上限与边界情况

| 项目 | 值 | 说明 |
| --- | --- | --- |
| 单帧最大 | 65535 字节 | `length` 是 `uint16` |
| 包体最大 | 65531 字节 | 65535 − 4（包头） |
| 写入溢出行为 | **静默截断**（`(ushort)ms.Length`） | `Shared/Packet.cs:79`；Rust 必须改为报错 |
| 单帧最小 | 4 字节 | 空负载包；C# 只校验 `length >= 2`，2/3 属畸形 |
| socket 读取块 | 8 KB | `MirConnection.cs:53`、`Network.cs:22`（与帧上限无关） |
| 接收频率限制 | 默认 50 次**socket 接收回调** / 5 秒 | `Server/Settings.cs:71`（`MaxPacket = 50`），判定在 `MirConnection.cs:151-202`；超限 → 断连 + 封 IP 24 小时。注意统计的是**接收回调次数**，不是包数 |
| 空闲超时 | 默认 10000 ms | `Server/Settings.cs:67`（`TimeOut`）；客户端会主动发 `KeepAlive`（`Client/MirNetwork/Network.cs:230-231`） |
| `ReadString` 长度上限 | `int.MaxValue` 字节（5 字节前缀） | 实际上受帧长约束 |
| `ReadBytes(n)` 的长度上限 | 无校验 → 可 OOM | `Shared/ClientPackets.cs:16`、`:2490`；Rust 必须先做上界检查 |
| 递归/嵌套深度 | 有界（helper 之间不互相递归） | 例如 `UserItem` 内不再含 `UserItem` |

---

## 11. 逐字节示例

### 11.1 空负载包：`S.Connected`（index 0）

`Shared/ServerPackets.cs:24-37` 的 `Connected` 包体为空。

```
04 00 00 00
│     └─┴─ index = 0x0000 = Connected
└──┬──┘    length = 0x0004 = 4（只有包头）
```

客户端连上后收到的第一帧就是这个（`Server/MirNetwork/MirConnection.cs:97`）。

### 11.2 单字节体：`S.Disconnect`（index 2，`Reason = 3` = "服务器崩溃"）

`Shared/ServerPackets.cs:62-87`：`Reason = reader.ReadByte();`

```
05 00 02 00 03
│           └─ Reason = 3
│     └─┴─ index = 2
└──┬──┘    length = 5
```

### 11.3 定长体：`S.ObjectRemove`（index 26，`ObjectID = 0x01020304`）

`Shared/ServerPackets.cs:1048-1067`：`ObjectID = reader.ReadUInt32();`

```
08 00 1A 00 04 03 02 01
│           └───────── ObjectID 小端 = 0x01020304
│     └─┴─ index = 26
└──┬──┘    length = 8
```

### 11.4 字符串体：`C.Login`（index 5，账号 `admin`、密码 `pass`）

`Shared/ClientPackets.cs:170-190`。包体 = `05 61 64 6D 69 6E` + `04 70 61 73 73` = 6 + 5 = 11 字节。

```
0F 00 05 00 05 61 64 6D 69 6E 04 70 61 73 73
│           │  └──────────────┘ │  └──────────────┘
│           │   "admin"（5B）    │   "pass"（4B）
│           └ AccountID 前缀    └ Password 前缀
└──┬──┘      length = 0x000F = 15 = 4 + 11
   index = 5
```

### 11.5 ≥128 字节字符串（前缀变 2 字节）

128 个 `'a'` 的字符串编码为：`80 01` + 128 × `61`，共 130 字节。若作为某个包的唯一字段，则 `length = 4 + 130 = 134`（`86 00`）。

---

## 12. Rust 实现建议

按上面的语义，`mir-protocol` 至少需要这些能力：

```rust
// 1) 帧编码：严格对齐 GetPacketBytes()
fn encode_frame(index: i16, payload: &[u8], compressed: bool) -> Result<Vec<u8>, FrameError> {
    let body = if compressed { gzip(payload)? } else { payload.to_vec() };
    let total = 4 + body.len();
    if total > u16::MAX as usize { return Err(FrameError::TooLarge(total)); } // 不要照抄 (ushort) 截断
    let mut out = Vec::with_capacity(total);
    out.extend_from_slice(&(total as u16).to_le_bytes()); // 长度含包头
    out.extend_from_slice(&index.to_le_bytes());          // i16 小端
    out.extend_from_slice(&body);
    Ok(out)
}

// 2) 帧切分：必须能吃下"一次 read 里半帧/多帧"
//    - length < 4                → 协议错误（C# 只检查 length>=2，2/3 会丢缓冲或抛异常）
//    - length > 缓冲区剩余       → 继续等数据
//    - 未知 index                → 错误（C# 会清空整个缓冲区）
//    - 解出包体后允许尾部多余字节 → 与 C# 一致，不要报错

// 3) 包体读取器
//    read_u8 / read_i8 / read_u16 / read_i16 / read_u32 / read_i32 /
//    read_u64 / read_i64 / read_f32 / read_bool(!=0) /
//    read_leb128_len + read_utf8 (lossy 或严格二选一) / read_bytes(n)（先校验 n ≤ remaining）

// 4) 压缩：只有 index == 102 (NPCGoods) 走 gzip；
//    解码建议"完整 gzip 优先，失败再吃掉尾部 4 字节"的兼容策略（见 §8.3）
```

实现清单：

- [ ] 包头严格 4 字节、小端、`length` 含包头。
- [ ] 帧长 > 65531 时报错，不截断。
- [ ] `length < 4` 判协议错误（不要像 C# 那样"留着不动"，否则缓冲区会无限增长）。
- [ ] 未知 `index` 的处理策略二选一（**清空缓冲**以对齐参考实现，或**断连**并记录日志），但必须显式决定，不要"丢弃这一帧继续"——那会与任何真实对端错位。
- [ ] 字符串用 LEB128 长度 + UTF-8，空串写 `0x00`，≥128 字节用 2 字节前缀。
- [ ] `bool` 读端宽松（非 0 即真）。
- [ ] `Point` = 8 字节、`Color` = 4 字节 ARGB、`DateTime` = `ToBinary()` 的 i64。
- [ ] 只有 `NPCGoods`(102) 压缩，且读端容忍缺 4 字节尾部的 gzip。
- [ ] 不加任何加密/TLS/混淆。
- [ ] 接收缓冲区按"半帧/多帧"设计（`Vec<u8>` + 游标，或用 `tokio_util::codec` 自写 `Decoder`）。

---

## 13. 源码索引

| 主题 | 位置 |
| --- | --- |
| 包基类、`IsServer`、`Observable`、`Compressed`、`Index` | `Shared/Packet.cs:4-9` |
| `ReceivePacket` 全流程 | `Shared/Packet.cs:11-51` |
| `extra = rawBytes`（先写 out 参数） | `Shared/Packet.cs:13` |
| 包头结构注释（`2Bytes: Packet Size` + `2Bytes: Packet ID`） | `Shared/Packet.cs:17` |
| 长度/编号读取 | `Shared/Packet.cs:19-20` |
| `length` 合法性检查 | `Shared/Packet.cs:22` |
| 方向选择（`IsServer`） | `Shared/Packet.cs:26` |
| 未知编号 → 丢缓冲区 | `Shared/Packet.cs:27-33` |
| 压缩/非压缩包体流 | `Shared/Packet.cs:35-38`（压缩少切 4 字节在 `:36`） |
| 异常统一改写为 `InvalidDataException` | `Shared/Packet.cs:42-45` |
| `extra` 剩余字节复制 | `Shared/Packet.cs:47-48` |
| `GetPacketBytes` 全流程 | `Shared/Packet.cs:53-86` |
| `Index < 0` 返回空数组 | `Shared/Packet.cs:55` |
| 压缩判定与包体写 | `Shared/Packet.cs:66` |
| 包头预留 + 回填长度 | `Shared/Packet.cs:71-79` |
| 编号写入（`short`） | `Shared/Packet.cs:75` |
| `ReadPacket`/`WritePacket` 抽象定义 | `Shared/Packet.cs:88-89` |
| 默认压/解压实现 | `Shared/Packet.cs:91-92` |
| 客户端包编号 → 类的 `switch` | `Shared/Packet.cs:94-406` |
| 服务端包编号 → 类的 `switch` | `Shared/Packet.cs:407-970` |
| gzip 压缩 / 解压 | `Shared/Functions/Functions.cs:486-509` |
| `ServerPacketIds : short` | `Shared/Enums.cs:1434-1715` |
| `ClientPacketIds : short` | `Shared/Enums.cs:1717-1879` |
| 唯一压缩包 `NPCGoods` | `Shared/ServerPackets.cs:3074-3106`（`Compressed => true` 在 `:3077`） |
| 服务端接收：拼接 + 解析循环 | `Server/MirNetwork/MirConnection.cs:129-205` |
| 解析异常 → 断连 + 封 IP 24h | `Server/MirNetwork/MirConnection.cs:173-181` |
| 包数/读取次数限制 | `Server/MirNetwork/MirConnection.cs:151-202`、`Server/Settings.cs:71` |
| 服务端发送：拼接所有包再 `BeginSend` | `Server/MirNetwork/MirConnection.cs:206-284` |
| 服务端 `Packet.IsServer = true` | `Server.MirForms/Program.cs:14` |
| 客户端接收/发送 | `Client/MirNetwork/Network.cs:94-161`、`:186-247` |
| 客户端 `Packet.IsServer = false` | `Client/Program.cs:38` |
| 明文密码（`Login`） | `Shared/ClientPackets.cs:170-190` |
| 明文密码（`NewAccount`） | `Shared/ClientPackets.cs:68-87` |
