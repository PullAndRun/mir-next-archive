# Crystal 协议：登录 / 会话流程（账号 → 选角 → 进世界）

> 参照实现：`C:\Users\diana\ref\Crystal`（只读，未修改）。文中所有 `路径:行号` 都相对于该仓库根目录。
> 数据包编号总表见 [packet-ids.md](packet-ids.md)；本文只覆盖登录、账号、选角、进世界这条链路，以及这条链路上的状态机与枚举。

本文写作目的：为 `传奇-Next` 的 Rust 服务端重写提供**可直接照抄的精确语义**——精确的数字、精确的字符串、精确的字段顺序。凡是 Crystal 里没有名字、只有注释定义的“枚举”（例如 `Login.Result`），本文按注释原文转录，并显式说明它不是一个真实 `enum`。

---

## 0. 速查：实现关键点

| # | 事实 | 证据 |
| --- | --- | --- |
| 1 | 口令是 **PBKDF2-HMAC-SHA1，salt 24 字节随机、迭代 50 次、输出 24 字节**，且输出**先按 UTF-8 解码成字符串再按序数比较**（有损解码，必须复刻） | `Server/Utils/Crypto.cs:8-24`、`Server/MirDatabase/AccountInfo.cs:21-32`、`Server/MirEnvir/Envir.cs:3882-3884` |
| 2 | 阶段枚举是 `GameStage { None, Login, Select, Game, Observer, Disconnected }`，全部合法迁移共 8 条（详见 §5.2），核心路径是 `None → Login → Select → Game` | `Server/MirNetwork/MirConnection.cs:13`、`:112`、`:831`、`:871`、`:1138`、`:1149`、`Envir.cs:3919`、`PlayerObject.cs:1131` |
| 3 | 单账号最多 **4 个角色**（`Globals.MaxCharacterCount = 4`），且角色**创建时只有 Name/Class/Gender**，等级/起始物品/出生点都在**首次进世界**时才生成（首次看到的选择界面等级是 0） | `Shared/Globals.cs:15`、`Envir.cs:4041-4052`、`CharacterInfo.cs:111-120`、`PlayerObject.cs:230`、`HumanObject.cs:234-246` |
| 4 | 版本校验 = 对**客户端 exe 文件字节**算 MD5(16 B)，与 `Configs/Setup.ini` 的 `[General] VersionPath`（逗号分隔、可多文件）在启动时算出的 MD5 列表逐一比对 | `Client/MirScenes/LoginScene.cs:133-152`、`Server/Settings.cs:346-375`、`:42-44`、`:380-381` |
| 5 | 密码错误 **第 6 次**触发 2 分钟封禁；账号重复登录是**踢掉旧连接**（`Disconnect` reason=1），不是拒绝新登录 | `Envir.cs:3886-3898`、`Envir.cs:3911-3916`、`ServerPackets.cs:62-87` |

---

## 1. 连接生命周期

### 1.1 监听与接入（accept）

- 主监听：`new TcpListener(IPAddress.Parse(Settings.IPAddress), Settings.Port)`，随后 `BeginAcceptTcpClient(Connection, null)`（`Server/MirEnvir/Envir.cs:3424-3426`）。默认 `IPAddress = "127.0.0.1"`、`Port = 7000`（`Server/Settings.cs:64-66`）。
- 另有一个**状态端口 3000，端口号硬编码不可配置**，由 `StatusPortEnabled = true` 控制（`Envir.cs:104`、`:3428-3433`）。
- 接入处理 `Envir.Connection`（`Envir.cs:3558-3623`）顺序如下：
  1. 取 IP：`tempTcpClient.Client.RemoteEndPoint.ToString().Split(':')[0]`（`:3574`）。
  2. `IPBlocks` 检查：若该 IP 在封禁表里且未过期，**直接关闭 socket**（`:3576`、`:3608-3609`）。
  3. 同 IP 已连接数 `count >= Settings.MaxIP` → `UpdateIPBlock(ip, 5s)` 并关闭（`:3590-3595`）；默认 `MaxIP = 5`（`Settings.cs:70`）。日志 `"{0} Disconnected, Too many connections."`（`Shared/Language.cs:2532`）。
  4. `new MirConnection(++_sessionID, tempTcpClient)`（`:3598`）。
  5. 接入完成后 `while (Connections.Count >= Settings.MaxUser) Thread.Sleep(1);` 才继续 accept（`:3617`）；默认 `MaxUser = 50`（`Settings.cs:68`）。即“满员时阻塞 accept 线程”，而不是回复一个“服务器满”的包。
- 构造函数 `MirConnection(int sessionID, TcpClient client)`（`Server/MirNetwork/MirConnection.cs:78-102`）：
  - `IPAddress` = 去掉端口的对端 IP（`:81`）。
  - **`Envir.UpdateIPBlock(IPAddress, TimeSpan.FromSeconds(Settings.IPBlockSeconds))`（`:83`）**——注意副作用：每建立一条连接就把该 IP 封 5 秒（默认 `IPBlockSeconds = 5`，`Settings.cs:73`）。因为 IPBlocks 的检查发生在构造**之前**，实际效果是**同一 IP 每 5 秒最多新建一条连接**，已有的旧连接不受影响。Rust 侧若照抄，必须保持“封禁检查在构造前、写封禁在构造中”的次序。
  - `NoDelay = true`（`:88`）；`TimeConnected = Envir.Time`；`TimeOutTime = TimeConnected + Settings.TimeOut`（`:90-91`），默认 `TimeOut = 10000`（毫秒，`Settings.cs:67`）。
  - **服务器发出的第一个包是 `S.Connected`（id=0）**：`_sendList.Enqueue(new S.Connected());`（`:97`），随后 `BeginReceive()`（`:101`）。
- 所有连接的处理都在主循环线程里：`lock (Connections) { for (...) Connections[i].Process(); }`（`Envir.cs:2072-2078`），因此登录/账号逻辑是**单线程串行**的（另有 `Envir.AccountLock` 保护账号表）。

### 1.2 帧格式（入站解帧）

- 线上格式：`| uint16 length（小端，含头） | int16 id（小端） | payload(length-4) |`——`Packet.ReceivePacket`（`Shared/Packet.cs:17-22`）与 `Packet.GetPacketBytes`（`Shared/Packet.cs:69-83`）。
  - `rawBytes.Length < 4` → 返回 `null`，等待更多数据（`:17`）。
  - `length > rawBytes.Length || length < 2` → 返回 `null`，缓冲保留（`:22`）。
  - payload 使用 `new MemoryStream(rawBytes, 4, length - 4)`（`:37`）。
- 解析异常（`ReadPacket` 抛错，含“长度不足”）被转成 `InvalidDataException`（`:42-45`）；`MirConnection.ReceiveData` 捕获后：**IP 封禁 24 小时**、日志 `"{0} Disconnected, Invalid packet."`、断开（`MirConnection.cs:173-181`）。
- **未知 id**：`GetClientPacket` 返回 `null` → 服务端清空整个接收缓冲并返回（`Packet.cs:27-33`）。
- **已注册 id 但没有 case**：`ProcessPacket` 的 `default` 只打日志 `"Invalid packet received. Index : {0}"`，不断开（`MirConnection.cs:754-756`）。
- 限流：以 5 秒为窗口统计收包次数 `_dataCounter`，`_dataCounter > Settings.MaxPacket` 时（默认 `MaxPacket = 50`，`Settings.cs:71`）→ IP 封禁 24 小时 + 日志（含最近 10 个包名）后断开（`MirConnection.cs:151-202`，`_lastPackets` 容量 10 见 `:93`）。
- 发送：`Process()` 每轮把 `_sendList` 全部出队、拼成一个 `byte[]` 一次 `BeginSend`（`:273-284`）。
- 超时：`Envir.Time > TimeOutTime` → `Disconnect(21)`（`:267-271`）；任何收到的包都会刷新 `TimeOutTime = Envir.Time + Settings.TimeOut`（`:257`）。
- 注意：客户端只在发送队列空闲且超时时才发 `KeepAlive`（`Client/MirNetwork/Network.cs:230-231`），服务端收到后原样回 `S.KeepAlive{Time}`（`MirConnection.cs:298-300`、`:873-879`）。

### 1.3 首包 `ClientVersion` 与版本校验

**客户端侧**（`Client/MirScenes/LoginScene.cs`）：

- 收到 `Connected`（id 0）→ `Network.Connected = true; SendVersion();`（`:102-105`）。
- `SendVersion()`：对 `Application.ExecutablePath`（即客户端 exe 自身）算 `MD5`，把 16 字节写进 `C.ClientVersion.VersionHash`（`:133-152`，`MD5.Create()` 在 `:141-143`）。
- 包体编码为 **int32 长度 + 原始字节**：`writer.Write(VersionHash.Length); writer.Write(VersionHash);`（`Shared/ClientPackets.cs:14-22`）。

**服务端侧**（`MirConnection.ClientVersion`，`Server/MirNetwork/MirConnection.cs:836-872`）：

1. `if (Stage != GameStage.None) return;`（`:838`）——**只允许在会话最早期处理一次**；重复发会被静默忽略。
2. `Settings.CheckVersion` 为真时，遍历 `Settings.VersionHashes`，逐个 `Functions.CompareBytes(hash, p.VersionHash)`（`:844-851`）。`CompareBytes` 的语义是“同引用 / 非 null / 长度相等 / 逐字节相等”（`Shared/Functions/Functions.cs:7-16`）——**长度不等直接失败，因此 16 字节以外的哈希（例如 SHA256 的 32 字节）永远无法通过**。
3. **不匹配**：`Disconnecting = true` → 先发 `S.ClientVersion { Result = 0 }`（`:857-861`）→ `SoftDisconnect(10)`（`:862`）→ 日志 `"{0}, Disconnnected - Wrong Client Version."`（`Language.cs:2514`，原文确为三个 n 的 `Disconnnected`）。
4. **匹配**：日志 `"{0}, {1}, Client version matched."`（`Language.cs:2515`）→ `Enqueue(new S.ClientVersion { Result = 1 })`（`:869`）→ **`Stage = GameStage.Login`**（`:871`）。
5. `Settings.CheckVersion == false` 时跳过比对，直接走第 4 步（`:840`）。

