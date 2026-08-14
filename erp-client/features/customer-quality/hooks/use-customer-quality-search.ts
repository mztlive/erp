"use client"

import * as React from "react"

import type { CustomerQualityPatch } from "./use-customer-quality-navigation-state"

export function useCustomerQualitySearch({
    qParam,
    patchUrl,
}: {
    qParam: string
    patchUrl: CustomerQualityPatch
}) {
    const [searchInput, setSearchInput] = React.useState(qParam)
    const searchInputRef = React.useRef<HTMLInputElement | null>(null)

    // P3：URL 回填搜索草稿（输入中不被旧值覆盖）；300ms 防抖自动写 URL；`/` 聚焦；Enter 兜底
    React.useEffect(() => {
        const el = searchInputRef.current
        if (el && document.activeElement === el) return
        setSearchInput(qParam)
    }, [qParam])

    React.useEffect(() => {
        const handle = globalThis.setTimeout(() => {
            if (searchInput.trim() === qParam) return
            patchUrl({ q: searchInput.trim() || null })
        }, 300)
        return () => globalThis.clearTimeout(handle)
        // eslint-disable-next-line react-hooks/exhaustive-deps -- patchUrl 以当前 URL 快照为准
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

    return { searchInput, setSearchInput, searchInputRef }
}
