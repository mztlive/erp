"use client"

import * as React from "react"

import { getErrorMessage } from "@/lib/api/errors"
import type { CustomerAccountsListView } from "@/features/customer-receivables/types"
import type { CustomerReceivablesPatchUrl } from "./use-customer-receivables-url-state"

/** 仅显式登记链接可自动进入核销；普通来源导航保留筛选和返回上下文。 */
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
            counterpartyPartyName?: string
            customerId?: string
            customerName?: string
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
        const fromSalesOrder =
            from === "W05" && Boolean(returnTo) && Boolean(registerMode)
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
        const receivable =
            data.receivables.find(
                (row) =>
                    row.counterpartyPartyId === party &&
                    (!receivableAccountId ||
                        row.accountId === receivableAccountId) &&
                    (!salesOrderId || row.salesOrderId === salesOrderId),
            ) ??
            data.receivables.find((row) => row.counterpartyPartyId === party)
        const counterparty = data.counterparties.find(
            (item) => item.counterpartyPartyId === party,
        )
        autoSessionRef.current = true
        void (async () => {
            try {
                const session = await createSession.mutateAsync({
                    mode: registerMode === "invoice" ? "invoice" : "receipt",
                    counterpartyPartyId: party,
                    counterpartyPartyName:
                        receivable?.counterpartyPartyName ??
                        counterparty?.counterpartyPartyName,
                    customerId:
                        customerId ??
                        receivable?.customerId ??
                        counterparty?.customerId,
                    customerName:
                        receivable?.customerName ?? counterparty?.customerName,
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
        counterpartyPartyId,
        customerId,
        data,
        from,
        receivableAccountId,
        registerMode,
        returnTo,
        salesOrderId,
        sessionId,
    ])
}