**期望哈希是怎么构建的**（`Server/Settings.cs:346-375`，`LoadVersion()`）：

```csharp
VersionHashes = new List<byte[]>();
var paths = VersionPath.Split(',');
foreach (var path in paths) {
    if (File.Exists(path))
        using (FileStream stream = new FileStream(path, FileMode.Open, FileAccess.Read))
        using (MD5 md5 = MD5.Create())
            VersionHashes.Add(md5.ComputeHash(stream));
}
```

- 哈希对象是**文件字节**（这里是客户端 `Mir2.Exe`），不是版本号字符串，也不是 CRC。
- `VersionPath` 支持**逗号分隔的多个路径**；不存在的路径被**静默跳过**（不报错）。
- 边界：`CheckVersion = true` 且所有路径都不存在 ⇒ `VersionHashes` 为空 ⇒ **任何客户端都会被判为版本错误**。
- 配置来源：`Configs/Setup.ini` 的 `[General]` 段（`Server/Settings.cs:37` 构造 `InIReader`，`:380-381` 读键）：
  - `VersionPath`，默认 `".\Mir2.Exe"`（`:42`）
  - `CheckVersion`，默认 `true`（`:43`）
  - INI 解析器为“`[section]` + `key=value` 行”模型，布尔用 `bool.TryParse`（即 `True`/`False`），大小写按序数比较（`Shared/Functions/IniReader.cs:33-43`、`:83-94`）；缺失的键会**回写默认值**到文件。
- 构建时机：`Settings.Load()` 末尾调用 `LoadVersion()`（`Server/Settings.cs:600`），`Settings.Load()` 由服务端程序启动时调用（`Server.MirForms/Program.cs:21`）；配置窗体关闭时也会重新 `LoadVersion()`（`Server.MirForms/ConfigForm.cs:56-60`）。

**客户端对结果的处置**（`Client/MirScenes/LoginScene.cs:153-167`）：

- `Result = 0` → 弹窗 `"Wrong version, please update your game.\nGame will now Close"`（`Shared/Language.cs:3998`），然后 `Network.Disconnect()`。
- `Result = 1` → 关闭“正在连接”提示框，显示登录对话框。

### 1.4 各阶段允许的客户端包

分发表 `ProcessPacket`（`MirConnection.cs:286-758`）**没有全局阶段判定**，而是每个 handler 第一行自己 gate；未通过 gate 的包**静默丢弃**（`return`，不回错误包）。任何阶段都有实际动作的只有 `Disconnect` 与 `KeepAlive`（`LogOut` 虽无阶段 gate，但在 `Game`/`Observer` 之外是空操作）。

| 阶段（`GameStage`） | 有实际动作的客户端包 | 代码 |
| --- | --- | --- |
| `None`（刚 accept，尚未校验版本） | `ClientVersion`(0) | `MirConnection.cs:836-872` |
| `Login`（版本已通过） | `NewAccount`(3)、`ChangePassword`(4)、`Login`(5) | `:880-893`、`:1024-1030` |
| `Select`（登录成功，选角中） | `NewCharacter`(6)、`DeleteCharacter`(7)、`StartGame`(8) | `:1031-1071` |
| `Game`（已进世界） | 全部 gameplay 包（约 140 个 case，handler 首行均为 `if (Stage != GameStage.Game) return;`） | 例：`:1155-1181`、`:1217-1332` |
| `Game` **或** `Observer` | `Chat`(13)、`Inspect`(42)、`Observe`(43)、`GetRanking`(135) | `:1183-1215`、`:1409-1431`、`:2182-2186` |
| 任意阶段（无 gate） | `Disconnect`(1) → `Disconnect(22)`；`KeepAlive`(2) → 回显 | `:295-300`、`:873-879` |
| `Game` 之外**会回错误包**而不是静默丢弃 | `UnlockStorage`(150) → `S.StorageUnlockResult{Result=3}`；`SetStoragePassword`(151)/`RemoveStoragePassword`(152) → `S.StoragePasswordResult{Result=0}` | `:894-1006` |

补充细节：

- `LogOut`(9) 只在 `Game` / `Observer` 阶段有动作；在 `Select` 阶段调用是**空操作**（`:1126-1153`，`if (Stage == GameStage.Game) ... else if (Stage == GameStage.Observer) ...`）。
- `Chat` 的长度检查在阶段判断**之前**：任何阶段发超过 `Globals.MaxChatLength = 80` 的消息都会被 `SendDisconnect(2)`（`:1183-1189`、`Shared/Globals.cs:17`）。这是阶段机里唯一的跨阶段副作用。
- `Observer` 由观察功能设置（`AddObserver`，`:104-113`）；被观察者下线时 `CleanObservers()` 把每个观察者**打回 `Login`** 并发 `S.ReturnToLogin`（`:827-834`）。
- 服务端主动断开只有三种形态：`Disconnect(byte reason)`（立即，不发包，`:776-808`）、`SendDisconnect(byte reason)`（先发 `S.Disconnect{Reason}` 再 `SoftDisconnect`，`:809-826`）、以及超时/非法包/刷包三条内部路径。`SoftDisconnect` 会 `Player.StopGame(reason)` 并把 `Account.Connection` 置空（`:760-775`）。
- **`SoftDisconnect` 不会立刻关 socket**：它只把 `Disconnecting` 置真（该属性 setter 顺手把 `TimeOutTime` 改成 `Envir.Time + 500`，`:39-48`）、解绑账号与玩家；真正的 `Disconnect()`（内部 `_client.Client.Dispose()`）由 `Process()` 在约 500 ms 后从超时分支 `Disconnect(21)` 触发（`:267-271`、`:776-808`）。也就是说“拒绝版本 / 顶号踢人”与实际断链之间有最多 500 ms 窗口，期间已入队的包仍会被 `BeginSend` 发出。重写时若想“发完即关”，需要自己保证写缓冲 flush 后再 `close()`。

---

## 2. 账号流程

### 2.1 `NewAccount`（Client→Server id = 3）

**请求包体**（`Shared/ClientPackets.cs:53-88`，按写入顺序）：

| 字段 | 编码 |
| --- | --- |
| `AccountID` | `string`（BinaryWriter：7-bit 变长长度前缀 + UTF-8） |
| `Password` | `string` |
| `BirthDate` | `int64`（`DateTime.ToBinary()`） |
| `UserName` | `string` |
| `SecretQuestion` | `string` |
| `SecretAnswer` | `string` |
| `EMailAddress` | `string` |

**处理入口**：`MirConnection.NewAccount` 要求 `Stage == GameStage.Login`（`MirConnection.cs:880-886`），先记日志 `"{0}, {1}, New account being created."`，再委托 `Envir.NewAccount`（`Server/MirEnvir/Envir.cs:3649-3731`）。

**校验顺序与返回码（返回码即 `S.NewAccount{Result}` 的 `byte` 值）**：

| 顺序 | 条件 | Result | 证据 |
| --- | --- | --- | --- |
| 1 | `!Settings.AllowNewAccount` | `0` | `Envir.cs:3651-3655` |
| 2 | 该 IP 在 1 小时窗口内建号次数 `> 2` → `IPBlocks[ip] = Now + 24h` | `0` | `Envir.cs:3658-3665` |
| 3 | `!AccountIDReg.IsMatch(p.AccountID)` | `1` | `Envir.cs:3682-3686` |
| 4 | `!PasswordReg.IsMatch(p.Password)` | `2` | `Envir.cs:3688-3692` |
| 5 | 邮箱非空且不匹配 `EMailReg`，**或** `EMailAddress.Length > 50` | `3` | `Envir.cs:3693-3698` |
| 6 | `UserName` 非空且 `Length > 20` | `4` | `Envir.cs:3700-3704` |
| 7 | `SecretQuestion` 非空且 `Length > 30` | `5` | `Envir.cs:3706-3710` |
| 8 | `SecretAnswer` 非空且 `Length > 30` | `6` | `Envir.cs:3712-3716` |
| 9 | `AccountExists(p.AccountID)`（**忽略大小写**） | `7` | `Envir.cs:3718-3724`、`:4121-4128` |
| 10 | 成功：`AccountList.Add(new AccountInfo(p){ Index = ++NextAccountID, CreationIP = c.IPAddress })` | `8` | `Envir.cs:3726-3729` |

**字符集规则（都是白名单正则，不存在“保留字符黑名单”）**（`Envir.cs:183-186`）：

```csharp
AccountIDReg = new Regex(@"^[A-Za-z0-9]{" + Globals.MinAccountIDLength + "," + Globals.MaxAccountIDLength + "}$");
PasswordReg  = new Regex(@"^[A-Za-z0-9]{" + Globals.MinPasswordLength  + "," + Globals.MaxPasswordLength  + "}$");
EMailReg     = new Regex(@"\w+([-+.]\w+)*@\w+([-.]\w+)*\.\w+([-.]\w+)*");
CharacterReg = new Regex(@"^[\u4e00-\u9fa5_A-Za-z0-9]{" + Globals.MinCharacterNameLength + "," + Globals.MaxCharacterNameLength + "}$");
```

常量值（`Shared/Globals.cs:7-15`）：`MinAccountIDLength = 3`、`MaxAccountIDLength = 15`、`MinPasswordLength = 5`、`MaxPasswordLength = 15`、`MinCharacterNameLength = 3`、`MaxCharacterNameLength = 15`、`MaxCharacterCount = 4`。

由此可推出的精确语义：

