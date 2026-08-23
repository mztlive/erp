import * as React from "react"

import type { SalesOrderListItem } from "@/features/sales-orders/types"
import type { SalesOrdersUrlState } from "@/features/sales-orders/lib/url-state"

/**
 * 列表行键盘导航：`/` 聚焦搜索框，`j/k` 或方向键移动聚焦行，`Enter` 打开详情。
 * 筛选条件或行数变化时聚焦回到第一行。
 */
export function useSalesOrdersListKeyboardNav(options: {
    items: SalesOrderListItem[]
    url: SalesOrdersUrlState
    onRowNavigate: (id: string) => void
}) {
    const { items, url, onRowNavigate } = options
    const [focusedIndex, setFocusedIndex] = React.useState(0)

    React.useEffect(() => {
        setFocusedIndex(0)
    }, [
        url.closeStatus,
        url.collection,
        url.commercialStatus,
        url.contractId,
        url.createdBy,
        url.createdFrom,
        url.createdTo,
        url.customerId,
        url.fulfillment,
        url.invoice,
        url.nature,
        url.origin,
        url.page,
        url.reviewStatus,
        url.search,
        url.summary,
        items.length,
    ])

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
                if (event.key === "/" && target.tagName !== "INPUT") {
                    // allow
                } else if (event.key !== "Escape") {
                    return
                }
            }

            if (event.key === "/" && !event.metaKey && !event.ctrlKey) {
                // Dialog/Sheet/Combobox 弹层打开时不聚焦背景搜索框
                if (document.querySelector('[role="dialog"]:not([hidden])')) {
                    return
                }
                event.preventDefault()
                document
                    .querySelector<HTMLInputElement>(
                        '[data-slot="so-list-search"]',
                    )
                    ?.focus()
                return
            }

            if (items.length === 0) return

            if (event.key === "j" || event.key === "ArrowDown") {
                event.preventDefault()
                setFocusedIndex((i) => Math.min(items.length - 1, i + 1))
            } else if (event.key === "k" || event.key === "ArrowUp") {
                event.preventDefault()
                setFocusedIndex((i) => Math.max(0, i - 1))
            } else if (event.key === "Enter") {
                event.preventDefault()
                const row = items[focusedIndex]
                if (row) onRowNavigate(row.id)
            }
        }
        window.addEventListener("keydown", onKeyDown)
        return () => window.removeEventListener("keydown", onKeyDown)
    }, [focusedIndex, items, onRowNavigate])

    return { focusedIndex }
}
