"use client"

import * as React from "react"

import type {
    SupplierOrdersUrlState,
    SupplierOrdersUrlUpdater,
} from "@/features/supplier-orders/lib/url-state"
import type { SupplierOrderListRow } from "@/features/supplier-orders/types"

export function useSupplierOrdersKeyboardNav({
    canOpenDetail,
    url,
    rows,
    searchInputRef,
    updateUrl,
}: {
    canOpenDetail: boolean
    url: SupplierOrdersUrlState
    rows: SupplierOrderListRow[]
    searchInputRef: React.RefObject<HTMLInputElement | null>
    updateUrl: SupplierOrdersUrlUpdater
}) {
    const [focusedIndex, setFocusedIndex] = React.useState(0)
    const rowRefs = React.useRef<Map<string, HTMLElement>>(new Map())

    const lastPreviewId = React.useRef<string | undefined>(url.preview)

    React.useLayoutEffect(() => {
        if (url.preview) lastPreviewId.current = url.preview
    }, [url.preview])

    const getRowFocusTarget = React.useCallback(
        (id: string) =>
            rowRefs.current.get(id) ??
            document
                .getElementById("supplier-orders-list-table")
                ?.querySelector<HTMLElement>(
                    `[data-row-id="${CSS.escape(id)}"]`,
                ),
        [],
    )

    const restorePreviewFocus = React.useCallback(() => {
        const id = lastPreviewId.current
        return (id ? getRowFocusTarget(id) : null) ?? searchInputRef.current
    }, [getRowFocusTarget, searchInputRef])

    const closePreview = React.useCallback(() => {
        updateUrl({ preview: undefined }, "push")
    }, [updateUrl])

    React.useEffect(() => {
        setFocusedIndex(0)
    }, [url.view, url.q, url.fulfillmentStatuses, url.page, rows.length])

    React.useEffect(() => {
        const onKeyDown = (event: KeyboardEvent) => {
            if (event.defaultPrevented || event.isComposing) return

            const target =
                event.target instanceof HTMLElement ? event.target : null
            const editing = target?.closest(
                'input, textarea, select, [contenteditable="true"]',
            )
            const overlayOpen = document.querySelector(
                '[role="dialog"], [role="alertdialog"], [role="menu"], [role="listbox"], [data-slot="popover-content"][data-open]',
            )
            if (overlayOpen || event.metaKey || event.ctrlKey || event.altKey)
                return

            if (event.key === "Escape" && url.preview && canOpenDetail) {
                event.preventDefault()
                closePreview()
                return
            }
            if (editing) return

            if (event.key === "/") {
                event.preventDefault()
                searchInputRef.current?.focus()
                return
            }
            if (!canOpenDetail || rows.length === 0) return

            const rowId =
                target?.closest<HTMLElement>("[data-row-id]")?.dataset.rowId
            const isIdentity =
                rowId != null && rowRefs.current.get(rowId) === target
            const interactive = target?.closest(
                'button, a, [role="button"], [role="combobox"], [role="checkbox"]',
            )
            const offset =
                event.key === "j" || event.key === "ArrowDown"
                    ? 1
                    : event.key === "k" || event.key === "ArrowUp"
                      ? -1
                      : 0

            if (offset && (!interactive || isIdentity)) {
                event.preventDefault()
                const rowIndex = rowId
                    ? rows.findIndex((row) => row.orderId === rowId)
                    : -1
                const nextIndex = Math.max(
                    0,
                    Math.min(
                        rows.length - 1,
                        (rowIndex >= 0 ? rowIndex : focusedIndex) + offset,
                    ),
                )
                setFocusedIndex(nextIndex)
                getRowFocusTarget(rows[nextIndex].orderId)?.focus()
            } else if (event.key === "Enter" && !interactive) {
                const row = rows[focusedIndex]
                if (!row) return
                event.preventDefault()
                updateUrl({ preview: row.orderId }, "push")
            }
        }
        window.addEventListener("keydown", onKeyDown)
        return () => window.removeEventListener("keydown", onKeyDown)
    }, [
        canOpenDetail,
        closePreview,
        getRowFocusTarget,
        focusedIndex,
        updateUrl,
        rows,
        searchInputRef,
        url.preview,
    ])

    return { focusedIndex, rowRefs, closePreview, restorePreviewFocus }
}