- 账号名 / 密码**只允许 ASCII 字母与数字**，长度 3–15 / 5–15；中文、下划线、空格、任何标点都被拒。
- 邮箱可留空（空串跳过格式校验），非空时最长 50。
- `BirthDate` **完全不校验**（客户端可传任意值）。
- 已存在的判断是**忽略大小写**的线性扫描（`string.Compare(..., StringComparison.OrdinalIgnoreCase)`，`Envir.cs:4121-4128`），但**新建时保留原大小写**。
- 建号/建角限流按 IP 统计，窗口 1 小时，清理是“移除第一个过期项”的近似实现（`Envir.cs:3666-3674`）。

**客户端文案**（`Client/MirScenes/LoginScene.cs:178-221` 的 switch；字符串出自 `Shared/Language.cs:3999-4007`）：

| Result | 客户端显示（原文） |
| --- | --- |
| 0 | `Account creation is currently disabled.` |
| 1 | `Your AccountID is not acceptable.` |
| 2 | `Your Password is not acceptable.` |
| 3 | `Your E-Mail Address is not acceptable.` |
| 4 | `Your User Name is not acceptable.` |
| 5 | `Your Secret Question is not acceptable.` |
| 6 | `Your Secret Answer is not acceptable.` |
| 7 | `An Account with this ID already exists.` |
| 8 | `Your account was created successfully.` |

**HTTP 旁路**：`Envir.HTTPNewAccount` 返回同一套 0–8 数值（`Envir.cs:3733-3780`）；`HTTPLogin` 返回 0–7（`Envir.cs:3928-3975`，7 = 成功）。HTTP 服务由 `StartHTTPService`（默认 `false`）、`HTTPIPAddress`（默认 `http://127.0.0.1:5679/`）、`HTTPTrustedIPAddress`（默认 `127.0.0.1`）控制（`Settings.cs:76-78`）。

### 2.2 口令哈希算法与盐

- 盐：`Crypto.GenerateSalt()` = `RandomNumberGenerator.Create()` 生成 **24 字节**（`Server/Utils/Crypto.cs:8`、`:11-18`）。
- 哈希：

```csharp
public const int SaltSize = 24, HashSize = 24, Iterations = 50;
Rfc2898DeriveBytes pbkdf2 = new Rfc2898DeriveBytes(password, salt, Iterations, HashAlgorithmName.SHA1);
return Encoding.UTF8.GetString(pbkdf2.GetBytes(HashSize));   // Server/Utils/Crypto.cs:20-24
```

即 **PBKDF2-HMAC-SHA1(password, salt, 50 iterations) → 24 字节 → UTF-8 解码成字符串**。

- 写入时机：`AccountInfo.Password` 的 **setter 每次赋值都会重新生成盐**并重算哈希（`Server/MirDatabase/AccountInfo.cs:21-32`），因此改密码 = 换盐 + 换哈希。盐单独存一列（`byte[24]`，`:32`；序列化 `:206-207`，反序列化 `:108-109`）。
- 校验时机：登录/改密时把客户端明文按**已存盐**重算，再 `string.CompareOrdinal(account.Password, p.Password) != 0`（`Envir.cs:3882-3884`、`:3828-3833`）。整个握手过程**没有任何挑战/随机数**，客户端发明文（`Shared/ClientPackets.cs:170-190`）。
- **兼容性陷阱（必须原样复刻）**：`Encoding.UTF8.GetString(24 字节)` 是**有损解码**——随机 PBKDF2 输出经常不是合法 UTF-8，非法序列会被替换为 `U+FFFD`。数据库里存的是**这个字符串**，比较也是**字符串序数比较**。Rust 侧如果改成“存 24 字节原始哈希 + 按字节比较”，则对老库中所有“曾被替换过”的账号都会验证失败。两种可行做法：
  1. 复刻语义：`String::from_utf8_lossy(&pbkdf2_out).to_string()` 后与库中字符串比较；
  2. 或另存 `hash_bytes` 新列，老账号首次成功登录时再迁移（需要先用做法 1 验证一次）。
- 同一套函数也被仓库密码（`StoragePassword` + `StorageSalt`）复用（`AccountInfo.cs:34-46`、`:288-294`）。

### 2.3 `Login`（Client→Server id = 5）

**请求包体**：`AccountID:string`、`Password:string`（`Shared/ClientPackets.cs:170-190`）。

**处理**：`MirConnection.Login` 要求 `Stage == GameStage.Login`（`MirConnection.cs:1024-1030`），先记日志 `"{0}, {1}, User logging in."`，再委托 `Envir.Login`（`Envir.cs:3839-3926`）。

**完整判定顺序**：

| 顺序 | 条件 | 服务端动作 | 证据 |
| --- | --- | --- | --- |
| 1 | `!Settings.AllowLogin` | `S.Login{Result=0}` | `Envir.cs:3841-3845` |
| 2 | `!AccountIDReg.IsMatch(p.AccountID)` | `S.Login{Result=1}` | `:3847-3851` |
| 3 | `!PasswordReg.IsMatch(p.Password)` | `S.Login{Result=2}` | `:3853-3857` |
| 4 | `GetAccount(p.AccountID) == null`（忽略大小写） | `S.Login{Result=3}` | `:3858-3864` |
| 5 | `account.Banned && account.ExpiryDate > Now` | `S.LoginBanned{Reason, ExpiryDate}` | `:3866-3876` |
| 5b | 已过期的 Ban | 清 `Banned=false`，继续（并清空 `BanReason`/`ExpiryDate`） | `:3877-3880` |
| 6 | 哈希不匹配且 `account.WrongPasswordCount++ >= 5` | 封 2 分钟：`Banned=true`、`BanReason = "Too many Wrong Login Attempts."`、`ExpiryDate = Now.AddMinutes(2)`、回 `S.LoginBanned` | `:3884-3898` |
| 6b | 哈希不匹配（未到阈值） | `S.Login{Result=4}` | `:3900-3901` |
| 7 | `account.RequirePasswordChange` | `S.Login{Result=5}` | `:3905-3909` |
| 8 | 成功 | 见下 | `:3911-3925` |

成功路径逐句：

```csharp
account.WrongPasswordCount = 0;                       // :3903
lock (AccountLock) {
    account.Connection?.SendDisconnect(1);            // 踢掉旧连接，reason=1
    account.Connection = c;
}
c.Account = account;
c.Stage = GameStage.Select;                           // :3919  ← 登录成功的阶段迁移点
account.LastDate = Now;  account.LastIP = c.IPAddress;
c.Enqueue(new ServerPackets.LoginSuccess { Characters = account.GetSelectInfo() });  // :3925
```

**要点**：

- **“账号已登录”不是拒绝，而是顶号**：旧连接收到 `S.Disconnect{Reason=1}`（注释原文 `1: Another User.`，`Shared/ServerPackets.cs:71-76`），然后走 `SoftDisconnect(1)` → `Player.StopGame(1)` 正常存盘（`MirConnection.cs:760-775`；reason 1 的日志文案 `"{0} Has logged out. Reason: Double login"`，`Language.cs:1940`）。
- **没有“账号锁定（locked）”字段**：封禁只有 `Banned`(bool) + `ExpiryDate`(DateTime) + `BanReason`(string) 三件套（`AccountInfo.cs:58-61`）。`RequirePasswordChange` 是另一回事（强制改密，Result=5）。
- 密码错误计数是 **post-increment**：判定用的是自增前的值，所以**第 6 次**错误才触发封禁。封禁时**不清零**计数（只有成功登录才重置，`:3903`），因此 2 分钟封禁结束后再错一次会立即再次被封。
- 登录成功后 `Stage` 直接从 `Login` 跳到 `Select`，`S.LoginSuccess` 是在 Stage 改完之后才入队的（`:3919` → `:3925`）。
- 客户端对 `S.Login` 的处理（`Client/MirScenes/LoginScene.cs:267-299`）：
  - `0` → `Logging in is currently disabled.`
  - `1` → `Your AccountID is not acceptable.`
  - `2` → `Your Password is not acceptable.`
  - `3` → `The AccountID does not exist.`
  - `4` → `Incorrect Password and AccountID combination.`
  - `5` → `The account's password must be changed before logging in.`，并**直接弹改密框、预填账号与旧密码**（`:293-297`）
  - `S.LoginBanned` → `This account is banned.\n\nReason: {0}\nExpiryDate: {1}\nDuration: {2:#,##0} Hours, {3} Minutes, {4} Seconds`（`Language.cs:4012`）
  - `S.LoginSuccess` → 播放登录音效、场景淡出后 `new SelectScene(p.Characters)`（`:308-321`）

### 2.4 `ChangePassword`（Client→Server id = 4）

**请求包体**：`AccountID:string`、`CurrentPassword:string`、`NewPassword:string`（`Shared/ClientPackets.cs:89-112`）。

**处理**：要求 `Stage == GameStage.Login`（`MirConnection.cs:887-893`），日志 `"{0}, {1}, Password being changed."`，委托 `Envir.ChangePassword`（`Envir.cs:3782-3838`）。

| 顺序 | 条件 | Result | 证据 |
| --- | --- | --- | --- |
| 1 | `!Settings.AllowChangePassword` | `0` | `:3784-3788` |
| 2 | `!AccountIDReg.IsMatch(AccountID)` | `1` | `:3790-3794` |
| 3 | `!PasswordReg.IsMatch(CurrentPassword)` | `2` | `:3796-3800` |
| 4 | `!PasswordReg.IsMatch(NewPassword)` | `3` | `:3802-3806` |
| 5 | `GetAccount(...) == null` | `4` | `:3808-3814` |
| 6 | 账号被封且未过期 | 回 `S.ChangePasswordBanned{Reason, ExpiryDate}`（不占 Result 编号） | `:3816-3824` |
| 7 | 旧密码哈希序数比较不等 | `5` | `:3828-3833` |
| 8 | 成功：`account.Password = p.NewPassword`（新建盐+哈希）、`RequirePasswordChange = false` | `6` | `:3835-3837` |

**要点**：

