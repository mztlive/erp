/**
 * W10 库存台账 · 真实 HTTP API（按资源拆分）。
 * 对接 /admin/stock-balances|movements|reservations|adjustments。
 * 实现拆分：dto.ts（后端形状）、display.ts（文案映射）、pagination.ts（排序/游标）、
 * mappers.ts（DTO → 视图）、list/detail/adjustment/export.ts（HTTP 入口）。
 * 本文件只做再导出，保持既有导入路径可用。
 */

export {
    buildAdjustmentSubmitRequest,
    createAdjustmentDraft,
    fetchAdjustmentDetail,
    readInstanceResponsibility,
    resolveAdjustmentUnknown,
    STOCK_ADJUSTMENT_DOCUMENT_TYPE,
    submitAdjustment,
} from "./adjustment"
export { fetchBalanceDetail } from "./detail"
export { startInventoryExport } from "./export"
export { fetchInventoryList } from "./list"
export type { InventoryExportJob } from "./dto"
