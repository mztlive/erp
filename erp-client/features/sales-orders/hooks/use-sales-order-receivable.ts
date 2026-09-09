"use client"

import { useState } from "react"
import { useQuery } from "@tanstack/react-query"
import { useAccountProfileQuery } from "@/features/auth/queries"
import {
    customerReceivableKeys,
    useCustomerAccountsDetailQuery,
} from "@/features/customer-receivables/hooks/queries"
import type { CustomerAccountsDetailKind } from "@/features/customer-receivables/types"
import type { SalesOrderDetailView } from "@/features/sales-orders/api/sales-orders"
import {
    fetchOrderReceivables,
    fetchOrderReceipts,
    fetchOrderInvoices,
} from "@/features/sales-orders/api/sales-order-finance"
import { hasPermission } from "@/lib/permissions"

/** 本单只读票款查询；每类数据分别授权、分页、失败，不挂载登记命令。 */
export function useSalesOrderReceivable(order: SalesOrderDetailView) {
    const profile = useAccountProfileQuery()
    const granted = profile.data?.permissions
    const ready = Boolean(profile.data) && !profile.isError
    const canRead = (permission: string) =>
        ready && hasPermission(granted, permission)
    const [receiptPage, setReceiptPage] = useState(1)
    const [invoicePage, setInvoicePage] = useState(1)
    const [preview, setPreview] = useState<{
        kind: CustomerAccountsDetailKind
        id: string
    } | null>(null)
    const accounts = useQuery({
        staleTime: 0,
        queryKey: [
            ...customerReceivableKeys.all,
            "finance",
            order.id,
            "accounts",
            order.nature,
        ],
        queryFn: () => fetchOrderReceivables(order),
        enabled: canRead("receivable_account:list"),
    })
    const receipts = useQuery({
        staleTime: 0,
        queryKey: [
            ...customerReceivableKeys.all,
            "finance",
            order.id,
            "receipts",
            receiptPage,
        ],
        queryFn: () => fetchOrderReceipts(order, receiptPage),
        enabled: canRead("customer_receipt:list"),
    })
    const invoices = useQuery({
        staleTime: 0,
        queryKey: [
            ...customerReceivableKeys.all,
            "finance",
            order.id,
            "invoices",
            invoicePage,
        ],
        queryFn: () => fetchOrderInvoices(order, invoicePage),
        enabled: canRead("invoice:list"),
    })
    const detail = useCustomerAccountsDetailQuery(
        preview?.kind ?? null,
        preview?.id ?? null,
        true,
    )
    return {
        profile,
        canRead,
        accounts,
        receipts,
        invoices,
        receiptPage,
        invoicePage,
        setReceiptPage,
        setInvoicePage,
        preview,
        setPreview,
        detail,
    }
}
