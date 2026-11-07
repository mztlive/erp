"use client"

import type { RefObject } from "react"

import { useSearchDraft } from "@/lib/use-search-draft"

/**
 * 列表页搜索草稿（docs/ui-filter-design.md §5）：
 * 输入框受控于本地草稿，只有表单提交（applyFilters）会写 URL；
 * URL 回填时若搜索框正处于编辑状态，则不覆盖尚未提交的内容。
 */
export function useSupplierOrdersSearchDraft({
    q,
    searchInputRef,
}: {
    q: string | undefined
    searchInputRef: RefObject<HTMLInputElement | null>
}) {
    const { searchDraft, setSearchDraft } = useSearchDraft(q, {
        skipWhileFocused: true,
        inputRef: searchInputRef,
    })
    return { searchDraft, setSearchDraft }
}
