// 京东来源字段与执行预算升级核对；此脚本始终只读，不创建索引或回填历史业务事实。
// 在已确认的目标数据库和连接范围执行：
// ERP_JD_CONNECTION_ID=<连接ID> mongosh "$ERP_MONGO_URI" --file scripts/migrations/20261029-jd-vop-source-budget-audit.mongosh.js
// 可选 ERP_JD_AUDIT_MAX_ROWS（默认1000，最大10000）、ERP_JD_AUDIT_BATCH_SIZE（默认100，最大500）。
// source_purchase_terms 缺失按未知兼容读取；禁止补造起购量、税率、报价数量或历史预算。
// 加密执行快照仅报告存在性，不在数据库脚本中解密或从总额反推运费预算。
// 回退必须保留来源、原动作、订单树和资金证据；本脚本没有数据写入可撤销。

const auditConnection = process.env.ERP_JD_CONNECTION_ID?.trim();
if (!auditConnection || auditConnection.length > 256) {
    throw new Error("必须提供已确认的 ERP_JD_CONNECTION_ID；禁止默认扫描全部连接");
}
if (process.env.ERP_JD_AUDIT_APPLY === "1") {
    throw new Error("本升级不允许回填来源事实或历史预算；此入口只执行只读核对");
}

function auditLimit(name, fallback, maximum) {
    const value = process.env[name];
    if (value === undefined) return fallback;
    if (!/^\d+$/.test(value)) throw new Error(`${name} 必须为正整数`);
    const parsed = BigInt(value);
    if (parsed < 1n || parsed > BigInt(maximum)) throw new Error(`${name} 超出有界核对范围`);
    return Number(parsed);
}

const auditMaxRows = auditLimit("ERP_JD_AUDIT_MAX_ROWS", 1000, 10000);
const auditBatchSize = auditLimit("ERP_JD_AUDIT_BATCH_SIZE", 100, 500);

// 定点数只经字符串和BigInt解释，不经浮点转换；支持BSON Decimal128的科学计数形式。
function auditFixed(value) {
    if (typeof value === "number" || value === null || value === undefined) return null;
    const raw = typeof value === "string" ? value : value.toString();
    const match = /^([+-]?)(\d+)(?:\.(\d+))?(?:[eE]([+-]?\d+))?$/.exec(raw);
    if (!match || raw.length > 128) return null;
    const exponent = BigInt(match[4] || "0");
    const shift = 6n + exponent - BigInt((match[3] || "").length);
    if (shift < -64n || shift > 64n) return null;
    let units = BigInt(`${match[2]}${match[3] || ""}`);
    if (shift >= 0n) units *= 10n ** shift;
    else {
        const divisor = 10n ** (-shift);
        if (units % divisor !== 0n) return null;
        units /= divisor;
    }
    return match[1] === "-" ? -units : units;
}

function auditTerms(terms) {
    if (terms === null || terms === undefined) return [];
    if (typeof terms !== "object" || Array.isArray(terms)) return ["SOURCE_TERMS_SHAPE"];
    const failures = [];
    for (const key of ["minimum_quantity", "maximum_quantity", "quantity_multiple"]) {
        if (terms[key] !== null && terms[key] !== undefined) {
            const quantity = auditFixed(terms[key]);
            if (quantity === null || quantity <= 0n) failures.push(key.toUpperCase());
        }
    }
    const minimum = auditFixed(terms.minimum_quantity);
    const maximum = auditFixed(terms.maximum_quantity);
    if (minimum !== null && maximum !== null && maximum < minimum) failures.push("QUANTITY_RANGE");
    if (terms.restrictions !== undefined && !Array.isArray(terms.restrictions)) failures.push("RESTRICTIONS_SHAPE");
    const quote = terms.quote_context;
    if (quote !== null && quote !== undefined) {
        const quantity = auditFixed(quote.quantity);
        if (quantity === null || quantity <= 0n) failures.push("QUOTE_QUANTITY");
        if (quote.tax_rate !== null && quote.tax_rate !== undefined) {
            const rate = auditFixed(quote.tax_rate);
            if (rate === null || rate < 0n || rate > 1000000n) failures.push("QUOTE_TAX_RATE");
        }
        if (quote.address_fingerprint !== null && quote.address_fingerprint !== undefined
            && !/^[a-fA-F0-9]{64}$/.test(quote.address_fingerprint)) failures.push("QUOTE_ADDRESS_SCOPE");
    }
    return failures;
}

