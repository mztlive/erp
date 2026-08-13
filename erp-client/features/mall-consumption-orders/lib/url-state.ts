/**
 * 纯 URL 参数解析：列表筛选参数白名单解析与对象中心 section 解析。
 * 不含 React hooks，供页面在渲染期调用。
 */

import type {
    FactType,
    MallConsumptionOrderMetricKey,
    ObjectCenterSectionId,
    SupplierFulfillmentStatus,
} from "@/features/mall-consumption-orders/types"
import {
    FACT_TYPE_LABEL,
    OBJECT_CENTER_SECTIONS,
    SUPPLIER_STATUS_LABEL,
} from "@/features/mall-consumption-orders/types"

export function parseMetric(
    raw: string | null,
): MallConsumptionOrderMetricKey | "all" {
    if (
        raw === "paid" ||
        raw === "pending_attr" ||
        raw === "fact_diff" ||
        raw === "auto_exception" ||
        raw === "cost_none"
    ) {
        return raw
    }
    return "all"
}

/** 逗号分隔多值 URL 参数 → 白名单过滤后的数组；非法值忽略。 */
export function parseMultiValue<T extends string>(
    raw: string | null,
    allowed: readonly T[],
): T[] {
    if (!raw) return []
    const set = new Set<string>(allowed)
    return raw
        .split(",")
        .map((v) => v.trim())
        .filter((v): v is T => v !== "" && set.has(v))
}

export const FACT_TYPES = Object.keys(FACT_TYPE_LABEL) as FactType[]

export const SUPPLIER_STATUSES = Object.keys(
    SUPPLIER_STATUS_LABEL,
) as SupplierFulfillmentStatus[]

export const DATA_SOURCES = ["REALTIME", "BACKFILL"] as const

export function parseSection(raw: string | null): ObjectCenterSectionId {
    const found = OBJECT_CENTER_SECTIONS.find((s) => s.id === raw)
    return found?.id ?? "overview"
}
