"use client"

import * as React from "react"

type ReplaceParams = (patch: Record<string, string | null | undefined>) => void

/**
 * W23 列表搜索框状态：草稿与 URL 回填对齐、300ms 防抖写回 URL、`/` 聚焦。
 */
export function useExecutionProjectionSearch(options: {
    q: string
    replaceParams: ReplaceParams
}): {
    searchDraft: string
    setSearchDraft: React.Dispatch<React.SetStateAction<string>>
    searchInputRef: React.RefObject<HTMLInputElement | null>
} {
    const { q, replaceParams } = options

    const [searchDraft, setSearchDraft] = React.useState(q)
    const searchInputRef = React.useRef<HTMLInputElement | null>(null)
    React.useEffect(() => {
        // URL 回填时保留焦点保护：输入中不被 URL 旧值覆盖草稿
        const el = searchInputRef.current
        if (el && document.activeElement === el) return
        setSearchDraft(q)
    }, [q])

    // P3：搜索 300ms 防抖自动写 URL（replace），Enter 兜底，`/` 聚焦
    React.useEffect(() => {
        const handle = globalThis.setTimeout(() => {
            if (searchDraft.trim() === q) return
            replaceParams({ q: searchDraft.trim() || null, page: "1" })
        }, 300)
        return () => globalThis.clearTimeout(handle)
        // eslint-disable-next-line react-hooks/exhaustive-deps -- replaceParams 以当前 URL 快照为准
    }, [searchDraft])

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
            event.preventDefault()
            searchInputRef.current?.focus()
        }
        window.addEventListener("keydown", onKey)
        return () => window.removeEventListener("keydown", onKey)
    }, [])

    return { searchDraft, setSearchDraft, searchInputRef }
}
