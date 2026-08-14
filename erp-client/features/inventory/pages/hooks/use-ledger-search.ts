"use client"

import * as React from "react"

import type { LedgerPatchUrl } from "./use-inventory-ledger-url-state"

export interface LedgerSearchInput {
    qParam: string
    patchUrl: LedgerPatchUrl
}

export function useLedgerSearch({ qParam, patchUrl }: LedgerSearchInput) {
    const [searchInput, setSearchInput] = React.useState(qParam)
    const searchInputRef = React.useRef<HTMLInputElement | null>(null)

    // URL q 变化时同步输入框
    React.useEffect(() => {
        setSearchInput(qParam)
    }, [qParam])

    // `/` 聚焦列表搜索（输入框内输入时忽略）
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

    // 输入防抖 → URL
    React.useEffect(() => {
        const handle = globalThis.setTimeout(() => {
            if (searchInput === qParam) return
            patchUrl({ q: searchInput.trim() || null }, { replace: true })
        }, 300)
        return () => globalThis.clearTimeout(handle)
        // eslint-disable-next-line react-hooks/exhaustive-deps -- patchUrl 使用当前 URL 快照
    }, [searchInput])

    return { searchInput, setSearchInput, searchInputRef }
}
