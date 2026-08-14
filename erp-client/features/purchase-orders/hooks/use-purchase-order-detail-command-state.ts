"use client"

import * as React from "react"

import { FormalCommandKeyLedger } from "@/lib/formal-command"

export type PurchaseOrderDetailResult = {
    status: "succeeded" | "rejected" | "blocked" | "unknown"
    title: string
    description: string
    reference?: string
    facts?: { label: string; value: React.ReactNode }[]
}

/**
 * 详情页正式命令身份账本与操作结果展示状态。
 * 账本随采购单切换重建，避免不同单据复用同一命令身份。
 */
export function usePurchaseOrderDetailCommandState(purchaseOrderId: string) {
    const commandLedgerRef = React.useRef<{
        purchaseOrderId: string
        ledger: FormalCommandKeyLedger
    } | null>(null)
    if (commandLedgerRef.current?.purchaseOrderId !== purchaseOrderId) {
        commandLedgerRef.current = {
            purchaseOrderId,
            ledger: new FormalCommandKeyLedger(),
        }
    }
    const commandLedger = commandLedgerRef.current.ledger
    const [result, setResult] =
        React.useState<PurchaseOrderDetailResult | null>(null)

    return { commandLedger, result, setResult }
}
