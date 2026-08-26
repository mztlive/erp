"use client"

import * as React from "react"

import { getErrorMessage } from "@/lib/api/errors"
import type { CustomerAccountsListView } from "@/features/customer-receivables/types"
import type { CustomerReceivablesPatchUrl } from "./use-customer-receivables-url-state"

/** W05 销售单或 W01 开票任务链入时自动打开受控核销会话。 */
export function useAutoAllocationSession(args: {
    data: CustomerAccountsListView | undefined
    from: string | undefined
    returnTo: string | undefined
    sessionId: string | undefined
    counterpartyPartyId: string | undefined
    customerId: string | undefined
    salesOrderId: string | undefined
    registerMode: "receipt" | "invoice" | undefined
    receivableAccountId: string | undefined
    canRegister?: boolean
    createSession: {
        mutateAsync: (input: {
            mode: "receipt" | "invoice"
            counterpartyPartyId: string
            salesOrderId?: string
            receivableAccountId?: string
            returnTo?: string
            from?: string
        }) => Promise<{ draftSessionId: string }>
    }
    patchUrl: CustomerReceivablesPatchUrl
    setActionError: React.Dispatch<React.SetStateAction<string | null>>
}): void {
    const {
        data,
        from,
        returnTo,
        sessionId,
        counterpartyPartyId,
        customerId,
        salesOrderId,
        registerMode,
        receivableAccountId,
        canRegister = true,
        createSession,
        patchUrl,
        setActionError,
    } = args
    const autoSessionRef = React.useRef(false)

    React.useEffect(() => {
        if (autoSessionRef.current || sessionId || !data) return
        const fromSalesOrder = from === "W05" && Boolean(returnTo)
        const fromInvoiceTask =
            from === "W01" &&
            registerMode === "invoice" &&
            Boolean(receivableAccountId)
        if (!fromSalesOrder && !fromInvoiceTask) return
        if (!data.canRegister || !canRegister) return
        const party =
            counterpartyPartyId ??
            data.receivables[0]?.counterpartyPartyId ??
            data.counterparties.find((c) => c.customerId === customerId)
                ?.counterpartyPartyId
        if (!party) return
        autoSessionRef.current = true
        void (async () => {
            try {
                const session = await createSession.mutateAsync({
                    mode: registerMode === "invoice" ? "invoice" : "receipt",
                    counterpartyPartyId: party,
                    salesOrderId,
                    receivableAccountId,
                    returnTo,
                    from,
                })
                patchUrl(
                    { sessionId: session.draftSessionId },
                    { replace: true },
                )
            } catch (err) {
                setActionError(getErrorMessage(err, "无法开始本次核销"))
            }
        })()
        // eslint-disable-next-line react-hooks/exhaustive-deps
    }, [
        canRegister,
        data,
        from,
        receivableAccountId,
        registerMode,
        returnTo,
        sessionId,
    ])
}
