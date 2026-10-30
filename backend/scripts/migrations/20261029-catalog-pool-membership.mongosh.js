// 目录池成员索引迁移。缺省只读核对，不建立集合、不改写来源或扫描证据。
// 应用：ERP_MEMBERSHIP_APPLY=1 mongosh "$ERP_MONGO_URI" --file scripts/migrations/20261029-catalog-pool-membership.mongosh.js
// 回退：先停止目录采集与成员核对，确认 active 扫描为零，再以 ERP_MEMBERSHIP_ROLLBACK=1 执行本文件。
// 回退只移除本文件索引；扫描证据、成员历史、目录身份与已应用业务资料必须完整保留。
// 已写入完整范围及成员状态后，只能部署能继续识别这些证据的恢复执行器；不得清空集合模拟旧版本。
const membershipIndexes = [
    ["supplier_catalog_membership_scans", [
        [{id: 1}, {name: "uk_catalog_membership_scan_id", unique: true}],
        [{connection_id: 1, "scope.account_scope": 1}, {
            name: "uk_catalog_membership_active_account", unique: true,
            partialFilterExpression: {active: true, deleted_at: 0},
        }],
        [{connection_id: 1, supplier_id: 1, "scope.account_scope": 1, state: 1, completed_at: -1, id: -1}, {name: "idx_catalog_membership_baseline"}],
        [{connection_id: 1, "scope.account_scope": 1, id: 1}, {name: "idx_catalog_membership_namespace"}],
    ]],
    ["supplier_catalog_pool_members", [
        [{id: 1}, {name: "uk_catalog_pool_member_id", unique: true}],
        [{connection_id: 1, account_scope: 1, pool_id: 1, "sku.spec_id": 1}, {name: "uk_catalog_pool_member_source", unique: true}],
        [{connection_id: 1, supplier_id: 1, account_scope: 1, active: 1, id: 1}, {name: "idx_catalog_pool_member_reconcile"}],
        [{connection_id: 1, supplier_id: 1, account_scope: 1, "sku.spec_id": 1, last_seen_run: 1, pool_id: 1, active: 1}, {name: "idx_catalog_pool_member_current"}],
    ]],
    ["supplier_catalog_items", [
        [{connection_id: 1, spec_id: 1, deleted_at: 1, id: 1}, {name: "idx_supplier_catalog_spec_membership"}],
    ]],
];

const applyMembership = process.env.ERP_MEMBERSHIP_APPLY === "1";
const rollbackMembership = process.env.ERP_MEMBERSHIP_ROLLBACK === "1";
if (applyMembership && rollbackMembership) throw new Error("应用与回退模式不能同时开启");

function exists(name) {
    return db.getCollectionInfos({name}).length === 1;
}

function duplicateCount(name, keys, filter = {}) {
    if (!exists(name)) return 0;
    const identity = Object.fromEntries(keys.map((key) => [key.replaceAll(".", "_"), `$${key}`]));
    return db.getCollection(name).aggregate([
        {$match: filter}, {$group: {_id: identity, count: {$sum: 1}}},
        {$match: {count: {$gt: 1}}}, {$limit: 20}, {$count: "count"},
    ]).toArray()[0]?.count ?? 0;
}

const duplicateFindings = [
    duplicateCount("supplier_catalog_membership_scans", ["id"]),
    duplicateCount("supplier_catalog_membership_scans", ["connection_id", "scope.account_scope"], {active: true, deleted_at: 0}),
    duplicateCount("supplier_catalog_pool_members", ["id"]),
    duplicateCount("supplier_catalog_pool_members", ["connection_id", "account_scope", "pool_id", "sku.spec_id"]),
];
const activeMembershipScans = exists("supplier_catalog_membership_scans")
    ? db.getCollection("supplier_catalog_membership_scans").countDocuments({active: true, deleted_at: 0}) : 0;
const namespaceConflicts = exists("supplier_catalog_membership_scans")
    ? db.getCollection("supplier_catalog_membership_scans").aggregate([
        {$match: {deleted_at: 0}},
        {$group: {_id: "$connection_id", accounts: {$addToSet: "$scope.account_scope"}}},
        {$match: {$expr: {$gt: [{$size: "$accounts"}, 1]}}}, {$limit: 20}, {$count: "count"},
    ]).toArray()[0]?.count ?? 0 : 0;

printjson({mode: applyMembership ? "apply" : rollbackMembership ? "rollback" : "read_only", duplicateGroupsUpTo20: duplicateFindings, namespaceConflictsUpTo20: namespaceConflicts, activeMembershipScans});
if (duplicateFindings.some((count) => count !== 0) || namespaceConflicts !== 0) {
    throw new Error("目录池身份或账号命名空间存在冲突，须先完成来源核对，不得自动合并或删除");
}
if (!applyMembership && !rollbackMembership) {
    print("只读核对完成，未修改任何集合或索引。确认结果后显式开启应用模式。");
} else if (rollbackMembership && activeMembershipScans !== 0) {
    throw new Error("仍有活动扫描，必须先保留证据并完成恢复或明确中止，禁止移除围栏索引");
} else {
    for (const [name, indexes] of membershipIndexes) {
        if (rollbackMembership) {
            if (!exists(name)) continue;
            const collection = db.getCollection(name);
            const existing = new Set(collection.getIndexes().map((index) => index.name));
            for (const [, options] of indexes) if (existing.has(options.name)) collection.dropIndex(options.name);
        } else {
            for (const [keys, options] of indexes) db.getCollection(name).createIndex(keys, options);
        }
    }
}