- **没有“新旧密码必须不同”的检查**；**不重置** `WrongPasswordCount`；**不重置** `Banned`（只清理已过期的）。
- 成功路径里的 `account.Password = ...` 会走 setter，因此盐一定变（`AccountInfo.cs:21-30`），旧哈希立即失效。
- 客户端文案（`LoginScene.cs:222-258`；字符串见 `Language.cs`）：
  - `0` → `Password Changing is currently disabled.`（:4008）
  - `1` → `Your AccountID is not acceptable.`（:4000）
  - `2` → `The current Password is not acceptable.`（:4009）
  - `3` → `Your new Password is not acceptable.`（:4010）
  - `4` → `The AccountID does not exist.`（:2958，键 `NoAccountID`）
  - `5` → `Incorrect Password and AccountID combination.`（:2959）
  - `6` → `Your password was changed successfully.`（:4011）
  - `S.ChangePasswordBanned` → `This account is banned.\n\nReason: {0}\nExpiryDate: {1}\nDuration: {2:#,##0} Hours, {3} Minutes, {4} Seconds`（:4012，键 `AccountBannedReasonDuration`）
- 客户端 UI 为三层确认：账号 + 当前密码 + 新密码 ×2（`LoginScene.cs:1263`）。

### 2.5 登录相关服务器回包一览

| id | 包 | 字段（写入顺序） | 含义 |
| --- | --- | --- | --- |
| 0 | `Connected` | 无 | 连接建立后的第一包；客户端据此发 `ClientVersion`（`ServerPackets.cs:24-38`） |
| 1 | `ClientVersion` | `byte Result`（0=错误版本，1=正确） | 版本校验结果（`:39-61`） |
| 2 | `Disconnect` | `byte Reason` | 服务端主动断开及原因（`:62-87`） |
| 3 | `KeepAlive` | `long Time` | 心跳回显（`:6-23`） |
| 4 | `NewAccount` | `byte Result` | 建号结果 0–8（`:88-118`） |
| 5 | `ChangePassword` | `byte Result` | 改密结果 0–6（`:119-146`） |
| 6 | `ChangePasswordBanned` | `string Reason`、`int64 ExpiryDate`(ToBinary) | 封禁中的账号改密（`:147-168`） |
| 7 | `Login` | `byte Result` | 登录结果 0–5（`:234-259`） |
| 8 | `LoginBanned` | `string Reason`、`int64 ExpiryDate` | 封禁（含密码错误过多）（`:260-281`） |
| 9 | `LoginSuccess` | `int32 count` + `SelectInfo[count]` | 登录成功 + 角色列表（`:282-306`） |

两个关键包的完整定义（原文，`Shared/ServerPackets.cs:234-306`）：

```csharp
public sealed class Login : Packet
{
    public override short Index { get { return (short)ServerPacketIds.Login; } }
    public byte Result;
    /* 0: Disabled
     * 1: Bad AccountID
     * 2: Bad Password
     * 3: Account Not Exist
     * 4: Wrong Password */
    protected override void ReadPacket(BinaryReader reader)  { Result = reader.ReadByte(); }
    protected override void WritePacket(BinaryWriter writer) { writer.Write(Result); }
}

public sealed class LoginSuccess : Packet
{
    public override short Index { get { return (short)ServerPacketIds.LoginSuccess; } }
    public List<SelectInfo> Characters = new List<SelectInfo>();
    protected override void ReadPacket(BinaryReader reader) {
        int count = reader.ReadInt32();
        for (int i = 0; i < count; i++) Characters.Add(new SelectInfo(reader));
    }
    protected override void WritePacket(BinaryWriter writer) {
        writer.Write(Characters.Count);
        for (int i = 0; i < Characters.Count; i++) Characters[i].Save(writer);
    }
}
```

> 注意 `Login` 的注释只列到 `4`，而代码还会发 `5`（强制改密，`Envir.cs:3905-3909`）；`LoginBanned` 与 `Login` 是**两个不同的包**（id 8 vs id 7），不是同一个 Result 取值。

---

## 3. 角色流程

### 3.1 角色列表（`SelectInfo`）

Crystal **没有** `GetCharacterInfo` 这个包。角色列表随 `LoginSuccess`（id 9）与 `LogOutSuccess`（id 58）以 `List<SelectInfo>` 一次性下发（`Shared/ServerPackets.cs:282-306`、`:2008-2034`）。

来源：`AccountInfo.GetSelectInfo()`（`Server/MirDatabase/AccountInfo.cs:252-264`）——**跳过 `Deleted` 的角色，最多返回 `Globals.MaxCharacterCount = 4` 条**；每条由 `CharacterInfo.ToSelectInfo()` 生成（`Server/MirDatabase/CharacterInfo.cs:572-583`）：

```csharp
return new SelectInfo { Index = Index, Name = Name, Level = Level, Class = Class,
                        Gender = Gender, LastAccess = LastLogoutDate };
```

**字段顺序与类型（这是选角界面上唯一的角色记录结构）**（`Shared/Data/SharedData.cs:3-32`）：

| # | 字段 | 线上类型 | 说明 |
| --- | --- | --- | --- |
| 1 | `Index` | `int32` | 角色全局唯一 ID（`CharacterInfo.Index`） |
| 2 | `Name` | `string` | 角色名 |
| 3 | `Level` | `uint16` | **首次进游戏前为 0**（见 3.2） |
| 4 | `Class` | `byte` | `MirClass`（0–4） |
| 5 | `Gender` | `byte` | `MirGender`（0–1） |
| 6 | `LastAccess` | `int64` | `LastLogoutDate.ToBinary()`；`DateTime.MinValue` 表示从未登录 |

客户端把列表按 `LastAccess` **降序**排序（`Client/MirScenes/SelectScene.cs:228-232`，构造时立即 `SortList()`，见 `:23-29`），`_selected` 默认 0，因此默认选中“最近登出”的那个角色（`:21`、`:530`）。

### 3.2 `NewCharacter`（Client→Server id = 6）

**请求包体**：`string Name`、`byte Gender`、`byte Class`（`Shared/ClientPackets.cs:191-210`）。

**处理**：要求 `Stage == GameStage.Select`（`MirConnection.cs:1031-1036`）；把 `Account.AdminAccount` 作为 `IsGm` 传进去（`:1035`），即 **GM 账号可绕过保留名检查**。

**校验顺序与 Result**（`Envir.cs:3977-4069`）：

| 顺序 | 条件 | Result | 证据 |
| --- | --- | --- | --- |
| 1 | `!Settings.AllowNewCharacter` | `0` | `:3979-3983` |
| 2 | 该 IP 1 小时内建角次数 `> 4` → `IPBlocks[ip] = Now + 24h` | `0` | `:3985-3992` |
| 3 | `!CharacterReg.IsMatch(p.Name)`，或（非 GM 且）名字在 `DisabledCharNames` 中（按大写比对） | `1` | `:4009-4019` |
| 4 | `Gender != Male && Gender != Female` | `2` | `:4021-4025` |
| 5 | `Class` 不属于 Warrior/Wizard/Taoist/Assassin/Archer，或该类被 `AllowCreateAssassin`/`AllowCreateArcher` 关闭 | `3` | `:4027-4039` |
| 6 | 该账号非删除角色数已达上限 | `4` | `:4041-4052` |
| 7 | `CharacterExists(p.Name)`（忽略大小写，扫全部角色含已删除未归档的） | `5` | `:4054-4060` |
| 8 | 成功：`new CharacterInfo(p, c){ Index = ++NextCharacterID, AccountInfo = c.Account }`，加入账号与全局表，回 `S.NewCharacterSuccess{ CharInfo = info.ToSelectInfo() }` | — | `:4062-4067` |

**名字规则**：`^[\u4e00-\u9fa5_A-Za-z0-9]{3,15}$`（`Envir.cs:186`）——即 **CJK 基本区汉字 / 下划线 / ASCII 字母数字**，长度 3–15；其他任何字符一律拒绝。唯一性判定 `string.Compare(CharacterList[i].Name, name, OrdinalIgnoreCase) == 0`（`Envir.cs:4130-4137`）。

**保留名**：`Envir\DisabledChars.txt`（`Settings.EnvirPath` 下，逐行 `ToUpper()` 载入，`;` 开头与空行忽略；文件不存在会自动创建空文件）（`Envir.cs:3188-3208`）。仅作用于角色名，不影响账号名。

**上限**：`if (++count >= Globals.MaxCharacterCount) → Result = 4`（`:4047-4051`），`MaxCharacterCount = 4` ⇒ **最多 4 个未删除角色**。

**角色创建时到底写了什么**：`CharacterInfo(p, c)` 只赋值 `Name`、`Class`、`Gender`、`CreationIP`、`CreationDate`，外加 `Heroes = new HeroInfo[MaximumHeroCount]`（`CharacterInfo.cs:111-120`）。**`Level`、`HP/MP`、`CurrentMapIndex`、`BindMapIndex` 等全部保持默认 0**。

**起始等级 / 物品 / 位置的真正生成点**（首次进世界的 `PlayerObject.Load`）：

1. `if (Level == 0) NewCharacter();`（`Server/MirObjects/PlayerObject.cs:230`）——用“等级 0”作为“从未初始化”的哨兵。
2. `PlayerObject.NewCharacter()`：若 `Envir.StartPoints` 为空则**直接 return**（不建人物状态）；否则 `SetBind()`（`PlayerObject.cs:468-475`）。
3. `SetBind()`：`SafeZoneInfo szi = Envir.StartPoints[Envir.Random.Next(Envir.StartPoints.Count)]; BindMapIndex = szi.Info.Index; BindLocation = szi.Location;`（`PlayerObject.cs:1057-1063`）。`StartPoints` 来自地图定义里带 `StartPoint` 标记的安全区（`Server/MirDatabase/MapInfo.cs:213-214`、`Server/MirDatabase/SafeZoneInfo.cs:8`）。
4. `HumanObject.NewCharacter()`：`Level = 1`；`Hair = (byte)Envir.Random.Next(0, 9)`（0–8）；遍历 `Envir.StartItems`，用 `CorrectStartItem(info)` 过滤 `RequiredClass` / `RequiredGender`，命中则 `AddItem(Envir.CreateFreshItem(info))`（`Server/MirObjects/HumanObject.cs:234-246`；过滤逻辑 `:1631-1667`）。
5. `Envir.StartItems` = 物品表里 `ItemInfo.StartItem == true` 的所有条目（`Envir.cs:3380-3386`）——**因此“每职业起始装备”不是代码常量，而是 DB 数据**。
6. 之后 `RefreshStats()`；若 `HP == 0` 则 `SetHP(Stats[Stat.HP])`、`SetMP(Stats[Stat.MP])`，并把 `CurrentLocation/CurrentMapIndex` 设为绑定点（`PlayerObject.cs:240-261`）；`PKPoints >= 200` 时改投 `PKTown`（`:250-260`）。

