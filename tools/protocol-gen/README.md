# 协议生成器（protocol-gen）

把 Crystal 的 C# 协议实现翻译成 Rust。输入是 Crystal 源码里的
`Shared/Enums.cs`、`Shared/ClientPackets.cs`、`Shared/ServerPackets.cs`，
输出是 `crates/mir-core/src/enums/generated.rs` 与
`crates/mir-protocol/src/packets/{client,server}.rs`。

## 用法

```bash
# 需要一份 Crystal 检出（默认 C:/Users/diana/ref/Crystal）
python tools/protocol-gen/generate.py --crystal /path/to/Crystal

# 只校验「仓库里的生成结果与重新生成的一致」（CI 用）
python tools/protocol-gen/generate.py --crystal /path/to/Crystal --check
```

生成器会先跑 `rustfmt`，所以 `--check` 与 `cargo fmt --check` 结果一致。

## 工作方式

1. **枚举**：解析 `enum X : byte { A = 1, … }`，保留底层类型；`[Flags]` 属性会被记录。
2. **包结构**：按 `public (sealed) class X : Packet` 切分，取出
   `Index`、`Compressed`、字段声明与 `ReadPacket`/`WritePacket` 的方法体。
3. **双向翻译**：读侧翻译成 `decode`，写侧翻译成 `encode`，两边各自推导出一条
   **线格式签名**（按顺序的一串 `prim:u32` / `string` / `optional:struct:UserItem` …）。
   只有两条签名**完全一致**才会产出代码。
4. **字段覆盖检查**：每个声明的字段都必须出现在读侧翻译结果里
   （Crystal 自己「声明了但读写都漏掉」的字段除外），否则这个包会被登记为待手写。
   这条检查是因为真的出现过 `ManageHeroes.Heroes` 被静默丢弃的缺陷。
5. **兜底**：生成不了的包写进 [`report.md`](report.md)，由
   `crates/mir-protocol/src/manual/` 里的手写实现补上；`generate.py` 里的
   `HAND_WRITTEN` 表负责把它们 `pub use` 进包枚举，保证编号表完整。

## 支持的语句形态

读侧 / 写侧各自支持（未覆盖的会在 `report.md` 里给出原因）：

| 形态 | 例子 |
| --- | --- |
| 原语读写 | `X = reader.ReadInt32();` / `writer.Write(X);` |
| 枚举（含强转） | `X = (MirGridType)reader.ReadByte();` / `writer.Write((byte)X);` |
| 字符串 | `ReadString()` / `Write(string)`（7 位长度前缀 + UTF-8） |
| 定长/计数数组 | `X = new UserItem[reader.ReadInt32()];` + 填充循环 |
| 列表 | `for (…) X.Add(new T(reader));` / `for (…) X[i].Save(writer);` |
| 可选单值 | `if (reader.ReadBoolean()) X = new T(reader);` / `writer.Write(X != null); if (X != null) X.Save(writer);` |
| 可选数组 | `if (reader.ReadBoolean()) { count…; X[i] = … }` / `writer.Write(Xs != null); if (Xs != null) { … }` |
| 复合值 | `new Point(x, y)`、`Color.FromArgb(i)`、`DateTime.FromBinary(i64)` |
| 局部计数变量 | `int count = …;` 与复用写法 `count = …;`（`GuildBuffList`） |

## 修改生成器时的自检

```bash
python tools/protocol-gen/generate.py          # 重新生成
cargo test -p mir-protocol                     # 含「所有包默认值 encode→decode」往返测试
cargo clippy -p mir-protocol --all-targets -- -D warnings
cargo fmt --all --check
```

往返测试会遍历生成出来的每个包（含 gzip 的 `NPCGoods`），任何「读写不对称」
都会当场失败——比逐个肉眼核对字段顺序可靠得多。