function auditCollection(collectionName, filter, projection, inspect) {
    const report = {collection: collectionName, inspected: 0, terms_unknown: 0, invalid: 0, samples: [], complete: true};
    const cursor = db.getCollection(collectionName).find(filter, projection)
        .sort({_id: 1}).batchSize(auditBatchSize).limit(auditMaxRows + 1);
    for (const row of cursor) {
        if (report.inspected === auditMaxRows) { report.complete = false; break; }
        report.inspected += 1;
        if (projection.source_purchase_terms && (row.source_purchase_terms === null || row.source_purchase_terms === undefined)) report.terms_unknown += 1;
        const failures = inspect(row);
        if (failures.length > 0) {
            report.invalid += 1;
            if (report.samples.length < 20) report.samples.push({id: row.id, reasons: failures});
        }
    }
    return report;
}

const catalogReport = auditCollection("supplier_catalog_items", {connection_id: auditConnection, deleted_at: 0},
    {id: 1, source_purchase_terms: 1, source_fingerprint: 1}, (row) => {
        const failures = auditTerms(row.source_purchase_terms);
        if (row.source_purchase_terms && !/^[a-fA-F0-9]{64}$/.test(row.source_fingerprint || "")) failures.push("SOURCE_FINGERPRINT");
        return failures;
    });

const offeringRows = db.getCollection("supplier_offerings")
    .find({source_connection_id: auditConnection, deleted_at: 0}, {id: 1})
    .sort({_id: 1}).batchSize(auditBatchSize).limit(auditMaxRows + 1).toArray();
const offeringScopeComplete = offeringRows.length <= auditMaxRows;
const offeringIds = offeringRows.slice(0, auditMaxRows).map((row) => row.id);
const revisionReport = auditCollection("supplier_offering_revisions", {supplier_offering_id: {$in: offeringIds}},
    {id: 1, source_purchase_terms: 1}, (row) => auditTerms(row.source_purchase_terms));
revisionReport.complete = revisionReport.complete && offeringScopeComplete;

let encryptedSnapshots = 0;
let supplierActionsRequireRecovery = 0;
const executionReport = auditCollection("supplier_api_order_executions", {connection_id: auditConnection},
    {id: 1, stage: 1, request_encrypted: 1, source_lines: 1, order_tree: 1, pending_order_tree: 1, confirmations: 1}, (row) => {
        if (row.request_encrypted) encryptedSnapshots += 1;
        if (["CREATION_UNKNOWN", "CONFIRMATION_UNKNOWN", "PAYMENT_UNKNOWN"].includes(row.stage)) supplierActionsRequireRecovery += 1;
        const failures = [];
        if (!Array.isArray(row.source_lines) || row.source_lines.length === 0) failures.push("FROZEN_PURCHASE_LINES");
        if (row.stage === "CONFIRM_ORDER" && !row.order_tree) failures.push("CONFIRMATION_WITHOUT_VERIFIED_TREE");
        return failures;
    });

const auditReports = [catalogReport, revisionReport, executionReport];
printjson({
    mode: "READ_ONLY",
    connection_id: auditConnection,
    reports: auditReports,
    coverage_complete: auditReports.every((report) => report.complete),
    invalid_records: auditReports.reduce((sum, report) => sum + report.invalid, 0),
    encrypted_snapshots: encryptedSnapshots,
    supplier_actions_require_recovery: supplierActionsRequireRecovery,
    snapshot_budget_validation: "REQUIRES_APPLICATION_AUTHORIZED_DECRYPTION_NO_HISTORY_BACKFILL",
    rollout_rule: "UNKNOWN_SOURCE_FIELDS_ARE_COMPATIBLE_NEW_WRITES_REQUIRE_VERIFIED_PROTOCOL_AND_BUDGET",
});
