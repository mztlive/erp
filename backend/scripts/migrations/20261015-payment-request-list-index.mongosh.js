// 在目标 ERP 数据库执行；不读取或修改业务文档。
// 应用：mongosh "$ERP_MONGO_URI" --file scripts/migrations/20261015-payment-request-list-index.mongosh.js
// 回滚：ERP_INDEX_ROLLBACK=1 mongosh "$ERP_MONGO_URI" --file scripts/migrations/20261015-payment-request-list-index.mongosh.js
// 回滚前须先部署不再登记该索引的代码，否则下次启动会重建。
const paymentRequestIndexName = "idx_purchase_payment_requests_list";
const paymentRequestCollection = db.getCollection("purchase_payment_requests");
if (process.env.ERP_INDEX_ROLLBACK === "1") {
    if (paymentRequestCollection.getIndexes().some((index) => index.name === paymentRequestIndexName)) {
        paymentRequestCollection.dropIndex(paymentRequestIndexName);
    }
} else {
    paymentRequestCollection.createIndex(
        { deleted_at: 1, created_at: -1, id: -1 },
        { name: paymentRequestIndexName, unique: false },
    );
}
