"use client"

import * as React from "react"

import type { PurchaseOrderListItem } from "@/features/purchase-orders/types"

export type UsePurchaseOrdersListKeyboardOptions = {
    pageRows: readonly PurchaseOrderListItem[]
    focusedIndex: number
    previewId: string | null
    createOpen: boolean
    onFocusIndex: React.Dispatch<React.SetStateAction<number>>
    onOpenPreview: (purchaseOrderId: string) => void
    onClosePreview: (purchaseOrderId: string) => void
}

/**
 * 列表键盘导航：j/k/↑/↓ 移动焦点行，Enter 打开预览，Escape 关闭预览，
 * / 聚焦搜索框。预览抽屉或建单弹框打开时后台列表不响应 j/k/Enter，避免状态污染。
 */
export function usePurchaseOrdersListKeyboard({
    pageRows,
    focusedIndex,
    previewId,
    createOpen,
    onFocusIndex,
    onOpenPreview,
    onClosePreview,
}: UsePurchaseOrdersListKeyboardOptions) {
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
                event.preventDefault()
                document
                    .querySelector<HTMLInputElement>(
                        '[data-slot="po-list-search"]',
                    )
                    ?.focus()
                return
            }

            if (previewId) {
                if (event.key === "Escape") {
                    event.preventDefault()
                    onClosePreview(previewId)
                }
                return
            }
            if (createOpen) return

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
                if (row) onOpenPreview(row.purchaseOrderId)
            }
        }
        window.addEventListener("keydown", onKeyDown)
        return () => window.removeEventListener("keydown", onKeyDown)
    }, [
        createOpen,
        focusedIndex,
        onClosePreview,
        onFocusIndex,
        onOpenPreview,
        pageRows,
        previewId,
    ])
}
