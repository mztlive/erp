/**
 * W27 API 供应商结算 · 兼容出口
 * 实现已按资源拆分为 settlements-wire / settlements-list /
 * settlements-detail / settlements-actions；本模块保持原有导出不变。
 */

export { fetchSettlementList } from "./settlements-list"
export type { ListQueryInput } from "./settlements-list"
export { fetchSettlementDetail } from "./settlements-detail"
export {
    appendDifferenceEvidence,
    createSettlementDraft,
    decideSettlementReview,
    refreshSettlementTrial,
    resolveDifference,
    submitSettlementReview,
} from "./settlements-actions"
