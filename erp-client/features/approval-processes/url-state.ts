import { createUrlStateCodec } from "@/lib/url-state"

import type { CatalogUrlState, DetailUrlState } from "./types"
import { APPROVAL_REQUIREMENTS, CONFIGURATION_STATUSES } from "./types"

const CATALOG_PARAM_KEYS = ["policy", "status", "q", "page"] as const
const DETAIL_PARAM_KEYS = ["view", "version"] as const

const catalogCodec = createUrlStateCodec<CatalogUrlState>([
    {
        key: "policy",
        type: "enum",
        values: [...APPROVAL_REQUIREMENTS, "ALL"],
        defaultValue: "ALL",
    },
    {
        key: "status",
        type: "enum",
        values: [...CONFIGURATION_STATUSES, "HAS_DRAFT", "ALL"],
        defaultValue: "ALL",
    },
    { key: "q", type: "string", trim: true },
    { key: "page", type: "number", defaultValue: 1, min: 1 },
])

const detailCodec = createUrlStateCodec<DetailUrlState>([
    {
        key: "view",
        type: "enum",
        values: ["current", "draft", "history"],
        defaultValue: "current",
    },
    { key: "version", type: "string" },
])

/**
 * 解析目录 URL。非法枚举回默认值。
 *
 * @param searchParams 查询串
 */
export const parseCatalogSearchParams = (
    searchParams:
        | ConstructorParameters<typeof URLSearchParams>[0]
        | URLSearchParams
        | { get(name: string): string | null },
): CatalogUrlState => {
    const parsed = catalogCodec.parse(searchParams as URLSearchParams)
    return { ...parsed, q: parsed.q ?? "" }
}

/**
 * 写回目录 URL。
 *
 * @param state 目录状态
 */
export const buildCatalogSearchParams = catalogCodec.build

/**
 * 解析详情 URL。
 *
 * @param searchParams 查询串
 */
export const parseDetailSearchParams = detailCodec.parse

/**
 * 写回详情 URL。
 *
 * @param state 详情状态
 */
export const buildDetailSearchParams = detailCodec.build

const isAllowedKey = (key: string, allowed: readonly string[]): boolean =>
    allowed.includes(key)

/**
 * 判断目录页是否出现未知查询参数。
 *
 * @param searchParams 当前 URL 查询
 */
export const hasUnknownCatalogParams = (
    searchParams: URLSearchParams,
): boolean => {
    for (const key of searchParams.keys()) {
        if (!isAllowedKey(key, CATALOG_PARAM_KEYS)) return true
    }
    return false
}

/**
 * 判断详情页是否出现未知查询参数。
 *
 * @param searchParams 当前 URL 查询
 */
export const hasUnknownDetailParams = (
    searchParams: URLSearchParams,
): boolean => {
    for (const key of searchParams.keys()) {
        if (!isAllowedKey(key, DETAIL_PARAM_KEYS)) return true
    }
    return false
}

/**
 * 目录行是否匹配当前筛选。
 *
 * @param item 目录行
 * @param state 筛选状态
 */
export const matchesCatalogFilters = (
    item: {
        document_type_label: string
        approval_requirement: CatalogUrlState["policy"] | string
        configuration_status: string
        draft_version: string | null
    },
    state: CatalogUrlState,
): boolean => {
    if (state.policy !== "ALL" && item.approval_requirement !== state.policy) {
        return false
    }
    if (state.status === "HAS_DRAFT") {
        if (!item.draft_version) return false
    } else if (
        state.status !== "ALL" &&
        item.configuration_status !== state.status
    ) {
        return false
    }
    const query = state.q.trim()
    if (!query) return true
    return item.document_type_label.includes(query)
}
