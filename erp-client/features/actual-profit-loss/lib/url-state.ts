import type { DataFreshnessState } from "@/components/business/page"
import type {
    PeriodPreset,
    ProfitLossCoverage,
    ProfitLossDimension,
    ProjectionFreshnessState,
} from "@/features/actual-profit-loss/types"

const PERIOD_BASIS_LABEL: Record<string, string> = {
    sales_revenue_recognition_date: "销售收入确认日",
    sales_order_effective_date: "销售单生效日",
    fulfillment_complete_date: "履约完成日",
    cost_occurred_date: "成本发生日",
}

export function basisLabel(code: string): string {
    return PERIOD_BASIS_LABEL[code] ?? code
}

export function parseCoverage(raw: string | null): ProfitLossCoverage {
    if (raw === "uncovered" || raw === "all" || raw === "covered") return raw
    return "covered"
}

export function parseDimension(raw: string | null): ProfitLossDimension {
    if (
        raw === "customer" ||
        raw === "scenario" ||
        raw === "fulfillment" ||
        raw === "cost_type" ||
        raw === "sales_order"
    ) {
        return raw
    }
    return "sales_order"
}

/**
 * 逗号分隔多选参数（docs/ui-filter-design.md §6.1）：
 * 解析为去重、排序后的值列表，空值与空白项丢弃。
 */
export function parseCsvValues(raw: string | null): string[] {
    if (!raw) return []
    const values = raw
        .split(",")
        .map((value) => value.trim())
        .filter(Boolean)
    return [...new Set(values)].sort()
}

/** 多选参数序列化：去重、排序后的逗号分隔值；空列表返回空串（不写 URL）。 */
export function serializeCsvValues(values: readonly string[]): string {
    const cleaned = values.map((value) => value.trim()).filter(Boolean)
    return [...new Set(cleaned)].sort().join(",")
}

export function parsePreset(raw: string | null): PeriodPreset {
    if (
        raw === "last-month" ||
        raw === "quarter-to-date" ||
        raw === "month-to-date"
    ) {
        return raw
    }
    return "month-to-date"
}

function toISODate(date: Date): string {
    const y = date.getFullYear()
    const m = String(date.getMonth() + 1).padStart(2, "0")
    const d = String(date.getDate()).padStart(2, "0")
    return `${y}-${m}-${d}`
}

/** periodPreset → 明确 from/to（运行时锚定当前月，避免「本月迄今」解析为单日） */
export function resolvePeriod(preset: PeriodPreset): {
    from: string
    to: string
} {
    const now = new Date()
    const today = toISODate(now)
    if (preset === "last-month") {
        const firstOfThisMonth = new Date(now.getFullYear(), now.getMonth(), 1)
        const lastOfLastMonth = new Date(firstOfThisMonth.getTime() - 1)
        return {
            from: toISODate(new Date(now.getFullYear(), now.getMonth() - 1, 1)),
            to: toISODate(lastOfLastMonth),
        }
    }
    if (preset === "quarter-to-date") {
        const quarterStartMonth = Math.floor(now.getMonth() / 3) * 3
        return {
            from: toISODate(new Date(now.getFullYear(), quarterStartMonth, 1)),
            to: today,
        }
    }
    return {
        from: toISODate(new Date(now.getFullYear(), now.getMonth(), 1)),
        to: today,
    }
}

export function mapFreshnessState(
    state: ProjectionFreshnessState,
    options?: { refreshFailed?: boolean; refreshing?: boolean },
): {
    uiState: DataFreshnessState
    statusLabel: string
} {
    if (options?.refreshing) {
        return { uiState: "syncing", statusLabel: "正在刷新数据" }
    }
    if (options?.refreshFailed) {
        return { uiState: "failed", statusLabel: "刷新失败 · 保留旧数据" }
    }
    switch (state) {
        case "stale":
            return {
                uiState: "stale",
                statusLabel: "数据陈旧 · 来源更新时间已超前",
            }
        case "rebuilding":
            return { uiState: "syncing", statusLabel: "数据更新中" }
        case "failed":
            return { uiState: "failed", statusLabel: "数据更新失败" }
        default:
            return { uiState: "fresh", statusLabel: "数据已更新" }
    }
}

export function coveragePercentNumber(rate: string): number {
    const n = Number(rate.replace("%", ""))
    return Number.isFinite(n) ? Math.min(100, Math.max(0, n)) : 0
}

export const W16_FORMULA_HINT =
    "实际经营盈亏（不含税）= 非卡券不含税销售收入 − 非卡券不含税实际采购成本 − 非卡券不含税实际履约费用"
