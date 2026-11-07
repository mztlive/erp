"use client"

import * as React from "react"

import { useSearchHotkey } from "@/lib/use-search-hotkey"

/**
 * 搜索关键词草稿：URL 回填 + `/` 聚焦快捷键。
 *
 * 只维护 Draft 与输入框引用，不写 URL —— 提交统一走显式 apply
 * （Enter / 「查询」）。
 */
export function useCustomerQualitySearch({ qParam }: { qParam: string }) {
    const [searchDraft, setSearchDraft] = React.useState(qParam)
    const searchInputRef = React.useRef<HTMLInputElement | null>(null)

    // URL 回填搜索草稿（输入框聚焦时保留未提交的编辑）
    React.useEffect(() => {
        const el = searchInputRef.current
        if (el && document.activeElement === el) return
        setSearchDraft(qParam)
    }, [qParam])

    useSearchHotkey(searchInputRef)

    return { searchDraft, setSearchDraft, searchInputRef }
}
