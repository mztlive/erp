import type {
    CurrentQualityDimension,
    HistoryQualityDimension,
    QualityCaliber,
} from "../dual-types"

export const DUAL_CALIBERS: ReadonlyArray<QualityCaliber> = [
    "current",
    "history",
]

const CURRENT_DIMENSIONS: ReadonlyArray<CurrentQualityDimension> = [
    "customer",
    "owner_user",
    "owner_org",
]

const HISTORY_DIMENSIONS: ReadonlyArray<HistoryQualityDimension> = [
    "attribution_user",
    "attribution_org",
]

const CURRENT_SORT_FIELDS = new Set([
    "orderCount",
    "grossTotal",
    "customerNo",
    "label",
    "customerCount",
])

const HISTORY_SORT_FIELDS = new Set(["orderCount", "grossTotal", "label"])

/** 口径参数：非法值回退当前负责口径，不静默混用历史条件。 */
export function parseCaliber(raw: string | null): QualityCaliber {
    return raw === "history" ? "history" : "current"
}

/**
 * 逗号分隔稳定 ID：去重排序，空片段丢弃。
 * 姓名、超长片段不在此拒绝，由服务端返回结构化校验错误。
 */
export function parseCsvIds(raw: string | null): string[] {
    if (!raw) return []
    return [
        ...new Set(
            raw
                .split(",")
                .map((v) => v.trim())
                .filter(Boolean),
        ),
    ].sort()
}

/** 多选序列化：空列表返回空串（调用方不写 URL）。 */
export function serializeCsvIds(values: readonly string[]): string {
    return [...new Set(values.map((v) => v.trim()).filter(Boolean))].join(",")
}

export function parseCurrentDimension(
    raw: string | null,
): CurrentQualityDimension {
    if (raw === "customer" || raw === "owner_user" || raw === "owner_org") {
        return raw
    }
    return "customer"
}

export function parseHistoryDimension(
    raw: string | null,
): HistoryQualityDimension {
    if (raw === "attribution_user" || raw === "attribution_org") {
        return raw
    }
    return "attribution_user"
}

/** 两口径分组不可互换：历史分组名在当前口径下回退默认，反之亦然。 */
export function isCurrentDimension(raw: string | null): boolean {
    return (CURRENT_DIMENSIONS as ReadonlyArray<string>).includes(raw ?? "")
}

export function isHistoryDimension(raw: string | null): boolean {
    return (HISTORY_DIMENSIONS as ReadonlyArray<string>).includes(raw ?? "")
}

function parseSort(
    raw: string | null,
    fields: Set<string>,
    fallback: string,
): string {
    if (!raw) return fallback
    const [field, direction] = raw.split(":")
    if (!field || !fields.has(field)) return fallback
    if (direction !== "asc" && direction !== "desc") return fallback
    return `${field}:${direction}`
}

export function parseCurrentSort(raw: string | null): string {
    return parseSort(raw, CURRENT_SORT_FIELDS, "orderCount:desc")
}

export function parseHistorySort(raw: string | null): string {
    return parseSort(raw, HISTORY_SORT_FIELDS, "orderCount:desc")
}

/** 服务端页码从 1 开始；非法值回退第一页。 */
export function parseDualPage(raw: string | null): number {
    const value = Number(raw ?? "1")
    return Number.isFinite(value) && value >= 1 ? Math.floor(value) : 1
}

/** 只接受服务端页大小白名单，非法值回退 20。 */
export function parseDualPageSize(raw: string | null): number {
    const value = Number(raw ?? "20")
    return value === 50 || value === 100 ? value : 20
}

export function parseDualBoolean(raw: string | null): boolean | undefined {
    if (raw === "true") return true
    if (raw === "false") return false
    return undefined
}
