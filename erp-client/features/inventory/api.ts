/**
 * W10 库存台账 · 稳定 HTTP 入口。
 * 请求函数与 DTO 映射实现见 api/inventory；本文件只做再导出。
 */

export {
    createAdjustmentDraft,
    fetchBalanceDetail,
    fetchInventoryList,
    resolveAdjustmentUnknown,
    startInventoryExport,
    submitAdjustment,
} from "./api/inventory"
export type { InventoryExportJob } from "./api/inventory"
