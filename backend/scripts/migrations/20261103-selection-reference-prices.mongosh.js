// 选品册历史参考价可选回填；升级不依赖本脚本。必须指定单册，禁止全库改写。
// 核对：ERP_SELECTION_BOOK_ID=<册ID> mongosh "$ERP_MONGO_URI" --file scripts/migrations/20261103-selection-reference-prices.mongosh.js
// 应用：备份该册商品池与陈列、停止 API 与 worker 后，在上述命令增加 ERP_SELECTION_PRICE_MODE=apply。
// 回滚：停止进程，从迁移前备份恢复该册 reference_prices 字段；不得修改客户售价或删除新版准备时生成的快照。
const bookletId = process.env.ERP_SELECTION_BOOK_ID;
const mode = process.env.ERP_SELECTION_PRICE_MODE || "check";
if (!bookletId) throw new Error("必须指定 ERP_SELECTION_BOOK_ID");
if (!["check", "apply"].includes(mode)) throw new Error("仅支持 check 或 apply；回滚必须使用迁移前备份");

const counts = { candidates: 0, missingRevision: 0, updated: 0 };

function referencePrices(sku) {
    if (!sku || sku.reference_prices != null) return null;
    const revision = db.sku_revisions.findOne({
        id: sku.sku_revision_id, sku_id: sku.sku_id, deleted_at: 0,
    });
    if (!revision) {
        counts.missingRevision += 1;
        return null;
    }
    return {
        dropship_price_gross: revision.sales_visible_price_gross ?? null,
        bulk_price_gross: revision.bulk_price_gross ?? null,
        market_price: revision.market_price ?? null,
    };
}

function backfill(collection, row, snapshots) {
    const values = {};
    const unchanged = [];
    for (const [path, sku] of snapshots) {
        const prices = referencePrices(sku);
        if (prices == null) continue;
        counts.candidates += 1;
        values[`${path}.reference_prices`] = prices;
        unchanged.push({ [`${path}.reference_prices`]: null });
    }
    if (mode !== "apply" || unchanged.length === 0) return;
    const result = collection.updateOne({
        id: row.id, booklet_id: bookletId, version: row.version, $and: unchanged,
    }, { $set: values });
    if (result.matchedCount !== 1) throw new Error("选品册已发生并发修改，停止回填并重新核对");
    counts.updated += result.modifiedCount;
}

for (const row of db.sales_selection_pool_members.find({ booklet_id: bookletId, deleted_at: 0 })) {
    backfill(db.sales_selection_pool_members, row, [["sku", row.sku]]);
}
for (const row of db.sales_selection_display_items.find({ booklet_id: bookletId, deleted_at: 0 })) {
    const snapshots = row.kind?.kind === "SINGLE_SKU"
        ? [["kind.sku", row.kind.sku]]
        : (row.kind?.members || []).map((sku, index) => [`kind.members.${index}`, sku]);
    backfill(db.sales_selection_display_items, row, snapshots);
}
printjson({ mode, ...counts });
