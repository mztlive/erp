// 在目标 ERP 数据库执行，只登记单位精确匹配索引，不修改单位或商品数据。
// 应用：mongosh "$ERP_MONGO_URI" --file scripts/migrations/20261027-catalog-unit-indexes.mongosh.js
// 回退：ERP_INDEX_ROLLBACK=1 mongosh "$ERP_MONGO_URI" --file scripts/migrations/20261027-catalog-unit-indexes.mongosh.js
// 回退前先回退 API 与索引登记代码；已引用单位及审计必须保留。
const catalogUnitIndexes = [
    [{name: 1}, "idx_unit_of_measures_name"],
    [{symbol: 1}, "idx_unit_of_measures_symbol"],
];
const catalogUnits = db.getCollection("unit_of_measures");
for (const [keys, name] of catalogUnitIndexes) {
    if (process.env.ERP_INDEX_ROLLBACK === "1") {
        if (catalogUnits.getIndexes().some((index) => index.name === name)) {
            catalogUnits.dropIndex(name);
        }
    } else {
        catalogUnits.createIndex(keys, {name, unique: false});
    }
}