> 结论：**不存在“每职业硬编码的初始属性表”**。等级固定 1、发型随机 0–8、属性由 `RefreshStats()` 从 `Shared/BaseStats.cs` + 职业公式推导、装备来自 DB 的 `StartItem` 标记、出生点来自地图 `StartPoint` 安全区随机取一。

**客户端确认流程**：`SelectScene` 打开 `NewCharacterDialog`，创建时发 `C.NewCharacter{Name, Class, Gender}`（`Client/MirScenes/SelectScene.cs:234-252`）；客户端自己的文件名正则为 `^[A-Za-z0-9]|[\u4e00-\u9fa5]{3,15}$`（`Client/MirScenes/Dialogs/NewCharacterDialog.cs:10`，注意与服务器正则**不一致**，且运算符优先级有 bug），输入框 `MaxLength = 15`（`:89`）。

**客户端文案**（`SelectScene.cs:323-352`；`Language.cs:3034-3039`）：`0`→`Creating new characters is currently disabled.`、`1`→`Your Character Name is not acceptable.`、`2`→`The gender you selected does not exist.\n Contact a GM for assistance.`、`3`→`The class you selected does not exist.\n Contact a GM for assistance.`、`4`→`You cannot make anymore then {0} Characters.`（原文如此，`{0}` = 4；`ClientTextKeys.YouCannotMakeMoreCharacters`，`Language.cs:3038`）、`5`→`A Character with this name already exists.`；成功后 `Your character was created successfully.` 并把 `p.CharInfo` 插到列表首位（`:353-361`）。

### 3.3 `DeleteCharacter`（Client→Server id = 7）

**请求包体**：`int32 CharacterIndex`（`Shared/ClientPackets.cs:211-225`）。

**处理**（`MirConnection.cs:1037-1068`，要求 `Stage == GameStage.Select`）：

| 条件 | 回包 | 证据 |
| --- | --- | --- |
| `!Settings.AllowDeleteCharacter` | `S.DeleteCharacter{Result=0}` | `:1041-1045` |
| 该 `CharacterIndex` 不在 `Account.Characters` 中 | `S.DeleteCharacter{Result=1}` | `:1047-1062` |
| 成功 | `temp.Deleted = true; temp.DeleteDate = Now; Envir.RemoveRank(temp);` 然后 `S.DeleteCharacterSuccess{CharacterIndex}` | `:1064-1067` |

**关于“延迟/锁定”**（这是最容易误判的一点）：

- **删除是立即生效的软删除**：只置 `Deleted = true` 与 `DeleteDate = Now`（`:1064-1065`，字段定义 `CharacterInfo.cs:37-38`），此后 `GetSelectInfo()` 立刻不再返回它（`AccountInfo.cs:256-258`），客户端的列表也立刻移除该条（`SelectScene.cs:407-420`）。
- 服务端**没有任何等待期、确认码、冷却时间或反向撤销包**；也**不检查角色是否在线**（`Stage == Select` 本身保证没进世界）。
- 唯一有时间语义的是**归档清理**：账号加载时，若 `Deleted && DeleteDate.AddMonths(Settings.ArchiveDeletedCharacterAfterMonths) <= Now`，则把角色存档到归档文件并从账号里剔除（`AccountInfo.cs:143-148`），默认 `ArchiveDeletedCharacterAfterMonths = 1`（`Settings.cs:341`）。另有“从未登录且创建超过 N 月”“超过 N 月未登录”两种归档（`AccountInfo.cs:150-162`），默认 `ArchiveInactiveCharacterAfterMonths = 12`（`Settings.cs:340`）。
- 归档前，**已删除角色的名字仍然被占用**（`CharacterExists` 扫的是全体 `CharacterList`，`Envir.cs:4130-4137`）。它仍留在 `CharacterList` 与账号的角色数组中（占用 DB 行），但**不计入**新建角色的 4 个名额（`Envir.cs:4045` 显式 `continue` 跳过 `Deleted`），也不出现在选角列表里。
- 客户端侧的两重确认（是/否弹窗 + 手动输入角色名，`SelectScene.cs:363-392`）**纯属 UI**，服务端不校验任何确认令牌；同时客户端在删除期间禁用按钮（`:379`）。
- 客户端文案：`0`→`Deleting characters is currently disabled.`、`1`→`The character you selected does not exist.\n Contact a GM for assistance.`、成功→`Your character was deleted successfully.`（`Language.cs:3043-3045`）。

### 3.4 `StartGame`（Client→Server id = 8）

**请求包体**：`int32 CharacterIndex`（`Shared/ClientPackets.cs:226-240`）。

**处理**（`MirConnection.cs:1069-1124`，要求 `Stage == GameStage.Select`）：

| 顺序 | 条件 | 回包 | 证据 |
| --- | --- | --- | --- |
| 1 | `!Settings.AllowStartGame && (Account == null || !Account.AdminAccount)` | `S.StartGame{Result=0}` | `:1073-1077` |
| 2 | `Account == null` | `S.StartGame{Result=1}` | `:1079-1083` |
| 3 | `Account.Characters` 中找不到该 Index | `S.StartGame{Result=2}` | `:1086-1099` |
| 4 | 角色被封且 `ExpiryDate > Now` | `S.StartGameBanned{Reason, ExpiryDate}` | `:1101-1107` |
| 4b | 角色曾封禁但已过期 | 清 `Banned/BanReason/ExpiryDate` 后继续 | `:1108-1111` |
| 5 | 成功 | `Player = new PlayerObject(info, this); Player.StartGame();` | `:1122-1123` |

**注意默认配置**：`AllowStartGame = false`（`Settings.cs:86`）⇒ **默认只有 `AdminAccount` 能进游戏**，普通账号会收到 `Result=0`，客户端的文案是 `Starting the game is currently disabled.`（`Language.cs:3048`）。Rust 侧若想“开箱即用”，必须把默认值改掉或显式配置。

> 判定顺序上的一个细节：第 1 条用的是 `if (!Settings.AllowStartGame && (Account == null || (Account != null && !Account.AdminAccount)))`（`MirConnection.cs:1073`）。因为 `AllowStartGame == false` 时 `Account == null` 也走 `Result=0`，**`Result=1` 实际不可达**（能走到第 2 条时 `AllowStartGame` 必为 true，而 `Stage == Select` 本身就意味着已登录）。重写时可以合并这两个分支，但要保留 `Result=0/1` 的取值以免客户端文案错位。

**重登冷却 `StartGameDelay` 已被注释掉**：`:1113-1120` 的 `if (delay < Settings.RelogDelay) { Enqueue(new S.StartGameDelay{...}); return; }` 整段被注释，因此 **`StartGameDelay`（id 16）在这个版本永远不会下发**——尽管客户端仍实现了它的倒计时逻辑（`Client/MirScenes/SelectScene.cs:422-441`），包体定义也还在（`ServerPackets.cs:449-468`）。`Settings.RelogDelay = 50` 仍然存在但只被读取/写回（`Settings.cs:69`、`:382`、`:671`）。

**进世界时服务器发出的内容与顺序**（`PlayerObject.StartGame()` → `StartGameSuccess()`）：

`StartGame()` 先做地图合法性检查：`CurrentMapIndex` 指向的图若无效则回退到 `BindMapIndex`，再无效则 `SetBind()` 后重试；仍然无效 → `StartGameFailed()`（`PlayerObject.cs:1069-1104`）。`StartGameFailed()` 回 `S.StartGame{Result=3}` 并 `CleanUp()`（`:1353-1357`）。

`StartGameSuccess()` 的第一件事就是**阶段迁移**：

```csharp
Connection.Stage = GameStage.Game;                                   // :1131
Enqueue(new S.StartGame { Result = 4, Resolution = Settings.AllowedResolution }); // :1133
```

随后（按代码顺序，`PlayerObject.cs:1133-1351`）：

| 顺序 | 包 | 说明 |
| --- | --- | --- |
| 1 | `S.StartGame{Result=4, Resolution=Settings.AllowedResolution}` | **成功标志**；客户端仅凭它切到 `GameScene` 并改分辨率（`SelectScene.cs:468-492`）。默认 `AllowedResolution = 1024`（`Settings.cs:91`，INI 键 `[Permission] MaxResolution`，`:414`） |
| 2 | `S.Chat`（Welcome / TestServer 提示）、`S.UpdateNotice`（若有公告且比上次登出更新） | `:1134-1175` |
| 3 | `S.NewItemInfo` × N（`GetItemInfo`） | `:1179` |
| 4 | `S.MapInformation`（`GetMapInfo`） | `:1710-1726`；当前地图一条 |
| 5 | `S.UserInformation`（`GetUserInfo`） | `:1649-1709`；自身完整状态 |
| 6 | `Spawned()` → 周围对象（`S.ObjectPlayer`/`ObjectMonster`/`ObjectItem`…）、任务/配方/邮件/好友/师徒/征服/商城 | `:1183-1213` |
| 7 | `S.BaseStats`、`S.TimeOfDay`、`S.ChangeAMode`、`S.ChangePMode`、`S.SwitchGroup`、`S.DefaultNPC`、`S.GuildBuffList`、按需 `S.SpellToggle` | `:1214-1229` |
| 8 | 宠物/英雄/增益/毒的重建；`UpdateGMBuff()`（仅 GM） | `:1231-1343` |

