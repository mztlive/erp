"use client"

import * as React from "react"

import type {
    SupplierOrdersUrlState,
    SupplierOrdersUrlUpdater,
} from "@/features/supplier-orders/lib/url-state"
import type { SupplierOrderListRow } from "@/features/supplier-orders/types"

/**
 * 列表键盘导航：
 * - focusedIndex 跟踪当前行（列表数据/筛选变化时归零）；
 * - rowRefs 供 Esc 关闭预览后焦点复位；
 * - 全局 keydown：/ 聚焦搜索、j/k 移动、Enter 开预览、Esc 关预览。
 */
export function useSupplierOrdersKeyboardNav({
    url,
    rows,
    updateUrl,
}: {
    url: SupplierOrdersUrlState
    rows: SupplierOrderListRow[]
    updateUrl: SupplierOrdersUrlUpdater
}) {
    const [focusedIndex, setFocusedIndex] = React.useState(0)
    const rowRefs = React.useRef<Map<string, HTMLElement>>(new Map())

    React.useEffect(() => {
        setFocusedIndex(0)
    }, [url.view, url.q, url.fulfillmentStatuses, url.page, rows.length])

    React.useEffect(() => {
        const onKeyDown = (event: KeyboardEvent) => {
            const target = event.target as HTMLElement | null
            if (
                target &&
                (target.tagName === "INPUT" ||
                    target.tagName === "TEXTAREA" ||
                    target.tagName === "SELECT" ||
                    target.isContentEditable)
            ) {
                if (event.key !== "Escape") return
            }

            if (event.key === "/" && !event.metaKey && !event.ctrlKey) {
                event.preventDefault()
                document
                    .querySelector<HTMLInputElement>(
                        '[data-slot="sfo-list-search"]',
                    )
                    ?.focus()
                return
            }

            if (rows.length === 0) return

            if (event.key === "j" || event.key === "ArrowDown") {
                event.preventDefault()
                setFocusedIndex((i) => Math.min(rows.length - 1, i + 1))
            } else if (event.key === "k" || event.key === "ArrowUp") {
                event.preventDefault()
                setFocusedIndex((i) => Math.max(0, i - 1))
            } else if (event.key === "Enter") {
                event.preventDefault()
                const row = rows[focusedIndex]
                if (row) updateUrl({ preview: row.orderId }, "push")
            } else if (event.key === "Escape" && url.preview) {
                event.preventDefault()
                const id = url.preview
                updateUrl({ preview: undefined }, "push")
                requestAnimationFrame(() => {
                    rowRefs.current.get(id)?.focus()
                })
            }
        }
        window.addEventListener("keydown", onKeyDown)
        return () => window.removeEventListener("keydown", onKeyDown)
    }, [focusedIndex, updateUrl, rows, url.preview])

    return { focusedIndex, rowRefs }
}
