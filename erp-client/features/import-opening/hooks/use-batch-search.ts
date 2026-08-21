"use client"

import * as React from "react"

/**
 * 批次关键词草稿：本地受控、不触发请求；URL 回填时保护正在编辑的焦点；
 * `/` 快捷键聚焦搜索框（Enter 由筛选表单统一提交，见 use-batch-list-filters）。
 */
export function useBatchSearchDraft(q: string) {
    const [qDraft, setQDraft] = React.useState(q)
    const searchInputRef = React.useRef<HTMLInputElement | null>(null)

    React.useEffect(() => {
        if (document.activeElement !== searchInputRef.current) {
            setQDraft(q)
        }
    }, [q])

    // `/` 聚焦搜索；忽略输入框、文本域与弹层场景（§10）
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
                target?.isContentEditable ||
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

    return { qDraft, setQDraft, searchInputRef }
}
