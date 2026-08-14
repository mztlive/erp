/**
 * W18 导入与期初 · 后端 DTO → feature view 映射（P4 F8）。
 * 按资源拆分：DTO 形状见 mapping/dto.ts，字段/枚举映射见 mapping/fields.ts，
 * 视图组装见 mapping/batch-view.ts；本模块只做统一再导出，签名不变。
 */

export type {
    BackendBatchDetail,
    BackendBatchListItem,
    BackendConfirmation,
    BackendRow,
} from "./mapping/dto"
export {
    instantToIso,
    mapIssueCode,
    mapObjectType,
    mapRowStatus,
    toBackendStatusFilter,
} from "./mapping/fields"
export {
    buildBatchView,
    environmentFromQuery,
    toListItem,
} from "./mapping/batch-view"
