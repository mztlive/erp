"use client"

import * as React from "react"

/**
 * 搜索关键词草稿：URL 回填 + `/` 聚焦快捷键。
 *
 * 只维护 Draft 与输入框引用，不写 URL —— 提交统一走显式 apply
 * （收起态 Enter / 搜索框尾部提交箭头 / 展开态「应用全部筛选」）。
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

    // `/` 聚焦搜索；忽略输入框/文本域/可编辑区域与弹层场景
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
