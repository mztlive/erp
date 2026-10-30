// 执行：mongosh <受控连接参数> --file 本文件；先停止商品回调消费者。
// MIGRATION_MODE=rollback 只删除本次新增索引，保留全部接收证据和来源资料。
// catalog_progress 缺省为首个提示；source_service_regions 缺省为未知。
// 不回填已处理状态，不清除历史地区、不变更价格或公司商品关系。
const mode = process.env.MIGRATION_MODE || 'apply';
if (!['apply', 'rollback'].includes(mode)) throw new Error('MIGRATION_MODE must be apply or rollback');
const collection = db.getCollection('supplier_catalog_items');
const indexes = [
  { name: 'idx_supplier_catalog_callback_product', key: { connection_id: 1, product_id: 1, deleted_at: 1, id: 1 } },
  { name: 'idx_supplier_catalog_callback_brand', key: { connection_id: 1, brand_id: 1, deleted_at: 1, id: 1 } },
];
for (const index of indexes) {
  if (mode === 'apply') {
    collection.createIndex(index.key, { name: index.name });
  } else if (collection.getIndexes().some(existing => existing.name === index.name)) {
    collection.dropIndex(index.name);
  }
}
print(JSON.stringify({ mode, indexes: indexes.map(index => index.name), dataModified: false }));
