/**
 * W26 供应商订单 · 真实 HTTP API 桶。
 * 路径：/admin/supplier-fulfillment-orders、/admin/work-items、/admin/background-jobs
 * 请求实现按资源拆分：list / detail / investigations / task-completions /
 * aftersales / export / misc；Wire DTO 类型见 wire-types.ts，映射见 mapping.ts。
 * 本文件只做再导出，保持既有导入路径不变。
 */

export { fetchSupplierOrders } from "./list"
export { fetchSupplierOrderDetail } from "./detail"
export { querySupplierResult, replaySupplierOrder } from "./investigations"
export { completeSupplierOrderTask } from "./task-completions"
export { submitAfterSalesAction } from "./aftersales"
export { createSupplierOrderExportJob } from "./export"
export {
    addCollaborationNote,
    clearAddressReveal,
    revealSupplierOrderAddress,
} from "./misc"
