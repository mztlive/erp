/**
 * W27 结算列表 · 筛选参数清单与派生规则
 * 对齐 docs/ui-filter-design.md §5.6：筛选参数集中声明，
 * hasAppliedFilters / chips / 提交与清除共用同一清单，避免漏清或隐形状态。
 */

import type { SettlementsUrlState } from "@/features/supplier-settlements/lib/url-state"
import {
    DIFF_TYPE_LABEL,
    STATUS_LABEL,
    type DifferenceType,
    type SettlementStatus,
} from "@/features/supplier-settlements/types"

/** 可被单独移除的已生效条件。 */
export type SettlementFilterKey =
    | "q"
    | "supplierId"
    | "status"
    | "differenceType"
    | "period"

export type SettlementFilterState = Pick<
    SettlementsUrlState,
    "q" | "supplierId" | "status" | "differenceType" | "periodFrom" | "periodTo"
>

export const SETTLEMENT_STATUS_VALUES = Object.keys(
    STATUS_LABEL,
) as SettlementStatus[]

export const DIFF_TYPE_RADIO_OPTIONS: ReadonlyArray<{
    value: DifferenceType | "all"
    label: string
}> = [
    { value: "all", label: "全部" },
    ...(Object.keys(DIFF_TYPE_LABEL) as DifferenceType[]).map((value) => ({
        value,
        label: DIFF_TYPE_LABEL[value],
    })),
]

/** URL 中逗号分隔的状态值解析为合法状态数组；非法枚举值降级丢弃。 */
export function parseSettlementStatusParam(raw?: string): SettlementStatus[] {
    if (!raw) return []
    return Array.from(
        new Set(
            raw
                .split(",")
                .map((value) => value.trim())
                .filter(
                    (value): value is SettlementStatus => value in STATUS_LABEL,
                ),
        ),
    )
}

/** 状态数组序列化为逗号分隔的 URL 值；空数组返回 undefined（不写 URL）。 */
export function joinSettlementStatusParam(
    values: readonly string[],
): string | undefined {
    const joined = parseSettlementStatusParam(values.join(",")).join(",")
    return joined || undefined
}

/** 结构化条件（面板内字段）是否存在已生效值。 */
export function hasStructuredSettlementFilters(
    state: SettlementFilterState,
): boolean {
    return Boolean(
        state.supplierId ||
        state.status ||
        state.differenceType ||
        state.periodFrom ||
        state.periodTo,
    )
}

/** 全部筛选参数（含关键词）是否存在已生效值；视图属于 Saved View，不算筛选。 */
export function hasAppliedSettlementFilters(
    state: SettlementFilterState,
): boolean {
    return hasStructuredSettlementFilters(state) || Boolean(state.q?.trim())
}

/** 结算期间校验：ISO 日期可直接按字符串比较。 */
export function validateSettlementPeriodRange(
    from: string,
    to: string,
): string | null {
    const fromTrimmed = from.trim()
    const toTrimmed = to.trim()
    if (fromTrimmed && toTrimmed && fromTrimmed > toTrimmed) {
        return "期间开始日期不能晚于结束日期"
    }
    return null
}

export type SettlementAppliedChip = Readonly<{
    key: SettlementFilterKey
    label: string
}>

/** 全部已生效条件均可单独撤销；chip 展示业务名或中文映射，不展示内部枚举原值。 */
export function buildSettlementFilterChips(
    state: SettlementFilterState,
    suppliers: readonly {
        supplierId: string
        supplierName: string
    }[],
): readonly SettlementAppliedChip[] {
    const chips: SettlementAppliedChip[] = []
    const q = state.q?.trim()
    if (q) chips.push({ key: "q", label: `搜索：${q}` })
    if (state.supplierId) {
        const supplierName = suppliers.find(
            (supplier) => supplier.supplierId === state.supplierId,
        )?.supplierName
        chips.push({
            key: "supplierId",
            label: `供应商：${supplierName ?? state.supplierId}`,
        })
    }
    const statuses = parseSettlementStatusParam(state.status)
    if (statuses.length > 0) {
        chips.push({
            key: "status",
            label: `状态：${statuses
                .map((status) => STATUS_LABEL[status])
                .join("、")}`,
        })
    }
    if (state.differenceType) {
        chips.push({
            key: "differenceType",
            label: `差异类型：${DIFF_TYPE_LABEL[state.differenceType]}`,
        })
    }
    if (state.periodFrom || state.periodTo) {
        chips.push({
            key: "period",
            label: `期间：${state.periodFrom ?? "不限"} 至 ${state.periodTo ?? "不限"}`,
        })
    }
    return chips
}
