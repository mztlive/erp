/**
 * W17 商城同步 · 真实 HTTP API（P4 F8）。
 * 导出签名保持与 queries.ts / 页面一致；字段适配仅在本文件完成。
 * 后端域：mall_sync + source_registry（/admin/source-systems 样板已有）。
 *
 * 按资源拆分：backend-dtos（wire DTO）、mappers（DTO → 视图）、fetch-page（聚合读）、
 * trigger-jobs（同步任务写）、mapping-actions（映射写）、source-systems（来源系统）。
 * 本文件只做再导出，既有导入路径保持不变。
 */

export type { MallSyncQueryInput } from "./fetch-page"
export { fetchMallSyncPage, resolveMallSourceSystemId } from "./fetch-page"
export {
    retryFailedJob,
    triggerManualIncremental,
    triggerSingleOrderPull,
} from "./trigger-jobs"
export {
    confirmMapping,
    reapplyMallSnapshot,
    requestSourceFix,
    resolveUnknownReapply,
} from "./mapping-actions"
export { fetchSourceSystems } from "./source-systems"
export type {
    BackendCursor,
    BackendJob,
    BackendMappingTask,
    BackendReconItem,
    BackendReconJob,
    BackendSnapshot,
    BackendSourceSystem,
} from "./backend-dtos"
