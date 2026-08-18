/**
 * W05 销售单 HTTP API（queryFn / mutationFn 纯函数）。
 *
 * 后端域：sales_order / sales_review / work_item / bulk_job。
 * 实现已按资源拆分至 api/sales-orders-*.ts；本文件统一再导出，
 * 保持既有导入路径（@/features/sales-orders/api/sales-orders）不变。
 */

export type {
    SalesOrderDetailView,
    SalesOrderListView,
    SalesOrdersListQuery,
} from "@/features/sales-orders/api/contracts"

export { createSalesOrderExportJob } from "@/features/sales-orders/api/export"

export { fetchSalesOrders } from "@/features/sales-orders/api/sales-orders-list"
export {
    downloadSalesOrderContractPdf,
    fetchSalesOrderDetail,
} from "@/features/sales-orders/api/sales-orders-detail"
export {
    createSalesOrder,
    fetchSalesOrderDraftForResume,
    saveSalesOrderDraft,
    submitSalesOrder,
    type SalesOrderDraftResumeData,
    type SubmitSalesOrderInput,
} from "@/features/sales-orders/api/sales-orders-create"
export {
    adjustProcurementRejectionDraft,
    prepareProcurementRejectionResolution,
    resolveProcurementRejection,
    type ResolveProcurementRejectionInput,
    type ResolveProcurementRejectionIntent,
    type ResolveProcurementRejectionPayload,
} from "@/features/sales-orders/api/sales-orders-procurement"
export { completeLowMarginManagerConfirmation } from "@/features/sales-orders/api/sales-orders-low-margin"
export {
    fetchSalesChangeOrderDetail,
    prepareStartSalesChangeOrder,
    startSalesChangeOrder,
    submitSalesChangeOrder,
    submitSalesChangeReviewDecision,
    type SalesChangeReviewDecisionInput,
    type StartSalesChangeOrderInput,
    type StartSalesChangeOrderIntent,
    type StartSalesChangeOrderPayload,
    type SubmitSalesChangeOrderInput,
} from "@/features/sales-orders/api/sales-orders-change"
