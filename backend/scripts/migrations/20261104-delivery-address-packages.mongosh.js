// 增量字段无需回填；此迁移执行只读形状核对，禁止打印或推断历史收货资料。
// mongosh "$ERP_MONGO_URI" --file scripts/migrations/20261104-delivery-address-packages.mongosh.js
// ERP_MIGRATION_ROLLBACK_CHECK=1 可执行旧版本回退前置核对；不会删除任何字段。
const deliveries = db.getCollection("deliveries");
const packed = {packing: {$type: "object"}};
const snapshot = {packing_addresses_encrypted: {$type: "string"}};
const invalid = deliveries.countDocuments({
    ...packed,
    $or: [
        {packing_addresses_encrypted: {$not: {$type: "string"}}},
        {packing_addresses_encrypted: ""},
        {"packing.dispatch": {$not: {$type: "array"}}},
        {"packing.packages": {$not: {$type: "array"}}},
    ],
});
const packedCount = deliveries.countDocuments(packed);
const snapshotCount = deliveries.countDocuments(snapshot);
printjson({mode: "read_only", packedCount, snapshotCount, invalid});
if (invalid) throw new Error("存在包裹与快照不完整记录，停止上线并核实原记录");
if (process.env.ERP_MIGRATION_ROLLBACK_CHECK === "1" && (packedCount || snapshotCount)) {
    throw new Error("已有地址包裹或剩余草稿，禁止回退旧写入端；保留事实并前向修复");
}
