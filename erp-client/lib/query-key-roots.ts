/**
 * 跨 feature 失效只依赖稳定根键，不得为了刷新缓存导入对方内部 hook。
 * 各 feature 的完整 query key 工厂仍由本域公开入口持有。
 */
export const queryKeyRoots = {
    accountProfile: ["account", "profile"] as const,
    workItems: ["work-items"] as const,
    approvals: ["approval"] as const,
    approvalCopies: ["approval-copies"] as const,
    salesInquiries: ["sales-inquiries"] as const,
    salesSelection: ["sales-selection"] as const,
    backgroundJobs: ["background-jobs"] as const,
    dataTransfers: ["data-transfer-context"] as const,
    paymentRequests: ["purchase-payment-requests"] as const,
    invoiceRequests: ["invoice-requests"] as const,
    purchaseOrders: ["purchase-orders"] as const,
    salesOrders: ["sales-orders"] as const,
    workspaceHome: ["workspace-home"] as const,
    entitySelectors: ["entity-selectors"] as const,
    customerReceivables: ["customer-receivables"] as const,
    supplierPayables: ["supplier-payables"] as const,
    fulfillmentOperations: ["fulfillment-operations"] as const,
}
