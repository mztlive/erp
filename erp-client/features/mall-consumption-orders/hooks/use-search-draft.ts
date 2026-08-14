"use client"

import * as React from "react"

import type { ReplaceParamsPatch } from "./use-consumption-orders-url-state"

type SearchDraftParams = {
    qParam: string
    replaceParams: (patch: ReplaceParamsPatch, resetPage?: boolean) => void
}

/**
 * 搜索草稿状态：URL 是草稿的事实来源（输入聚焦时不覆盖），
 * 300ms 防抖自动写回 URL，Enter 立即提交，`/` 聚焦搜索框。
 */
export function useSearchDraft({ qParam, replaceParams }: SearchDraftParams) {
    const [searchInput, setSearchInput] = React.useState(qParam)
    const searchInputRef = React.useRef<HTMLInputElement | null>(null)

    React.useEffect(() => {
        // URL is source of truth for search draft；输入中不被 URL 旧值覆盖（焦点保护）
        const el = searchInputRef.current
        if (el && document.activeElement === el) return
        setSearchInput(qParam)
    }, [qParam])

    // P3：搜索 300ms 防抖自动写 URL（replace），Enter 兜底，`/` 聚焦
    React.useEffect(() => {
        const handle = globalThis.setTimeout(() => {
            if (searchInput.trim() === qParam) return
            replaceParams({ q: searchInput.trim() || undefined })
        }, 300)
        return () => globalThis.clearTimeout(handle)
        // eslint-disable-next-line react-hooks/exhaustive-deps -- replaceParams 以当前 URL 快照为准
    }, [searchInput])

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

    const commitSearch = () => {
        replaceParams({ q: searchInput.trim() || undefined })
    }

    return { searchInput, setSearchInput, searchInputRef, commitSearch }
}