**`MapInformation`（id 17）字段顺序**（`ServerPackets.cs:469-517`，写入 `:501-516`）：

1. `int32 MapIndex`
2. `string FileName`
3. `string Title`
4. `uint16 MiniMap`
5. `uint16 BigMap`
6. `byte Lights`（`LightSetting`）
7. `byte bools`——**位标志位图**：`0x01 = Lightning`、`0x02 = Fire`（读侧 `:493-495`）
8. `byte MapDarkLight`
9. `uint16 Music`
10. `uint16 WeatherParticles`

**`UserInformation`（id 21）字段顺序**（`ServerPackets.cs:713-803`，读侧 `:632-711`）：

1. `uint32 ObjectID`（地图内对象 ID，临时）
2. `uint32 RealId`（= `CharacterInfo.Index`，`PlayerObject.cs:1656`）
3. `string Name`
4. `string GuildName`（无公会为空串）
5. `string GuildRank`
6. `int32 NameColour`（`Color.ToArgb()`）
7. `byte Class`
8. `byte Gender`
9. `uint16 Level`
10. `int32 Location.X`、`int32 Location.Y`
11. `byte Direction`（`MirDirection`）
12. `byte Hair`
13. `int32 HP`、`int32 MP`
14. `int64 Experience`、`int64 MaxExperience`
15. `uint16 LevelEffects`（`[Flags]`）
16. `bool HasHero`
17. `byte HeroBehaviour`
18. `bool Inventory != null` → `int32 len` → 每槽 `bool 非空` + `UserItem.Save`
19. `bool Equipment != null` → 同上
20. `bool QuestInventory != null` → 同上
21. `uint32 Gold`、`uint32 Credit`
22. `bool HasExpandedStorage`、`bool HasStoragePassword`、`bool RequireStoragePassword`
23. `int64 StoragePasswordLastSet`(ToBinary)、`int64 ExpandedStorageExpiryTime`(ToBinary)
24. `int32 Magics.Count` + `ClientMagic[]`
25. `int32 IntelligentCreatures.Count` + `ClientIntelligentCreature[]`
26. `byte SummonedCreatureType`、`bool CreatureSummoned`、`bool AllowObserve`、`bool Observer`

**关于 `StartInformation`**：**Crystal 没有这个包**（全仓 grep 无 `StartInformation` 类型，`ServerPacketIds` 里也没有对应成员，`Shared/Enums.cs:1434-1714`）。等价序列就是 `StartGame{Result=4}` → `NewItemInfo` → `MapInformation` → `UserInformation` → 周围对象。Rust 侧若沿用“一个 StartInformation 大包”的设计，属于**主动偏离**，客户端协议不兼容。

**`S.StartGame` 的 `Result` 语义**（类定义 `ServerPackets.cs:398-426`，结果注释在 `:408-413`）：

```
0: Disabled.
1: Not logged in
2: Character not found.
3: Start Game Error
```
—— 但实际代码里 **`4` 表示成功**（`PlayerObject.cs:1133`），注释里**没有写 4**；`3` 在代码中由 `StartGameFailed()` 使用（`:1355`），客户端文案是 `No active map and/or start point found.`（`Language.cs:3051`）。这是 Crystal 里典型的“注释滞后于实现”，重写时应以代码为准并自行定义枚举。

### 3.5 `LogOut`（Client→Server id = 9）与回到选角

`MirConnection.LogOut()`（`:1126-1153`）：

- 请求体为空（`Shared/ClientPackets.cs:241-250`）。
- `Stage == Game`：
  - 若 `Envir.Time < Player.LogTime` → 回 `S.LogOutFailed`（空包，`:2035-2051`）。`LogTime` 在若干动作（攻击/采集等）后被设为 `Envir.Time + Globals.LogDelay`（例如 `HumanObject.cs:2747`），`LogDelay = 10000`（`Shared/Globals.cs:35`）——即**动作后 10 秒内不能登出**。
  - 否则 `Player.StopGame(23)` → `Stage = GameStage.Select` → `Player = null` → `S.LogOutSuccess{ Characters = Account.GetSelectInfo() }`（`:1136-1141`）。reason 23 的日志文案 `"{0} Has logged out. Reason: User returned to select char"`（`Language.cs:1950`）。
- `Stage == Observer`：解除观察关系 → `Stage = Select` → 同样发 `S.LogOutSuccess`（`:1143-1152`）。
- 其他阶段：静默无动作。
- 客户端：`LogOutSuccess` → `ActiveScene = new SelectScene(p.Characters)`（`Client/MirScenes/GameScene.cs:3164-3178`）；`ReturnToLogin` → `new LoginScene()` + 提示“被观察者已登出”（`:3184-3193`）。

---

## 4. 时序图（ASCII）

### 4.1 成功：连接 → 登录 → 选角 → 进世界

```
Client                                                Server
  |                                                      |
  |--- TCP connect ------------------------------------>|  Envir.Connection()
  |                                                      |   IPBlocks?  MaxIP(5)?  MaxUser(50)?
  |                                                      |   new MirConnection(sessionID)
  |<------------------------------- S.Connected (id 0) -|  sendList 首包 (:97)
  |                                                      |
  |--- C.ClientVersion (id 0) -------------------------->|  gate: Stage == None
  |    int32 len + MD5(exe) = 16 bytes                   |  compare vs Settings.VersionHashes
  |<------------------- S.ClientVersion{Result=1} (id 1) |  Stage = Login  (:871)
  |                                                      |
  |--- C.Login (id 5) {AccountID, Password} ------------>|  gate: Stage == Login
  |                                                      |  AllowLogin / regex / exists / ban
  |                                                      |  PBKDF2(pw, salt) == stored ?
  |                                                      |  old conn?.SendDisconnect(1)
  |<--- S.LoginSuccess (id 9) {count, SelectInfo[4]} ----|  Stage = Select (:3919)
  |     (或 S.Login{Result=0..5} / S.LoginBanned)         |
  |                                                      |
  |--- C.StartGame (id 8) {CharacterIndex} ------------->|  gate: Stage == Select
  |                                                      |  AllowStartGame||Admin
  |                                                      |  char exists? char banned?
  |                                                      |  new PlayerObject -> StartGame()
  |<------------------ S.StartGame{Result=4, Resolution}|  Stage = Game (:1131)
  |<-------------------------- S.NewItemInfo × N --------|
  |<-------------------------- S.MapInformation (id 17) -|
  |<-------------------------- S.UserInformation (id 21)|
  |<-------------------------- S.ObjectPlayer / Chat ... |
  |                                                      |
```

### 4.2 失败：口令错误 / 账号被封 / 功能关闭

```
Client                                                Server
  |--- C.Login {id, wrongpw} --------------------------->|
  |                                                      |  WrongPasswordCount++ >= 5 ?
  |<------------------------------- S.Login{Result=4} ---|  否：普通失败 (第1..5次)
  |    客户端: "Incorrect Password and AccountID combo"  |
  |                                                      |
  |--- C.Login {id, wrongpw} (第 6 次) ----------------->|
  |                                                      |  Banned=true, ExpiryDate=Now+2min
  |                                                      |  BanReason="Too many Wrong Login Attempts."
  |<-- S.LoginBanned{Reason, ExpiryDate} ----------------|
  |    客户端: "This account is banned...."               |
  |                                                      |
  |--- C.Login（封禁期内） ------------------------------>|
  |<-- S.LoginBanned{Reason, ExpiryDate} ----------------|  直接回绝，不校验密码
  |                                                      |
  |--- C.Login（AllowLogin=false） ---------------------->|
  |<-- S.Login{Result=0} --------------------------------|
```

补充：版本不匹配时是另一条独立路径 ——

```
  |<-- S.ClientVersion{Result=0} ------------------------|  Disconnecting = true (:855)
  |    (随后连接被 SoftDisconnect(10) 关闭)               |  日志 "..., Disconnnected - Wrong Client Version."
```

### 4.3 新建账号

```
Client (NewAccountDialog)                             Server
  |--- C.NewAccount (id 3) ----------------------------->|  gate: Stage == Login
  |    AccountID, Password, BirthDate, UserName,         |  1 AllowNewAccount?         -> 0
  |    SecretQuestion, SecretAnswer, EMail              |  2 IP 1h 内 >2 次?           -> 0 (+24h IP 封禁)
  |                                                      |  3 AccountIDReg 3-15 alnum  -> 1
  |                                                      |  4 PasswordReg  5-15 alnum  -> 2
  |                                                      |  5 email 格式 / >50         -> 3
  |                                                      |  6 UserName > 20            -> 4
  |                                                      |  7 SecretQuestion > 30      -> 5
  |                                                      |  8 SecretAnswer > 30        -> 6
  |                                                      |  9 AccountExists(忽略大小写) -> 7
  |                                                      | 10 salt=24B, PBKDF2 50 rounds
  |                                                      |    AccountList.Add(..., Index=++NextAccountID)
  |<------------------------------ S.NewAccount{Result=8}|  成功
  |    客户端: "Your account was created successfully."  |
  |    (弹窗关闭，回到登录框；不会自动登录)                 |
```

### 4.4 新建角色

