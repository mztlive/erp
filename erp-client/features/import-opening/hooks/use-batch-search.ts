"use client"

import { useSearchDraft } from "@/lib/use-search-draft"
import { useSearchHotkey } from "@/lib/use-search-hotkey"

/**
 * 批次关键词草稿：本地受控、不触发请求；URL 回填时保护正在编辑的焦点；
 * `/` 快捷键聚焦搜索框（Enter 由筛选表单统一提交，见 use-batch-list-filters）。
 */
export function useBatchSearchDraft(q: string) {
    const { searchDraft, setSearchDraft, searchInputRef } = useSearchDraft(q, {
        skipWhileFocused: true,
    })
    useSearchHotkey(searchInputRef)
    return { qDraft: searchDraft, setQDraft: setSearchDraft, searchInputRef }
}
