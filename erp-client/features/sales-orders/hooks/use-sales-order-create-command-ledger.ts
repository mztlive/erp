"use client"

import * as React from "react"

import { FormalCommandKeyLedger } from "@/lib/formal-command"

/**
 * 页面生命周期内为当前销售单范围维护一份命令账本。
 * 账本不跨业务对象复用：scope（销售单 id）变化时重建。
 */
export function useSalesOrderCreateCommandLedger(
    scope: string,
    commandLedgerProp?: FormalCommandKeyLedger,
): FormalCommandKeyLedger {
    const ledgerRef = React.useRef<{
        scope: string
        ledger: FormalCommandKeyLedger
    } | null>(null)
    if (ledgerRef.current?.scope !== scope) {
        ledgerRef.current = {
            scope,
            ledger: new FormalCommandKeyLedger(),
        }
    }
    return commandLedgerProp ?? ledgerRef.current.ledger
}
