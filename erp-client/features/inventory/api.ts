/**
 * W10 库存台账 · 稳定 HTTP 入口。
 * 请求函数与 DTO 映射实现见 api/inventory；本文件只做再导出。
 */

export {
    buildAdjustmentSubmitRequest,
    buildCancelStockAdjustmentApprovalRequest,
    cancelStockAdjustmentApproval,
    createAdjustmentDraft,
    fetchAdjustmentDetail,
    fetchBalanceDetail,
    readInstanceResponsibility,
    fetchInventoryList,
    resolveAdjustmentUnknown,
    startInventoryExport,
    STOCK_ADJUSTMENT_DOCUMENT_TYPE,
    submitAdjustment,
} from "./api/inventory"
export type {
    CancelStockAdjustmentApprovalRequest,
    InventoryExportJob,
} from "./api/inventory"
