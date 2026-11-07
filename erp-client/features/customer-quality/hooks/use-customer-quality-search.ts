"use client"

import { useSearchDraft } from "@/lib/use-search-draft"
import { useSearchHotkey } from "@/lib/use-search-hotkey"

/**
 * 搜索关键词草稿：URL 回填 + `/` 聚焦快捷键。
 *
 * 只维护 Draft 与输入框引用，不写 URL —— 提交统一走显式 apply
 * （Enter / 「查询」）。
 */
export function useCustomerQualitySearch({ qParam }: { qParam: string }) {
    const draft = useSearchDraft(qParam, { skipWhileFocused: true })
    useSearchHotkey(draft.searchInputRef)
    return draft
}
