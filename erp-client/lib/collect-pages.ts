import { apiGet } from "@/lib/api"
import type { Page } from "@/lib/api/paging"
import { createApiError } from "@/lib/api/errors"

const ENVELOPE_KEYS = [
    "empty_reason",
    "scope_version",
    "policy_version",
    "organization_version",
    "as_of",
    "scope_summary",
    "ownership_basis",
] as const

type FilterOption = { value: string; label: string }

type ListEnvelope = {
    empty_reason?: string | null
    scope_version?: string
    policy_version?: number
    organization_version?: number
    as_of?: string
    scope_summary?: string
    ownership_basis?: string
    owner_options?: FilterOption[]
    capability_owner_options?: FilterOption[]
    procurement_owner_options?: FilterOption[]
}

type CompletePage<T> = Page<T> & ListEnvelope

function scopeChangedError() {
    return createApiError({
        kind: "Http",
        status: 409,
        code: "DATA_SCOPE_CHANGED",
        message: "DATA_SCOPE_CHANGED：数据范围已变化，请重新查询。",
    })
}

function takeEnvelope(
    page: CompletePage<unknown>,
    seen: ListEnvelope,
): ListEnvelope {
    const next = { ...seen }
    for (const key of ENVELOPE_KEYS) {
        const value = page[key]
        if (value === undefined) continue
        // as_of 是每页查询的解析时点，不是授权版本；保留首页时点。
        if (
            key === "as_of" &&
            next.as_of !== undefined &&
            typeof seen.scope_version === "string" &&
            seen.scope_version === page.scope_version
        )
            continue
        const previous = next[key]
        if (previous !== undefined && previous !== value)
            throw scopeChangedError()
        Object.assign(next, { [key]: value })
    }
    next.owner_options = mergeOptions(seen.owner_options, page.owner_options)
    next.capability_owner_options = mergeOptions(
        seen.capability_owner_options,
        page.capability_owner_options,
    )
    next.procurement_owner_options = mergeOptions(
        seen.procurement_owner_options,
        page.procurement_owner_options,
    )
    return next
}

function mergeOptions(
    seen: readonly { value: string; label: string }[] | undefined,
    next: readonly { value: string; label: string }[] | undefined,
): { value: string; label: string }[] | undefined {
    if (!seen && !next) return undefined
    const labels = new Map<string, string>()
    for (const option of [...(seen ?? []), ...(next ?? [])]) {
        if (!option.value) continue
        labels.set(option.value, option.label)
    }
    return [...labels.entries()].map(([value, label]) => ({ value, label }))
}

/** 读取完整匹配集合；重复页、总数变化、空的中间页或范围版本变化使整次查询失败。 */
export async function fetchCompleteList<T>(
    path: string,
    query: Record<string, unknown> = {},
    keyOf: (item: T) => string = (item) => (item as { id: string }).id,
): Promise<{ items: T[]; total: number } & ListEnvelope> {
    const items = new Map<string, T>()
    let expectedTotal: number | undefined
    let envelope: ListEnvelope = {}
    for (let page = 1; ; page += 1) {
        const result = await apiGet<CompletePage<T>>(path, {
            ...query,
            page,
            page_size: 100,
            ...(typeof envelope.scope_version === "string"
                ? { scope_version: envelope.scope_version }
                : {}),
        })
        envelope = takeEnvelope(result, envelope)
        expectedTotal ??= result.total
        const before = items.size
        for (const item of result.items) {
            const key = keyOf(item)
            if (!key)
                throw createApiError({
                    kind: "Parse",
                    message: "列表缺少记录标识，请重新查询。",
                })
            items.set(key, item)
        }
        if (
            !Number.isSafeInteger(result.total) ||
            result.total < 0 ||
            result.total !== expectedTotal ||
            items.size > result.total ||
            items.size - before !== result.items.length ||
            (items.size === before && items.size < result.total)
        ) {
            throw createApiError({
                kind: "Parse",
                message: "列表数据已变化，请重新查询。",
            })
        }
        if (items.size === result.total)
            return {
                items: [...items.values()],
                total: result.total,
                ...envelope,
            }
    }
}
