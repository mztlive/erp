"use client"

import * as React from "react"

import { getErrorMessage } from "@/lib/api/errors"
import type { CustomerAccountsListView } from "@/features/customer-receivables/types"
import type { CustomerReceivablesPatchUrl } from "./use-customer-receivables-url-state"

/** W05 链入：自动打开核销会话（有 salesOrderId + counterparty 或可推断） */
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
        if (from !== "W05" || !returnTo) return
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
    }, [data, from, returnTo, sessionId])
}
