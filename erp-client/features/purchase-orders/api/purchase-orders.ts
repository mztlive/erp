/**
 * W08 采购单 · 真实 HTTP API（公共入口）。
 * 实现已按资源拆分到 ./purchase-order-* 模块，本文件保持原有导出面不变。
 */

export type {
    PurchaseOrderListQuery,
    PurchaseOrderListResult,
} from "./purchase-orders-contract"
export type { CreationBasesQuery } from "./purchase-order-queries-api"

export {
    acquireDraftEditToken,
    fetchActivePurchaseChangeOrder,
    fetchCreationBases,
    fetchPurchaseChangeOrderDetail,
    fetchPurchaseOrderCenter,
    fetchPurchaseOrderExportData,
    fetchPurchaseOrders,
} from "./purchase-order-queries-api"

export {
    cancelPurchaseOrderApproval,
    createPurchaseOrderFromBasis,
    createPurchaseOrdersFromSourcing,
    savePurchaseOrderDraft,
    startPurchaseChange,
    submitPurchaseChange,
    submitPurchaseOrderForReview,
    voidPurchaseOrderDraft,
} from "./purchase-order-commands"
