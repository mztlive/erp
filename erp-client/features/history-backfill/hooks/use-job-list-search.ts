"use client"

import * as React from "react"

import type { HistoryBackfillUrlState } from "@/features/history-backfill/lib/url-state"

/**
 * 任务列表搜索框：草稿态输入、300ms 防抖写 URL、「/」快捷键聚焦，
 * 以及外部 URL 变化时回写草稿态。
 */
export function useJobListSearch(
    urlState: HistoryBackfillUrlState,
    patchUrl: (patch: Partial<HistoryBackfillUrlState>) => void,
) {
    const [qDraft, setQDraft] = React.useState(urlState.q ?? "")
    const searchInputRef = React.useRef<HTMLInputElement | null>(null)

    React.useEffect(() => {
        setQDraft(urlState.q ?? "")
    }, [urlState.q])

    // P3 搜索：300ms 防抖写 URL，Enter 兜底，/ 聚焦
    React.useEffect(() => {
        const handle = globalThis.setTimeout(() => {
            if (qDraft.trim() === (urlState.q ?? "")) return
            patchUrl({ q: qDraft.trim() || undefined, page: 1 })
        }, 300)
        return () => globalThis.clearTimeout(handle)
        // eslint-disable-next-line react-hooks/exhaustive-deps -- patchUrl 以当前 URL 快照为准
    }, [qDraft])

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
            event.preventDefault()
            searchInputRef.current?.focus()
        }
        window.addEventListener("keydown", onKey)
        return () => window.removeEventListener("keydown", onKey)
    }, [])

    return { qDraft, setQDraft, searchInputRef }
}
