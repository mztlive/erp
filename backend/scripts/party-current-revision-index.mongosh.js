/* global db, printjson */
// 当前修订名称搜索索引执行合同：只处理 parties 的一个新增非唯一索引。
// 前置条件：连接到已确认的业务库；ERP_PARTY_INDEX_DB 必须与 db.getName() 一致；
// 执行账号具备目标库的读取、createIndex / dropIndex 权限。
// 发布顺序：先执行 up 并确认索引完成，再部署按修订 ID 分批关联主体的版本。
// 以下命令在 backend/ 执行，连接串从 ERP_MONGO_URI 环境读取，不打印连接串。
// 检查：ERP_PARTY_INDEX_DB=<业务库名> mongosh "$ERP_MONGO_URI" --file scripts/party-current-revision-index.mongosh.js
// 上线：ERP_PARTY_INDEX_DB=<业务库名> ERP_PARTY_INDEX_MODE=up mongosh "$ERP_MONGO_URI" --file scripts/party-current-revision-index.mongosh.js
// 回滚：先恢复不分批关联的应用版本，并阻止新版重启自动重建索引；随后执行：
// ERP_PARTY_INDEX_DB=<业务库名> ERP_PARTY_INDEX_MODE=down ERP_PARTY_INDEX_UNBATCHED=1 mongosh "$ERP_MONGO_URI" --file scripts/party-current-revision-index.mongosh.js
// 默认 check 只读取索引。禁止删除其他索引、修改业务数据或 drop 集合/数据库。

const expectedDatabase = process.env.ERP_PARTY_INDEX_DB;
const mode = process.env.ERP_PARTY_INDEX_MODE || "check";
const indexName = "idx_parties_current_revision_active_id";
const keys = { current_revision_id: 1, deleted_at: 1, id: 1 };

if (!expectedDatabase || db.getName() !== expectedDatabase || ["admin", "local", "config"].includes(expectedDatabase)) {
    throw new Error("ERP_PARTY_INDEX_DB 必须明确指定当前业务数据库");
}
if (!["check", "up", "down"].includes(mode)) {
    throw new Error("ERP_PARTY_INDEX_MODE 仅允许 check、up 或 down");
}
if (mode === "down" && process.env.ERP_PARTY_INDEX_UNBATCHED !== "1") {
    throw new Error("回滚前必须恢复不分批关联的应用版本，并设置 ERP_PARTY_INDEX_UNBATCHED=1");
}
const collectionInfo = db.getCollectionInfos({ name: "parties" })[0];
if (!collectionInfo) {
    throw new Error("业务库缺少 parties 集合，停止索引操作");
}

const collection = db.getCollection("parties");
const existing = collection.getIndexes().find((index) => index.name === indexName);
const normalizedCollation = (value) => JSON.stringify(Object.entries(value || { locale: "simple" }).sort());
if (existing && (
    JSON.stringify(Object.entries(existing.key)) !== JSON.stringify(Object.entries(keys)) ||
    existing.unique === true || existing.sparse === true || existing.hidden === true ||
    existing.partialFilterExpression ||
    normalizedCollation(existing.collation) !== normalizedCollation(collectionInfo.options?.collation)
)) {
    throw new Error("同名索引与发布声明不同，禁止替换或删除；须先处理索引冲突");
}

if (mode === "up" && !existing) {
    collection.createIndex(keys, { name: indexName });
}
if (mode === "down" && existing) {
    collection.dropIndex(indexName);
}
printjson({
    mode,
    database: expectedDatabase,
    collection: "parties",
    index: indexName,
    keys,
    present: mode === "up" || (mode === "check" && Boolean(existing)),
});
