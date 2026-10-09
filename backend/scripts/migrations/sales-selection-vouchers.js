// 执行合同：先停旧版 API 与 worker，完成备份；检查通过后执行 apply，再启动新版。
// 检查：ERP_SELECTION_MIGRATION_MODE=check mongosh "$ERP_MONGO_URI" --file scripts/migrations/sales-selection-vouchers.js
// 迁移：ERP_SELECTION_MIGRATION_MODE=apply mongosh "$ERP_MONGO_URI" --file scripts/migrations/sales-selection-vouchers.js
// 回滚：停新版进程后执行 mode=rollback；仅在不存在提货券业务数据、全部公开链接已撤销时允许回滚。
// 本脚本默认只读检查，不生成历史选品册密码。历史册公开访问由新版锁定，负责人须设置密码。

const migrationMode = process.env.ERP_SELECTION_MIGRATION_MODE || "check"
const sessions = db.sales_selection_sessions
const proposals = db.sales_selection_proposals
const booklets = db.sales_selection_booklets

function participantDuplicates(collection) {
  return collection.aggregate([
    { $group: { _id: { booklet_id: "$booklet_id", participant_id: { $ifNull: ["$participant_id", ""] } }, count: { $sum: 1 } } },
    { $match: { count: { $gt: 1 } } },
    { $limit: 1 },
  ]).hasNext()
}

function ensurePreflight() {
  if (participantDuplicates(sessions) || participantDuplicates(proposals)) {
    throw new Error("同册同参与人存在重复会话或方案，禁止修改索引")
  }
  const duplicateVoucher = sessions.aggregate([
    { $match: { voucher_code_hash: { $type: "string" } } },
    { $group: { _id: "$voucher_code_hash", count: { $sum: 1 } } },
    { $match: { count: { $gt: 1 } } },
    { $limit: 1 },
  ]).hasNext()
  if (duplicateVoucher) throw new Error("提货券哈希重复，禁止修改索引")
}

function dropIfExists(collection, name) {
  if (collection.getIndexes().some(index => index.name === name)) collection.dropIndex(name)
}

ensurePreflight()

if (migrationMode === "check") {
  printjson({
    mode: migrationMode,
    sessions_missing_participant: sessions.countDocuments({ participant_id: { $exists: false } }),
    proposals_missing_participant: proposals.countDocuments({ participant_id: { $exists: false } }),
    published_without_password: booklets.countDocuments({ status: { $in: ["PUBLISHED", "SUBMITTED"] }, access_password_hash: null }),
    voucher_booklets: booklets.countDocuments({ submit_mode: "PICKUP_VOUCHER" }),
    active_public_links: booklets.countDocuments({ link_token_hash: { $type: "string" }, link_revoked: { $ne: true } }),
    session_indexes: sessions.getIndexes().map(index => index.name),
    proposal_indexes: proposals.getIndexes().map(index => index.name),
  })
} else if (migrationMode === "apply") {
  // 历史普通模式统一为空参与人；新字段为兼容新增，不删除原有任何业务字段。
  sessions.updateMany({ participant_id: null }, { $set: { participant_id: "" } })
  proposals.updateMany({ participant_id: null }, { $set: { participant_id: "" } })
  sessions.createIndex({ booklet_id: 1, participant_id: 1 }, { name: "uk_sales_selection_sessions_participant", unique: true })
  sessions.createIndex({ voucher_code_hash: 1 }, {
    name: "uk_sales_selection_sessions_voucher_hash", unique: true,
    partialFilterExpression: { voucher_code_hash: { $type: "string" } },
  })
  proposals.createIndex({ booklet_id: 1, participant_id: 1 }, { name: "uk_sales_selection_proposals_participant", unique: true })
  dropIfExists(sessions, "uk_sales_selection_sessions_booklet")
  dropIfExists(proposals, "uk_sales_selection_proposals_booklet")
  ensurePreflight()
  printjson({ mode: migrationMode, session_indexes: sessions.getIndexes().map(index => index.name), proposal_indexes: proposals.getIndexes().map(index => index.name) })
} else if (migrationMode === "rollback") {
  // 旧版公开路由忽略密码字段。恢复旧版前必须由业务命令关闭或撤销全部公开链接。
  // 此门禁不自动撤销链接，保证回滚脚本不改变业务访问状态。
  const activePublicLinks = booklets.countDocuments({ link_token_hash: { $type: "string" }, link_revoked: { $ne: true } })
  if (activePublicLinks) {
    throw new Error("存在未撤销的公开选品链接，禁止回滚；先关闭或撤销全部公开链接，避免旧版路由绕过密码公开选品内容")
  }
  const personalSessions = sessions.countDocuments({ participant_id: { $nin: ["", null] } })
  const personalProposals = proposals.countDocuments({ participant_id: { $nin: ["", null] } })
  if (booklets.countDocuments({ submit_mode: "PICKUP_VOUCHER" }) || personalSessions || personalProposals) {
    throw new Error("存在提货券册、个人会话或个人方案，禁止回滚；保留新版并按业务合同处理数据")
  }
  sessions.createIndex({ booklet_id: 1 }, { name: "uk_sales_selection_sessions_booklet", unique: true })
  proposals.createIndex({ booklet_id: 1 }, { name: "uk_sales_selection_proposals_booklet", unique: true })
  dropIfExists(sessions, "uk_sales_selection_sessions_participant")
  dropIfExists(sessions, "uk_sales_selection_sessions_voucher_hash")
  dropIfExists(proposals, "uk_sales_selection_proposals_participant")
  printjson({ mode: migrationMode, session_indexes: sessions.getIndexes().map(index => index.name), proposal_indexes: proposals.getIndexes().map(index => index.name) })
} else {
  throw new Error("ERP_SELECTION_MIGRATION_MODE 只允许 check、apply 或 rollback")
}
