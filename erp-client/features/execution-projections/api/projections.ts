/**
 * W23 销售单执行投影 · API 适配层统一出口。
 * 实现拆分为 mapping / reads / commands，本文件仅做兼容再导出，
 * 所有既有 `@/features/execution-projections/api/projections` 导入保持不变。
 */

export {
    secsToIso,
    mapDeliveryStatus,
    mapSource,
    mapCardForm,
    recomputeActions,
    whitelistFromRevision,
    loadMalls,
    mallName,
    computeMetrics,
    filterSummary,
    toRow,
} from "./mapping"
export type {
    BackendProjection,
    BackendRevision,
    BackendDelivery,
    BackendDeliveryActionResult,
} from "./mapping"

export {
    fetchExecutionProjectionList,
    fetchExecutionProjectionDetail,
    fetchSalesOrderCollaboration,
} from "./reads"

export {
    BULK_SELECTION_LIMIT,
    submitProjectionDeliveryCommand,
    submitBulkProjectionCommand,
} from "./commands"
export type { DeliveryCommandInput, BulkCommandInput } from "./commands"
