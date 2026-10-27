// 执行前停止商品写入并备份 product_revisions；使用目标数据库运行 mongosh。
// 默认只读核对。ERP_APPLY_CATALOG_MATCH_NAME=1 才执行回填。
// 回滚只需停止自动匹配并恢复旧程序，保留派生字段与并发控制集合。
const apply = process.env.ERP_APPLY_CATALOG_MATCH_NAME === "1";
function matchName(value) {
    if (typeof value !== "string" || !value.trim()) throw new Error("商品名称缺失，停止回填");
    return value.replace(/[！-～]/gu, ch => String.fromCodePoint(ch.codePointAt(0) - 0xfee0))
        .split(/[\u0009-\u000d \u0085\u00a0\u1680\u2000-\u200a\u2028\u2029\u202f\u205f\u3000]+/u).filter(Boolean).join(" ");
}
let checked = 0;
let changed = 0;
let batch = [];
function flush() {
    if (apply && batch.length) db.product_revisions.bulkWrite(batch, { ordered: true });
    batch = [];
}
// 一次性迁移按 id 索引游标分批读取；不把整库装入内存。
for (const row of db.product_revisions.find({}, { id: 1, name: 1, match_name: 1 }).sort({ id: 1 }).batchSize(500)) {
    const value = matchName(row.name);
    checked++;
    if (row.match_name !== value) {
        changed++;
        batch.push({ updateOne: { filter: { _id: row._id, name: row.name }, update: { $set: { match_name: value } } } });
        if (batch.length === 500) flush();
    }
}
flush();
print(JSON.stringify({ apply, checked, changed }));