```
Client (SelectScene / NewCharacterDialog)             Server
  |--- C.NewCharacter (id 6) {Name, Gender, Class} ---->|  gate: Stage == Select
  |                                                      |  AllowNewCharacter?        -> 0
  |                                                      |  IP 1h 内 >4 次?            -> 0 (+24h 封禁)
  |                                                      |  CharacterReg / DisabledChars -> 1
  |                                                      |  Gender ∈ {0,1}            -> 2
  |                                                      |  Class ∈ 0..4 且该类开放     -> 3
  |                                                      |  未删除角色数 >= 4          -> 4
  |                                                      |  CharacterExists（忽略大小写）-> 5
  |                                                      |  new CharacterInfo{Name,Class,Gender}
  |                                                      |     Index=++NextCharacterID
  |                                                      |     Level=0, HP=0, Map=0   ← 注意
  |<-- S.NewCharacterSuccess{SelectInfo} -----------------|  成功（无 Result 字段）
  |                                                      |
  |   （首次 StartGame 时才生成：Level=1 / Hair=rand(0..8) / |
  |     StartItems 按 Class+Gender 过滤 / 绑定=随机 StartPoint）|
```

---

## 5. 服务端状态机

### 5.1 枚举定义（原文）

```csharp
public enum GameStage { None, Login, Select, Game, Observer, Disconnected }   // Server/MirNetwork/MirConnection.cs:13
```

Crystal 只有这一个阶段枚举（没有 `SessionStage` / `ConnectionStage` 之类的分层）；它同时承担“协议阶段”与“会话状态”两种角色，字段是 `public GameStage Stage;`（`MirConnection.cs:30`）。

### 5.2 迁移图

```
        accept
          │
          ▼
     ┌──────────┐   S.ClientVersion{Result=1}
     │   None   │──────────────────────────────┐
     └──────────┘                              ▼
                                        ┌──────────────┐
          CleanObservers()  ───────────► │    Login     │
          （被观察者下线）                 └──────────────┘
                                                │  S.LoginSuccess
                                                ▼
                                        ┌──────────────┐
          LogOut (Game → Select) ──────► │    Select    │
          LogOut (Observer → Select) ──► └──────────────┘
                                                │  S.StartGame{Result=4}
                                                ▼
                                        ┌──────────────┐
                                        │     Game     │
                                        └──────────────┘
                                                │  Observe（成功）
                                                ▼
                                        ┌──────────────┐
                                        │   Observer   │
                                        └──────────────┘

  任意状态 ── Disconnect() / SoftDisconnect() / SendDisconnect() ──► Disconnected
  （版本不匹配走 SoftDisconnect(10)，超时走 Disconnect(21)）
```

精确的迁移点（全部列举，无遗漏）：

| 从 | 到 | 触发 | 代码 |
| --- | --- | --- | --- |
| `None` | `Login` | 版本校验通过 / `CheckVersion == false` | `MirConnection.cs:871` |
| `Login` | `Select` | `Envir.Login` 全部校验通过 | `Envir.cs:3919` |
| `Select` | `Game` | `StartGameSuccess()` 第一行 | `PlayerObject.cs:1131` |
| `Game` | `Select` | `LogOut()`（`Player.StopGame(23)` 之后） | `MirConnection.cs:1138` |
| `Observer` | `Select` | `LogOut()`（解除观察后） | `MirConnection.cs:1149` |
| 任意 | `Observer` | `AddObserver()`（被观察者身上挂载观察者） | `MirConnection.cs:112` |
| `Observer` | `Login` | `CleanObservers()`（被观察者下线/被清理） | `MirConnection.cs:831` |
| 任意 | `Disconnected` | `Disconnect()` / `SoftDisconnect()` / `SendDisconnect()` | `:762`、`:781`、`:825` |

注意 `Observer → Game` **不存在**：观察者要进游戏必须先 `LogOut` 回 `Select` 再 `StartGame`。

### 5.3 阶段 × 合法包（服务端视角）

见 1.4 的表格。再补充两条实现层面的约束：

- 分发表里 `case (short)ClientPacketIds.X:` 后面直接调 handler（`MirConnection.cs:290-757`），gate 在 handler 内；**因此“非法阶段 + 合法 id”不会断开连接，也不会有回包**（`Disconnect`/`KeepAlive`/三个 storage 包例外）。
- `LogOut` 在 `Select`/`Login`/`None` 阶段是**空操作**（`LogOut()` 内部只处理 `Game`/`Observer`），客户端不会得到任何回应。

### 5.4 连接关闭的 reason 编码（`S.Disconnect.Reason`）

包定义注释只写了 0–3（`Shared/ServerPackets.cs:71-76`：`0: Server Closing.`、`1: Another User.`、`2: Packet Error.`、`3: Server Crashed.`），但代码实际使用范围更广；服务端日志映射见 `PlayerObject.LogOutReason`（`Server/MirObjects/PlayerObject.cs:433-466`）：

| Reason | 触发点 | 日志文案（原文，`Shared/Language.cs:1939-1951`） |
| --- | --- | --- |
| `0` | 服务器关闭 `StopNetwork()` → `SendDisconnect(0)`（`Envir.cs:3462`） | `"{0} Has logged out. Reason: Server closed"`（:1939） |
| `1` | 重复登录（顶号）`account.Connection?.SendDisconnect(1)`（`Envir.cs:3913`） | `"{0} Has logged out. Reason: Double login"`（:1940） |
| `2` | 聊天超长 `SendDisconnect(2)`（`MirConnection.cs:1187`） | `"{0} Has logged out. Reason: Chat message too long"`（:1941） |
| `3` | 崩溃路径 | `"{0} Has logged out. Reason: Server crashed"`（:1942） |
| `4` | 管理员踢人 | `"{0} Has logged out. Reason: Kicked by admin"`（:1943） |
| `5` | 连接数上限 | `"{0} Has logged out. Reason: Maximum connections reached"`（:1944） |
| `6` | 账号封禁 | `"{0} Has logged out. Reason: Account has been Banned!"`（:1945） |
| `10` | 客户端版本错误 `SoftDisconnect(10)`（`MirConnection.cs:862`） | `"{0} Has logged out. Reason: Wrong client version"`（:1946） |
| `20` | （默认/未知）`Disconnect(20)`（`:246`，socket 已断） | `"{0} Has logged out. Reason: User gone missing / disconnected"`（:1947） |
| `21` | 收包超时 `Disconnect(21)`（`:269`） | `"{0} Has logged out. Reason: Connection timed out"`（:1948） |
| `22` | 客户端主动断开 / 收到 `C.Disconnect`（`:296`） | `"{0} Has logged out. Reason: User closed game"`（:1949） |
| `23` | 回选角 `LogOut`（`:1136`） | `"{0} Has logged out. Reason: User returned to select char"`（:1950） |
| `24` | 观察者相关 | `"{0} Has logged out. Reason: Began observing"`（:1951） |

> `SendDisconnect(reason)` 是**先发包再关连接**（`MirConnection.cs:809-826`）；`Disconnect(reason)` **不发包**（`:776-808`）。只有前者会让客户端看到 reason。

---

## 6. 枚举与常量总表

### 6.1 `GMOptions`（`[Flags] byte`，`Shared/Enums.cs:96-103`）

```csharp
[Flags]
public enum GMOptions : byte
{
    None = 0,
    GameMaster = 0x0001,
    Observer = 0x0002,
    Superman = 0x0004
}
```

| 值 | 成员 | 含义 |
| --- | --- | --- |
| 0 | `None` | 无 GM 标记 |
| 1 | `GameMaster` | 隐身/管理员形态（`GMGameMaster`） |
| 2 | `Observer` | 观察者形态（`Observer`） |
| 4 | `Superman` | 无敌（`GMNeverDie`） |

用途：仅 GM 在进世界时通过 `BuffType.GameMaster` 的 `values[0]` 下发（`Server/MirObjects/HumanObject.cs:833-844`：`AddBuff(BuffType.GameMaster, this, 0, null, false, values: (byte)options)`），客户端在 buff 面板里按位解析（`Client/MirScenes/Dialogs/BuffDialog.cs:282-286`）。**它不参与登录/选角判定**，只作为“进世界后第一屏”的一部分出现。

### 6.2 `GameStage`（`MirConnection.cs:13`）

| 值 | 成员 | 含义 |
| --- | --- | --- |
| 0 | `None` | 已连接，未校验版本 |
| 1 | `Login` | 版本已过，可登录/建号/改密 |
| 2 | `Select` | 已登录，可建/删角色、进游戏 |
| 3 | `Game` | 已进世界，全部玩法包可用 |
| 4 | `Observer` | 观察他人（仅 `Chat`/`Inspect`/`Observe`/`GetRanking`/`LogOut`） |
| 5 | `Disconnected` | 已断开（对象即将回收） |

### 6.3 `MirGender` / `MirClass`（`Shared/Enums.cs:818-831`）

| 值 | `MirGender` | | 值 | `MirClass` |
| --- | --- | --- | --- | --- |
| 0 | `Male` | | 0 | `Warrior` |
| 1 | `Female` | | 1 | `Wizard` |
| | | | 2 | `Taoist` |
| | | | 3 | `Assassin` |
| | | | 4 | `Archer` |

（`MirClass` 只有这 5 个值；`NewCharacter` 的校验正是“必须落在这个集合内”，`Envir.cs:4027-4032`。）

### 6.4 各“Result”码（Crystal 没有命名枚举，只有包内注释）

> 说明：Crystal **不存在** `LoginResult` / `StartGameResult` / `NewCharacterResult` / `NewAccountResult` / `ChangePasswordResult` / `DeleteCharacterResult` 这些类型（全仓 grep 无匹配）。它们是 `byte Result` 字段 + 注释；下表把注释原文与代码实际用法合并。

**`S.ClientVersion.Result`**（`ServerPackets.cs:39-61`）

| 值 | 注释原文 | 客户端行为 |
| --- | --- | --- |
| 0 | `Wrong Version` | 弹窗 + 断开（`LoginScene.cs:157-161`） |
| 1 | `Correct Version` | 显示登录框 |

**`S.NewAccount.Result`**（`ServerPackets.cs:88-118`）

| 值 | 注释原文 |
| --- | --- |
| 0 | `Disabled`（也用于 IP 建号超限） |
| 1 | `Bad AccountID` |
| 2 | `Bad Password` |
| 3 | `Bad Email` |
| 4 | `Bad Name`（指 `UserName`） |
| 5 | `Bad Question` |
| 6 | `Bad Answer` |
| 7 | `Account Exists.` |
| 8 | `Success` |

