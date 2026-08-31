"use client"

import * as React from "react"

/** `/` 聚焦列表搜索框；弹窗 / 抽屉打开时不抢焦点。 */
export function useSlashSearchHotkey(
    searchInputRef: React.RefObject<HTMLInputElement | null>,
) {
    React.useEffect(() => {
        const onKey = (event: KeyboardEvent) => {
            if (
                event.key !== "/" ||
                event.target instanceof HTMLInputElement ||
                event.target instanceof HTMLTextAreaElement
            ) {
                return
            }
            if (
                document.querySelector('[role="dialog"], [data-slot="sheet"]')
            ) {
                return
            }
            event.preventDefault()
            searchInputRef.current?.focus()
        }
        window.addEventListener("keydown", onKey)
        return () => window.removeEventListener("keydown", onKey)
    }, [searchInputRef])
}
