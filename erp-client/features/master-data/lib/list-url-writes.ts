type ListUrlPatch = Record<string, string | null>

type LifecycleStatus = "enabled" | "disabled" | "all"

/**
 * 把搜索草稿写入列表 URL。
 * 去掉首尾空白后与当前条件相同则不改地址，也不重置分页。
 */
export function commitTrimmedListSearch(input: {
    searchDraft: string
    appliedQuery: string
    patchUrl: (patch: ListUrlPatch) => void
    resetPagination: () => void
}) {
    const next = input.searchDraft.trim()
    if (next === input.appliedQuery.trim()) return
    input.patchUrl({ q: next || null, page: null })
    input.resetPagination()
}

/**
 * 切换启停筛选。选「全部」时从 URL 去掉启停和指标，并回到第一页。
 */
export function changeListLifecycle(input: {
    next: LifecycleStatus
    current: LifecycleStatus
    patchUrl: (patch: ListUrlPatch) => void
    resetPagination: () => void
}) {
    if (input.next === input.current) return
    input.patchUrl({
        lifecycleStatus: input.next === "all" ? null : input.next,
        metricKey: input.next === "all" ? null : input.next,
        page: null,
    })
    input.resetPagination()
}