**`S.ChangePassword.Result`**（`ServerPackets.cs:119-146`）

| 值 | 注释原文 |
| --- | --- |
| 0 | `Disabled` |
| 1 | `Bad AccountID` |
| 2 | `Bad Current Password` |
| 3 | `Bad New Password` |
| 4 | `Account Not Exist` |
| 5 | `Wrong Password` |
| 6 | `Success` |

**`S.Login.Result`**（`ServerPackets.cs:234-259`）

| 值 | 注释原文 |
| --- | --- |
| 0 | `Disabled` |
| 1 | `Bad AccountID` |
| 2 | `Bad Password` |
| 3 | `Account Not Exist` |
| 4 | `Wrong Password` |
| 5 | **代码新增、注释未写**：`RequirePasswordChange`（强制改密） |

**`S.NewCharacter.Result`**（`ServerPackets.cs:307-335`；成功走另一个包 `NewCharacterSuccess`，无 Result）

| 值 | 注释原文 |
| --- | --- |
| 0 | `Disabled.`（也用于 IP 建角超限） |
| 1 | `Bad Character Name` |
| 2 | `Bad Gender` |
| 3 | `Bad Class` |
| 4 | `Max Characters` |
| 5 | `Character Exists.` |
| 10 | `Success` ← **注释写 10，但代码从不发 10**；成功一律发 `S.NewCharacterSuccess`（`Envir.cs:4067`） |

**`S.DeleteCharacter.Result`**（`ServerPackets.cs:355-397`；成功走 `DeleteCharacterSuccess{CharacterIndex}`）

| 值 | 注释原文 |
| --- | --- |
| 0 | `Disabled.` |
| 1 | `Character Not Found` |

**`S.StartGame.Result`**（`ServerPackets.cs:398-426`；成功值为 `4`，注释缺）

| 值 | 注释原文 / 实际含义 |
| --- | --- |
| 0 | `Disabled.`（`AllowStartGame=false` 且非 GM） |
| 1 | `Not logged in` |
| 2 | `Character not found.` |
| 3 | `Start Game Error`（实际 = 地图/出生点都无效，`PlayerObject.cs:1355`） |
| 4 | **（注释未写）成功**；附带 `int32 Resolution = Settings.AllowedResolution`（`PlayerObject.cs:1133`） |

> `Resolution` 字段在**任何** Result 下都会被序列化（`WritePacket` 无条件写 `Result` + `Resolution`，`ServerPackets.cs:421-425`）；失败分支不赋值，线上就是 `0`。Rust 侧不要把它做成“仅在成功时出现”的可选字段。

**`S.StartGameBanned` / `S.LoginBanned` / `S.ChangePasswordBanned`**：没有 Result，只有 `string Reason` + `int64 ExpiryDate`。

**`S.StorageUnlockResult.Result`**（`ServerPackets.cs:169-197`，仅 Game 阶段相关，登录链路只用它的失败分支）

| 值 | 注释原文 |
| --- | --- |
| 0 | `Success` |
| 1 | `Bad Password` |
| 2 | `Wrong Password` |
| 3 | `Not Available`（阶段/距离不对时使用，`MirConnection.cs:898`） |
| 4 | `No Password Set` |

**`S.StoragePasswordResult.Result`**（`ServerPackets.cs:198-233`）

| 值 | 注释原文 |
| --- | --- |
| 0 | `Not Available` |
| 1 | `Bad Current Password` |
| 2 | `Wrong Current Password` |
| 3 | `Bad New Password` |
| 4 | `Success` |
| 5 | `No Password Set` |

### 6.5 登录链路用到的包 ID（双向）

`ServerPacketIds`（`Shared/Enums.cs:1434-1714`，0 起自增、无显式赋值）：`Connected=0`、`ClientVersion=1`、`Disconnect=2`、`KeepAlive=3`、`NewAccount=4`、`ChangePassword=5`、`ChangePasswordBanned=6`、`Login=7`、`LoginBanned=8`、`LoginSuccess=9`、`NewCharacter=10`、`NewCharacterSuccess=11`、`DeleteCharacter=12`、`DeleteCharacterSuccess=13`、`StartGame=14`、`StartGameBanned=15`、`StartGameDelay=16`、`MapInformation=17`、`NewMapInfo=18`、`WorldMapSetup=19`、`SearchMapResult=20`、`UserInformation=21`、`LogOutSuccess=58`、`LogOutFailed=59`、`ReturnToLogin=60`、`StorageUnlockResult=277`、`StoragePasswordResult=278`。

`ClientPacketIds`（`Shared/Enums.cs:1717-1878`）：`ClientVersion=0`、`Disconnect=1`、`KeepAlive=2`、`NewAccount=3`、`ChangePassword=4`、`Login=5`、`NewCharacter=6`、`DeleteCharacter=7`、`StartGame=8`、`LogOut=9`，以及 `UnlockStorage=150`、`SetStoragePassword=151`、`RemoveStoragePassword=152`（这三个是**追加在枚举末尾**的，别按名字顺序推断）。

> 两个方向的编号空间独立：`0` 在 Server→Client 是 `Connected`，在 Client→Server 是 `ClientVersion`。

### 6.6 与登录相关的常量与默认配置

| 项 | 值 | 位置 |
| --- | --- | --- |
| `Globals.MinAccountIDLength` / `MaxAccountIDLength` | 3 / 15 | `Shared/Globals.cs:7-8` |
| `Globals.MinPasswordLength` / `MaxPasswordLength` | 5 / 15 | `Shared/Globals.cs:10-11` |
| `Globals.MinCharacterNameLength` / `MaxCharacterNameLength` | 3 / 15 | `Shared/Globals.cs:13-14` |
| `Globals.MaxCharacterCount` | 4 | `Shared/Globals.cs:15` |
| `Globals.MaxChatLength` | 80 | `Shared/Globals.cs:17` |
| `Globals.LogDelay` | 10000 ms | `Shared/Globals.cs:35` |
| `Crypto.SaltSize` / `HashSize` / `Iterations` | 24 / 24 / 50 | `Server/Utils/Crypto.cs:8-10` |
| `Settings.Port` / `TimeOut` / `MaxUser` / `MaxIP` / `MaxPacket` | 7000 / 10000 ms / 50 / 5 / 50 | `Server/Settings.cs:66-71` |
| `Settings.IPBlockSeconds` | 5 | `Server/Settings.cs:73` |
| `Settings.VersionPath` / `CheckVersion` | `.\Mir2.Exe` / `true` | `Server/Settings.cs:42-43` |
| `Settings.AllowNewAccount` / `AllowChangePassword` / `AllowLogin` / `AllowNewCharacter` / `AllowDeleteCharacter` | 全部 `true` | `Server/Settings.cs:81-86` |
| `Settings.AllowStartGame` | **`false`**（只有 GM 能进） | `Server/Settings.cs:86` |
| `Settings.AllowCreateAssassin` / `AllowCreateArcher` | `true` / `true` | `Server/Settings.cs:87-88` |
| `Settings.AllowedResolution` | 1024（INI 键 `[Permission] MaxResolution`） | `Server/Settings.cs:91`、`:414` |
| `Settings.RelogDelay` | 50（**已无实际作用**，重登延迟逻辑被注释） | `Server/Settings.cs:69`；`MirConnection.cs:1116-1120` |
| `Settings.ArchiveDeletedCharacterAfterMonths` | 1 | `Server/Settings.cs:341` |
| `Settings.ArchiveInactiveCharacterAfterMonths` | 12 | `Server/Settings.cs:340` |
| 状态端口 | 3000（硬编码，非 INI） | `Server/MirEnvir/Envir.cs:3430` |
| 状态端口内容 | 每 10 秒发一次 `"c;/NoName/{PlayerCount}/CrystalM2/{Version}//;"`（ASCII） | `Server/MirNetwork/MirStatusConnection.cs:99-106` |

---

## 7. 给 Rust 重写的落地清单（按依赖顺序）

1. **帧层**：`u16 length（含头，小端） + i16 id + payload`；`length < 4` 等数据、`length > 已收字节` 等数据、未知 id 清缓冲、解析失败按“非法包”处理（24h IP 封禁）。见 1.2。
2. **连接层**：accept 时先查 IPBlocks、再查同 IP 连接数（默认 5），成功后**写入 5 秒 IP 封禁**（复刻这个反直觉顺序）；连接建立立刻发 `Connected`。
3. **版本层**：从 `Configs/Setup.ini` 的 `[General] VersionPath`（可逗号分隔多文件）加载，算 MD5（16 字节），与会话里的 16 字节逐字节比较。注意“全部文件缺失 ⇒ 全拒”。
4. **账号层**：PBKDF2-HMAC-SHA1 / 24B salt / 50 次 / 24B 输出，**再按有损 UTF-8 转字符串比较**；`WrongPasswordCount` 用“自增前的值 >= 5”判定（第 6 次封禁 2 分钟），成功登录才清零；重复登录=踢旧连接（reason 1）。
5. **角色层**：创建只落 Name/Class/Gender/CreatedIP/CreatedDate，`Level` 留 0；**首次进世界**才 `Level=1`、随机发型 0–8、按 DB 的 `StartItem` 标记发初始装备（再按 `RequiredClass`/`RequiredGender` 过滤）、在带 `StartPoint` 的安全区里随机取一作为绑定/出生点。
6. **进世界**：先发 `StartGame{Result=4, Resolution}` 完成 `Select → Game` 迁移，再依次 `NewItemInfo* → MapInformation → UserInformation → 周围对象 → 其余初始化包`。不要把 `MapInformation`/`UserInformation` 合并成“StartInformation”，客户端没有这个 id。
7. **删除角色**：立即软删除（`Deleted/DeleteDate`），没有延迟与确认令牌；按月的归档清理发生在账号加载时。
