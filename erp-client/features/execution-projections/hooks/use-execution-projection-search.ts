"use client"

import * as React from "react"

/**
 * W23 列表搜索框状态：草稿与 URL 回填对齐、`/` 聚焦。
 * 关键词通过整个筛选表单的显式提交写回 URL，输入过程不触发请求。
 */
export function useExecutionProjectionSearch(q: string): {
    searchDraft: string
    setSearchDraft: React.Dispatch<React.SetStateAction<string>>
    searchInputRef: React.RefObject<HTMLInputElement | null>
} {
    const [searchDraft, setSearchDraft] = React.useState(q)
    const searchInputRef = React.useRef<HTMLInputElement | null>(null)

    React.useEffect(() => {
        // URL 回填时保留焦点保护：输入中不被 URL 旧值覆盖草稿
        const el = searchInputRef.current
        if (el && document.activeElement === el) return
        setSearchDraft(q)
    }, [q])

    React.useEffect(() => {
        const onKey = (event: KeyboardEvent) => {
            if (
                event.key !== "/" ||
                event.metaKey ||
                event.ctrlKey ||
                event.altKey
            )
                return
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
            // 弹层 / 抽屉打开时不抢焦点
            if (document.querySelector('[role="dialog"], [data-slot="sheet"]')) {
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
