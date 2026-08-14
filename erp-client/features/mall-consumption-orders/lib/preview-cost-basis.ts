/**
 * 预览面板成本主口径推导：消费条目全部为 NONE 时按 NONE 展示，
 * 否则取 ACTUAL → STANDARD → NONE 的优先级；无消费条目时回退 NONE。
 */

import type {
    CostBasis,
    MallConsumptionOrderView,
} from "@/features/mall-consumption-orders/types"

export function derivePrimaryCostBasis(
    view: MallConsumptionOrderView,
): CostBasis {
    const noneEntries = view.consumptionEntries.filter(
        (e) => e.currentCostAssessment.costBasis === "NONE",
    )
    if (
        noneEntries.length === view.consumptionEntries.length &&
        view.consumptionEntries.length > 0
    ) {
        return "NONE"
    }
    if (
        view.consumptionEntries.some(
            (e) => e.currentCostAssessment.costBasis === "ACTUAL",
        )
    ) {
        return "ACTUAL"
    }
    if (
        view.consumptionEntries.some(
            (e) => e.currentCostAssessment.costBasis === "STANDARD",
        )
    ) {
        return "STANDARD"
    }
    return "NONE"
}
