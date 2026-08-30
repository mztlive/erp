"use client"

import * as React from "react"
import { usePathname, useRouter, useSearchParams } from "next/navigation"

import {
    parsePreviewKind,
    parseWorkItemId,
    patchForViewChange,
} from "@/features/supplier-payables/lib/url-state"
import type {
    AllocationTrack,
    FormalSubmitResult,
    SessionState,
    SupplierAccountsListView,
} from "@/features/supplier-payables/types"
import type { SupplierAccountsPatchUrl } from "./use-supplier-accounts-filters"

type Options = {
    data: SupplierAccountsListView | undefined
    supplierId: string | undefined
    purchaseOrderId: string | undefined
    patchUrl: SupplierAccountsPatchUrl
}

/** W12 深链、核销会话和详情预览导航。 */
export function useSupplierAccountsNavigation({
    data,
    supplierId,
    purchaseOrderId,
    patchUrl,
}: Options) {
    const router = useRouter()
    const pathname = usePathname()
    const searchParams = useSearchParams()
    const fromWorkspace = searchParams.get("from") ?? undefined
    const returnTo = searchParams.get("returnTo") ?? undefined
    const sessionTrack = searchParams.get("session") as AllocationTrack | null
    const draftSessionId = searchParams.get("draftSessionId") ?? undefined
    const detailId = searchParams.get("detailId") ?? undefined
    const previewKind = parsePreviewKind(searchParams.get("previewKind"))
    const workItemId = parseWorkItemId(searchParams)
    const existingInvoiceId = searchParams.get("invoiceId") ?? undefined
    const previewPayableId =
        previewKind === "payable" && detailId ? detailId : null
    const previewPaymentId =
        previewKind === "payment" && detailId ? detailId : null
    const previewRefundId =
        previewKind === "refund" && detailId ? detailId : null
    const previewReversalId =
        previewKind === "reversal" && detailId ? detailId : null

    const [session, setSession] = React.useState<SessionState | null>(null)
    const [lastResult, setLastResult] =
        React.useState<FormalSubmitResult | null>(null)
    const deepLinkHandled = React.useRef(false)

    React.useEffect(() => {
        if (deepLinkHandled.current || !data?.moduleAllowed) return
        if (
            (sessionTrack === "payment" ||
                sessionTrack === "purchase_invoice") &&
            supplierId
        ) {
            deepLinkHandled.current = true
            setSession({
                track: sessionTrack,
                supplierId,
                draftSessionId,
                purchaseOrderId,
                returnTo,
                fromWorkspace,
                existingInvoiceId,
                preselectPayableAccountId:
                    previewKind === "payable" ? detailId : undefined,
            })
            return
        }
        if (
            fromWorkspace !== "W01" &&
            fromWorkspace !== "W08" &&
            fromWorkspace !== "W09"
        ) {
            return
        }
        if (!purchaseOrderId) return
        const payable = data.payables.find(
            (candidate) =>
                candidate.sourceType === "PURCHASE_ORDER" &&
                candidate.sourceDocumentId === purchaseOrderId,
        )
        const resolvedSupplierId = supplierId ?? payable?.supplierId
        if (!resolvedSupplierId) return

        deepLinkHandled.current = true
        setSession({
            track: "payment",
            supplierId: resolvedSupplierId,
            purchaseOrderId,
            returnTo,
            fromWorkspace,
            preselectPayableAccountId: payable?.payableAccountId,
        })
        patchUrl(
            { session: "payment", supplierId: resolvedSupplierId },
            { replace: true },
        )
    }, [
        data,
        detailId,
        draftSessionId,
        existingInvoiceId,
        fromWorkspace,
        patchUrl,
        previewKind,
        purchaseOrderId,
        returnTo,
        sessionTrack,
        supplierId,
    ])

    const openSession = React.useCallback(
        (next: SessionState) => {
            setLastResult(null)
            setSession(next)
            patchUrl(
                {
                    session: next.track,
                    supplierId: next.supplierId,
                    draftSessionId: next.draftSessionId ?? null,
                    invoiceId: next.existingInvoiceId ?? null,
                    detailId: null,
                },
                { replace: true },
            )
        },
        [patchUrl],
    )
    const closeSession = React.useCallback(() => {
        setSession(null)
        patchUrl(
            { session: null, draftSessionId: null, invoiceId: null },
            { replace: true },
        )
    }, [patchUrl])
    const syncSessionId = React.useCallback(
        (nextDraftSessionId: string) => {
            setSession((previous) => {
                if (
                    !previous ||
                    previous.draftSessionId === nextDraftSessionId
                ) {
                    return previous
                }
                return {
                    ...previous,
                    draftSessionId: nextDraftSessionId,
                }
            })
            patchUrl({ draftSessionId: nextDraftSessionId }, { replace: true })
        },
        [patchUrl],
    )

    const openPreview = React.useCallback(
        (payableAccountId: string) => {
            patchUrl(
                {
                    detailId: payableAccountId,
                    previewKind: "payable",
                },
                { replace: true },
            )
        },
        [patchUrl],
    )
    const openPaymentPreview = React.useCallback(
        (paymentId: string) => {
            patchUrl(
                {
                    ...patchForViewChange("payment"),
                    detailId: paymentId,
                    previewKind: "payment",
                },
                { replace: true },
            )
        },
        [patchUrl],
    )
    const openRefundPreview = React.useCallback(
        (refundId: string) => {
            patchUrl(
                { detailId: refundId, previewKind: "refund" },
                { replace: true },
            )
        },
        [patchUrl],
    )
    const openReversalPreview = React.useCallback(
        (reversalId: string) => {
            patchUrl(
                { detailId: reversalId, previewKind: "reversal" },
                { replace: true },
            )
        },
        [patchUrl],
    )
    const closePreview = React.useCallback(() => {
        patchUrl({ detailId: null, previewKind: null }, { replace: true })
    }, [patchUrl])

    const openSettlements = React.useCallback(() => {
        const currentQuery = searchParams.toString()
        const selfHref = currentQuery ? `${pathname}?${currentQuery}` : pathname
        const params = new URLSearchParams()
        if (supplierId) params.set("supplierId", supplierId)
        params.set("returnTo", selfHref)
        router.push(`/supplier-api/settlements?${params.toString()}`)
    }, [pathname, router, searchParams, supplierId])

    return {
        fromWorkspace,
        returnTo,
        previewPayableId,
        previewPaymentId,
        previewRefundId,
        previewReversalId,
        workItemId,
        openPreview,
        openPaymentPreview,
        openRefundPreview,
        openReversalPreview,
        closePreview,
        session,
        openSession,
        closeSession,
        syncSessionId,
        lastResult,
        setLastResult,
        openSettlements,
    }
}
