// 核销资金来源索引迁移。默认只读核对，不修改付款、核销或资金流水。
// 核对：mongosh "$ERP_MONGO_URI" --file scripts/migrations/20261029-payment-allocation-funding-source.mongosh.js
// 应用：ERP_MIGRATION_APPLY=1 mongosh "$ERP_MONGO_URI" --file scripts/migrations/20261029-payment-allocation-funding-source.mongosh.js
// 回退：ERP_MIGRATION_APPLY=1 ERP_INDEX_ROLLBACK=1 mongosh "$ERP_MONGO_URI" --file scripts/migrations/20261029-payment-allocation-funding-source.mongosh.js
// 应用前停止新增核销写入，备份索引定义，完成身份及重复核对后按顺序替换索引。
// 已有预存款使用核销时禁止回退旧二进制和索引；必须保留原核销及冲回事实。

const allocations = db.getCollection("payment_allocations");
const bankField = "supplier_payment_id";
const depositField = "supplier_deposit_usage_id";
const bankName = "uk_payment_allocations_bank_seq";
const depositName = "uk_payment_allocations_deposit_seq";
const legacyName = "uk_payment_allocations_payment_seq";
const apply = process.env.ERP_MIGRATION_APPLY === "1";
const rollback = process.env.ERP_INDEX_ROLLBACK === "1";

function stringField(field) {
    return {$eq: [{$type: `$${field}`}, "string"]};
}

function absentField(field) {
    return {$in: [{$type: `$${field}`}, ["missing", "null"]]};
}

function validIdentity(field) {
    const value = {$convert: {input: `$${field}`, to: "string", onNull: "", onError: ""}};
    return {$and: [
        stringField(field), {$ne: [value, ""]},
        {$eq: [value, {$trim: {input: value}}]}, {$lte: [{$strLenBytes: value}, 128]},
    ]};
}

function assertSourceShape() {
    const invalid = allocations.aggregate([
        {$match: {$expr: {$not: [{$or: [
            {$and: [validIdentity(bankField), absentField(depositField)]},
            {$and: [validIdentity(depositField), absentField(bankField)]},
        ]}]}}},
        {$project: {_id: 0, id: 1}},
        {$limit: 1},
    ]).toArray();
    if (invalid.length) throw new Error(`核销资金来源缺失、重复或空白：${invalid[0].id}`);
}

function assertNoDuplicate(field) {
    const duplicate = allocations.aggregate([
        {$match: {[field]: {$type: "string"}}},
        {$group: {_id: {source: `$${field}`, seq: "$allocation_seq"}, count: {$sum: 1}}},
        {$match: {count: {$gt: 1}}},
        {$limit: 1},
    ], {allowDiskUse: true}).toArray();
    if (duplicate.length) throw new Error(`${field} 存在重复核销序号，必须先核实原资金事实`);
}

assertSourceShape();
assertNoDuplicate(bankField);
assertNoDuplicate(depositField);
const hasDeposit = allocations.findOne({[depositField]: {$type: "string"}}, {_id: 0, id: 1}) !== null;
printjson({mode: apply ? (rollback ? "apply_rollback" : "apply") : "read_only", hasDepositAllocations: hasDeposit});

if (apply && rollback) {
    if (hasDeposit) throw new Error("已存在预存款核销，禁止回退仅支持普通付款的索引或应用");
    allocations.createIndex({supplier_payment_id: 1, allocation_seq: 1}, {name: legacyName, unique: true});
    const names = new Set(allocations.getIndexes().map((index) => index.name));
    for (const name of [bankName, depositName]) if (names.has(name)) allocations.dropIndex(name);
} else if (apply) {
    allocations.createIndex({supplier_payment_id: 1, allocation_seq: 1}, {
        name: bankName, unique: true, partialFilterExpression: {supplier_payment_id: {$type: "string"}},
    });
    allocations.createIndex({supplier_deposit_usage_id: 1, allocation_seq: 1}, {
        name: depositName, unique: true, partialFilterExpression: {supplier_deposit_usage_id: {$type: "string"}},
    });
    if (allocations.getIndexes().some((index) => index.name === legacyName)) allocations.dropIndex(legacyName);
}
