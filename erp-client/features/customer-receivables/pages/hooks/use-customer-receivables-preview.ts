"use client"

import * as React from "react"

import type { CustomerAccountPreviewTarget } from "@/features/customer-receivables/components/customer-account-columns"
import type { CustomerAccountsView } from "@/features/customer-receivables/types"
import type { CustomerReceivablesPatchUrl } from "./use-customer-receivables-url-state"

export function useCustomerReceivablesPreview(args: {
    view: CustomerAccountsView
    previewKind:
        | "receivable"
        | "receipt"
        | "invoice"
        | "refund"
        | "reversal"
        | null
    previewId: string | undefined
    focusId: string | undefined
    patchUrl: CustomerReceivablesPatchUrl
}): {
    preview: CustomerAccountPreviewTarget | null
    openPreview: (next: CustomerAccountPreviewTarget | null) => void
    closePreview: () => void
    restoreRowFocus: () => void
} {
    const { previewKind, previewId, focusId, patchUrl } = args
    const lastFocusedRow = React.useRef<HTMLElement | null>(null)
    const restoreRowFocus = React.useCallback(() => {
        if (lastFocusedRow.current?.isConnected) lastFocusedRow.current.focus()
    }, [])

    const [preview, setPreview] =
        React.useState<CustomerAccountPreviewTarget | null>(() =>
            previewKind && previewId
                ? { kind: previewKind, id: previewId }
                : focusId
                  ? { kind: "receivable", id: focusId }
                  : null,
        )

    React.useEffect(() => {
        setPreview(
            previewKind && previewId
                ? { kind: previewKind, id: previewId }
                : focusId
                  ? { kind: "receivable", id: focusId }
                  : null,
        )
    }, [previewKind, previewId, focusId])

    const openPreview = React.useCallback(
        (next: CustomerAccountPreviewTarget | null) => {
            setPreview(next)
            if (next) {
                lastFocusedRow.current = document.querySelector<HTMLElement>(
                    `[data-row-id="${CSS.escape(next.id)}"]`,
                )
                // 打开/关闭详情用 push（P2）；旧 focusId 一并清理
                patchUrl(
                    {
                        previewKind: next.kind,
                        previewId: next.id,
                        focusId: null,
                    },
                    { replace: false },
                )
            }
        },
        [patchUrl],
    )

    const closePreview = React.useCallback(() => {
        setPreview(null)
        patchUrl(
            {
                previewKind: null,
                previewId: null,
                focusId: null,
            },
            { replace: false },
        )
    }, [patchUrl])

    return { preview, openPreview, closePreview, restoreRowFocus }
}
