/**
 * W11 客户往来 API：真实 HTTP（/admin/receivable-accounts、customer-receipts、invoices、
 * receipt-reversals、customer-refunds）。
 * 开放余额与净分配一律来自服务端投影，禁止前端拟合计冒充结果。
 * 导出签名保持稳定，供 queries.ts 作 queryFn/mutationFn。
 *
 * 实现按资源拆分：dto（后端形状）、mappers（投影）、loaders（列表 HTTP）、
 * list-view（列表/详情视图）、session（草稿会话）、post-allocation（提交核销）、
 * reverse-fact（冲正/退款/红票）。本文件仅做公共出口，保持既有导入路径不变。
 */

export {
    fetchCustomerAccountsList,
    fetchCustomerAccountsDetail,
} from "./list-view"
export {
    createAllocationSession,
    fetchAllocationSession,
    saveAllocationDraft,
} from "./session"
export {
    ensureCustomerReceiptDraft,
    postAllocation,
    resolvePostUnknown,
} from "./post-allocation"
export { reverseFact } from "./reverse-fact"
