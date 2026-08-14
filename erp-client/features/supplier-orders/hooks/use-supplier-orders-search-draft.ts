"use client"

import * as React from "react"

import type { SupplierOrdersUrlUpdater } from "@/features/supplier-orders/lib/url-state"

/**
 * 列表页搜索草稿：
 * 输入框受控于本地草稿，Enter / 失焦差异时提交到 URL，
 * URL 变化（外部清除、筛选重置）时回写草稿。
 */
export function useSupplierOrdersSearchDraft({
    q,
    updateUrl,
}: {
    q: string | undefined
    updateUrl: SupplierOrdersUrlUpdater
}) {
    const [searchDraft, setSearchDraft] = React.useState(q ?? "")

    React.useEffect(() => {
        setSearchDraft(q ?? "")
    }, [q])

    const commitSearch = React.useCallback(
        (draft: string) => updateUrl({ q: draft || undefined, page: 1 }),
        [updateUrl],
    )

    const commitOnBlur = React.useCallback(() => {
        if ((q ?? "") !== searchDraft) commitSearch(searchDraft)
    }, [q, searchDraft, commitSearch])

    return { searchDraft, setSearchDraft, commitSearch, commitOnBlur }
}
