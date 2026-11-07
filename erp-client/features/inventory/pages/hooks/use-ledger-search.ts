"use client"

import { useSearchDraft } from "@/lib/use-search-draft"
import { useSearchHotkey } from "@/lib/use-search-hotkey"

export interface LedgerSearchInput {
    qParam: string
}

/**
 * 库存台账搜索草稿（docs/ui-filter-design.md §5）：输入只改本地 Draft，
 * 不写 URL、不触发请求；提交由 useLedgerFilters 的统一 applyFilters 完成。
 * URL 回填同步 Draft（含聚焦时的历史导航与清除筛选），
 * 「/」聚焦搜索时忽略输入控件与打开的 Dialog / Sheet。
 */
export function useLedgerSearch({ qParam }: LedgerSearchInput) {
    const draft = useSearchDraft(qParam)
    useSearchHotkey(draft.searchInputRef)
    return draft
}
