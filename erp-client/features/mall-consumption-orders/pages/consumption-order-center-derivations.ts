/**
 * 对象中心页面的纯派生计算：成本口径、成本覆盖率、事实排序与选中事实解析。
 * 不依赖 React，便于单测与复用。
 */

import type {
    CostBasis,
    MallOrderFactView,
} from "@/features/mall-consumption-orders/types"

/** 只依赖成本评估口径的最小结构，便于直接喂给 view.consumptionEntries。 */
export type CostAssessedEntry = {
    currentCostAssessment: { costBasis: CostBasis }
}

export type CostCoverageState = "complete" | "partial" | "none"

export type CostCoverage = {
    total: number
    coveredCount: number
    percent: number
    state: CostCoverageState
}

export function computeCostBasisPrimary(
    entries: readonly CostAssessedEntry[],
): CostBasis {
    const noneEntries = entries.filter(
        (e) => e.currentCostAssessment.costBasis === "NONE",
    )
    if (noneEntries.length === entries.length && entries.length > 0) {
        return "NONE"
    }
    if (entries.some((e) => e.currentCostAssessment.costBasis === "ACTUAL")) {
        return "ACTUAL"
    }
    if (entries.some((e) => e.currentCostAssessment.costBasis === "STANDARD")) {
        return "STANDARD"
    }
    return "NONE"
}

/** 成本覆盖率按消费条目逐条统计，不做占位伪造。 */
export function computeCostCoverage(
    entries: readonly CostAssessedEntry[],
): CostCoverage {
    const total = entries.length
    const actualCount = entries.filter(
        (e) => e.currentCostAssessment.costBasis === "ACTUAL",
    ).length
    const standardCount = entries.filter(
        (e) => e.currentCostAssessment.costBasis === "STANDARD",
    ).length
    const coveredCount = actualCount + standardCount
    return {
        total,
        coveredCount,
        percent: total === 0 ? 0 : Math.round((coveredCount / total) * 100),
        state:
            total === 0 || coveredCount === 0
                ? "none"
                : coveredCount === total
                  ? "complete"
                  : "partial",
    }
}

export function sortFactsByOccurredAt(
    facts: readonly MallOrderFactView[],
): MallOrderFactView[] {
    return [...facts].sort(
        (a, b) =>
            new Date(a.occurredAt).getTime() - new Date(b.occurredAt).getTime(),
    )
}

/** 未指定 fact 参数时优先选中支付成功记录，其次首条。 */
export function resolveSelectedFactId(
    explicitFactId: string | undefined,
    facts: readonly MallOrderFactView[],
): string | undefined {
    return (
        explicitFactId ??
        facts.find((f) => f.factType === "PAYMENT_SUCCEEDED")?.factId ??
        facts[0]?.factId
    )
}
