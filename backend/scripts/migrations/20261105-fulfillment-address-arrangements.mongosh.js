// 默认只读；发布批准后 APPLY_MIGRATION=true 创建唯一索引，不回填收件资料。
const collection = db.getCollection("fulfillment_address_arrangements");
const exists = db.getCollectionNames().includes("fulfillment_address_arrangements");
const duplicateCount = exists
    ? collection.aggregate([{ $group: { _id: "$id", count: { $sum: 1 } } }, { $match: { count: { $gt: 1 } } }, { $count: "count" }]).toArray()[0]?.count ?? 0
    : 0;
printjson({ collectionExists: exists, records: exists ? collection.countDocuments({}) : 0, duplicateIdentities: duplicateCount });
if (duplicateCount) throw new Error("地址安排身份重复，停止发布并核对；不得删除历史记录");
if (process.env.APPLY_MIGRATION === "true") {
    collection.createIndex({ id: 1 }, { unique: true, name: "uk_fulfillment_address_arrangements_id" });
    print("收货安排唯一索引已就绪；未回填地址");
} else {
    print("只读核对完成；未修改集合或索引");
}
