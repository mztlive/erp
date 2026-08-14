/**
 * W07 采购二次确认 · 真实 HTTP API。
 * 契约形状保持 features/procurement-confirmation/types.ts 与 hooks/queries.ts 不变；
 * 后端差异在各资源模块内适配，缺口登记见 docs/dev-plan/p4-evidence/F4.md。
 * 本文件仅汇聚公共导出，按资源拆分为 filters / errors / mapping / details /
 * recommendation / supply-options / sales-document / queue / mutations。
 */

export type { QueueFilters } from "./filters"
export { fetchProcurementRecommendation } from "./recommendation"
export type { ProcurementSupplyOption } from "./supply-options"
export { fetchProcurementSupplyOptions } from "./supply-options"
export type { ProcurementWorkItemPresentation } from "./sales-document"
export { fetchProcurementWorkItemPresentation } from "./sales-document"
export { fetchProcurementQueue } from "./queue"
export {
    saveProcurementConfirmation,
    completeProcurementDecision,
} from "./mutations"
