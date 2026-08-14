"use client"

import * as React from "react"
import { usePathname, useSearchParams } from "next/navigation"

import type { CustomerAccountPreviewTarget } from "@/features/customer-receivables/components/customer-account-columns"
import type { CustomerAccountsView } from "@/features/customer-receivables/types"
import type { CustomerReceivablesPatchUrl } from "./use-customer-receivables-url-state"

export function useCustomerReceivablesPreview(args: {
    view: CustomerAccountsView
    previewKind: "receivable" | "receipt" | "invoice" | null
    previewId: string | undefined
    focusId: string | undefined
    patchUrl: CustomerReceivablesPatchUrl
}): {
    preview: CustomerAccountPreviewTarget | null
    openPreview: (next: CustomerAccountPreviewTarget | null) => void
    closePreview: () => void
} {
    const { view, previewKind, previewId, focusId, patchUrl } = args
    const pathname = usePathname()
    const searchParams = useSearchParams()

    const [preview, setPreview] =
        React.useState<CustomerAccountPreviewTarget | null>(() =>
            previewKind && previewId
                ? { kind: previewKind, id: previewId }
                : focusId
                  ? { kind: "receivable", id: focusId }
                  : null,
        )

    const openPreview = React.useCallback(
        (next: CustomerAccountPreviewTarget | null) => {
            setPreview(next)
            if (next) {
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
        // eslint-disable-next-line react-hooks/exhaustive-deps
        [searchParams, pathname, view],
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
        // eslint-disable-next-line react-hooks/exhaustive-deps
    }, [searchParams, pathname, view])

    return { preview, openPreview, closePreview }
}
