// 品牌导入精确名称查重索引。默认检查；APPLY_BRAND_IMPORT_INDEX=1 时创建。
// 不修改品牌数据。回滚代码时保留索引；确认无导入流量后可手工删除该索引。
(() => {
    const collection = db.getCollection("product_brands");
    const name = "idx_product_brands_name";
    const key = { name: 1, deleted_at: 1 };
    const existing = collection.getIndexes().find(index => index.name === name);
    if (existing && (JSON.stringify(existing.key) !== JSON.stringify(key) || existing.unique)) {
        throw new Error(`索引 ${name} 与合同不符，停止迁移`);
    }
    if (process.env.APPLY_BRAND_IMPORT_INDEX === "1" && !existing) {
        collection.createIndex(key, { name });
        printjson({ status: "created", name, key });
        return;
    }
    printjson({ status: existing ? "ready" : "pending", name, key });
})();
