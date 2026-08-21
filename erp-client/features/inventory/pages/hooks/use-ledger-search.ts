"use client"

import * as React from "react"

export interface LedgerSearchInput {
    qParam: string
}

/**
 * 库存台账搜索草稿（docs/ui-filter-design.md §5）：输入只改本地 Draft，
 * 不写 URL、不触发请求；提交由 useLedgerFilters 的统一 applyFilters 完成。
 * URL 回填只同步 Draft（输入框聚焦时保护尚未提交的关键词），
 * 「/」聚焦搜索时忽略输入控件与打开的 Dialog / Sheet。
 */
export function useLedgerSearch({ qParam }: LedgerSearchInput) {
    const [searchDraft, setSearchDraft] = React.useState(qParam)
    const searchInputRef = React.useRef<HTMLInputElement | null>(null)

    // URL q 变化时同步输入框；clearAllFilters 直接清空草稿，不依赖此 effect
    React.useEffect(() => {
        if (document.activeElement !== searchInputRef.current) {
            setSearchDraft(qParam)
        }
    }, [qParam])

    // `/` 聚焦列表搜索（输入框内输入、Dialog/Sheet 打开时忽略）
    React.useEffect(() => {
        const onKey = (event: KeyboardEvent) => {
            if (
                event.key !== "/" ||
                event.metaKey ||
                event.ctrlKey ||
                event.altKey
            ) {
                return
            }
            const target = event.target as HTMLElement | null
            const tag = target?.tagName
            if (
                tag === "INPUT" ||
                tag === "TEXTAREA" ||
                tag === "SELECT" ||
                target?.isContentEditable
            ) {
                return
            }
            if (
                document.querySelector(
                    '[role="dialog"], [data-slot="sheet"]',
                )
            ) {
                return
            }
            event.preventDefault()
            searchInputRef.current?.focus()
        }
        window.addEventListener("keydown", onKey)
        return () => window.removeEventListener("keydown", onKey)
    }, [])

    return { searchDraft, setSearchDraft, searchInputRef }
}
