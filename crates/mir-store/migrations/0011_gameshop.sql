-- 0011_gameshop.sql — 商城（GameShop）的库存与购买计数持久化。
--
-- 对应 Crystal 的两份运行期计数：
--
-- | Crystal | 本迁移 | 说明 |
-- |---|---|---|
-- | `Envir.GameshopLog`（`Dictionary<int,int>`，`Server/MirEnvir/Envir.cs:133`） | `gameshop_stock` | **全服**已购数量，按商品 `GIndex` 记；存档 `:2598-2599`、读档 `:3054`，`ResetGS` 时 `ClearGameshopLog()`（`:3316`、`:5039-5041`）。对应 `GameShopItem.iStock == false`（「全服库存」）的分支（`PlayerObject.GameShopStock`，`PlayerObject.cs:13712-13715`） |
-- | `CharacterInfo.GSpurchases`（`Dictionary<int,int>`，按商品 `GIndex` 记） | `gameshop_purchases` | **按角色**已购数量，随角色存档/读档。对应 `GameShopItem.iStock == true`（「个人库存」）的分支（`PlayerObject.cs:13708-13711`、`:13838-13849`） |
--
-- 剩余库存 = `GameShopItem.Stock` − 对应计数（`PlayerObject.cs:13717`、`:13770`）；
-- `Stock == 0` 表示不限量（`PlayerObject.cs:13782-13785`），不写任何计数行。
--
-- ## 元宝（Crystal `AccountInfo.Credit` / 客户端里的 GameGold）
--
-- **本迁移不新增元宝列**：迁移 `0001_init.sql` 的 `characters.credit`（注释在 `:84`，
-- `gold` 在 `:85`、`credit` 在 `:86`）就是本作的元宝余额，与金币 `characters.gold`
-- 放在同一行。读写走已有的 `save_character`（`crates/mir-store/src/repo.rs`）与商城事务
-- （`crates/mir-server/src/store.rs` 的 `GameShopUpdate`），客户端通过
-- `S.UserInformation.credit` 显示、购买时用 `S.LoseCredit{Credit}` 扣减
-- （Crystal `PlayerObject.GameshopBuy`，`PlayerObject.cs:13791-13836`）。
--
-- 与 Crystal 的**差异**：Crystal 把元宝挂在**账号**上（`AccountInfo.Credit`，
-- 同账号多角色共享一个余额）；本作第 1 轮已按任务要求把金币/点数/仓库都放在角色行
-- （见 `crates/mir-store/src/models.rs` 的 `Character::credit` 与
-- `crates/mir-store/src/lib.rs` 的「取舍」一节），元宝沿用同一口径，
-- 即**角色级**元宝。要改回账号级需要同时改客户端显示的 `UserInformation.credit`、
-- 任务奖励（`PlayerObject.GainCredit`）与金币/仓库，属于跨层改动，不在本迁移范围。
--
-- ## 键口径（有意修正 Crystal 的自相矛盾）
--
-- Crystal 读已购计数用的是 **`GameShopItem.Info.Index`（物品模板 ID）**
-- （`PlayerObject.cs:13710`、`:13714`、`:13763`、`:13767`、`:13926`、`:13930`），
-- 写回却用 **`Product.GIndex`（商品 ID）**（`:13843-13858`）——读写的键不是同一个，
-- 结果是「库存校验基本恒等于没买过」。本作**统一按 `GIndex`** 存/读
-- （`gameshop_stock.g_index`、`gameshop_purchases.g_index`），属于有意修正；
--
-- ## 一致性
--
-- 两张计数表只在**商城购买事务**里写（`apply_gameshop_update`：角色行 + 元宝 +
-- 邮件 + 计数同一事务），与第 1 轮市场落盘（`apply_market_update`）同一风格；
-- 校验读取（`load_gameshop_stock` / `load_gameshop_purchases`）发生在扣款之前，
-- 会话层用进程级锁把「读计数 → 校验/扣款/发货 → 落盘」串起来，避免同一进程内
-- 两个会话读到同一份旧计数而超卖。

CREATE TABLE gameshop_stock (
    -- Crystal Envir.GameshopLog 的键：GameShopItem.GIndex（不是物品模板 Index）。
    g_index     INTEGER     PRIMARY KEY,
    -- 全服已购数量（Crystal GameshopLog[gIndex]）。
    purchased   BIGINT      NOT NULL DEFAULT 0,
    updated_at  TIMESTAMPTZ NOT NULL DEFAULT now(),

    CONSTRAINT gameshop_stock_purchased_non_negative CHECK (purchased >= 0)
);

-- 按角色记的已购数量（Crystal CharacterInfo.GSpurchases）。
CREATE TABLE gameshop_purchases (
    -- 购买者角色；角色硬删除时级联清理（与 market_auctions 的卖家外键同口径）。
    character_id BIGINT      NOT NULL REFERENCES characters (id) ON DELETE CASCADE,
    -- GameShopItem.GIndex。
    g_index      INTEGER     NOT NULL,
    -- 该角色已购数量（Crystal GSpurchases[gIndex]）。
    quantity     BIGINT      NOT NULL DEFAULT 0,
    updated_at   TIMESTAMPTZ NOT NULL DEFAULT now(),

    PRIMARY KEY (character_id, g_index),
    CONSTRAINT gameshop_purchases_quantity_non_negative CHECK (quantity >= 0)
);

-- 按角色查「我买过多少」（商城面板的剩余库存 / 购买校验）。
CREATE INDEX gameshop_purchases_character_idx ON gameshop_purchases (character_id);
