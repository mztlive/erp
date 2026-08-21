"use client"

import * as React from "react"

import type { PurchaseOrderListItem } from "@/features/purchase-orders/types"

export type UsePurchaseOrdersListKeyboardOptions = {
    pageRows: readonly PurchaseOrderListItem[]
    focusedIndex: number
    createOpen: boolean
    onFocusIndex: React.Dispatch<React.SetStateAction<number>>
    onOpenDetail: (purchaseOrderId: string) => void
}

/**
 * 列表键盘导航：j/k/↑/↓ 移动焦点行，Enter 打开详情，/ 聚焦搜索框。
 * / 聚焦忽略输入框、文本域、弹层（Dialog/Sheet）与建单弹框打开场景。
 */
export function usePurchaseOrdersListKeyboard({
    pageRows,
    focusedIndex,
    createOpen,
    onFocusIndex,
    onOpenDetail,
}: UsePurchaseOrdersListKeyboardOptions) {
    React.useEffect(() => {
        const onKeyDown = (event: KeyboardEvent) => {
            const target = event.target as HTMLElement | null
            const isTypingTarget =
                target != null &&
                (target.tagName === "INPUT" ||
                    target.tagName === "TEXTAREA" ||
                    target.tagName === "SELECT" ||
                    target.isContentEditable)

            if (event.key === "/" && !event.metaKey && !event.ctrlKey) {
                if (isTypingTarget || createOpen) return
                if (
                    document.querySelector(
                        '[role="dialog"], [data-slot="sheet"]',
                    )
                ) {
                    return
                }
                event.preventDefault()
                document
                    .querySelector<HTMLInputElement>(
                        '[data-slot="po-list-search"]',
                    )
                    ?.focus()
                return
            }

            if (isTypingTarget || createOpen) return

            if (pageRows.length === 0) return

            if (event.key === "j" || event.key === "ArrowDown") {
                event.preventDefault()
                onFocusIndex((i) => Math.min(pageRows.length - 1, i + 1))
            } else if (event.key === "k" || event.key === "ArrowUp") {
                event.preventDefault()
                onFocusIndex((i) => Math.max(0, i - 1))
            } else if (event.key === "Enter") {
                event.preventDefault()
                const row = pageRows[focusedIndex]
                if (row) onOpenDetail(row.purchaseOrderId)
            }
        }
        window.addEventListener("keydown", onKeyDown)
        return () => window.removeEventListener("keydown", onKeyDown)
    }, [createOpen, focusedIndex, onFocusIndex, onOpenDetail, pageRows])
}
