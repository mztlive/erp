// 在目标 ERP 数据库执行；只登记新增索引，不变更款项或业务事实。
// 回滚前关闭新功能并暂停相关采购付款；保留全部集合和索引，不删留抵事实。
const supplierCreditIndexes = [
    ["supplier_refunds", {source_purchase_order_id: 1, status: 1}, "idx_supplier_refunds_purchase", false],
    ["supplier_credits", {id: 1}, "uk_supplier_credits_id", true],
    ["supplier_credits", {source_refund_id: 1}, "uk_supplier_credits_source", true],
    ["supplier_credits", {original_payment_id: 1}, "idx_supplier_credits_payment", false],
    ["supplier_credits", {supplier_id: 1, payer_party_id: 1, created_at: 1}, "idx_supplier_credits_owner", false],
    ["supplier_credit_uses", {id: 1}, "uk_supplier_credit_uses_id", true],
    ["supplier_credit_uses", {payment_request_id: 1}, "idx_supplier_credit_uses_request", false],
    ["supplier_credit_uses", {credit_id: 1}, "idx_supplier_credit_uses_credit", false],
];
for (const [collection, keys, name, unique] of supplierCreditIndexes) {
    db.getCollection(collection).createIndex(keys, {name, unique});
}
