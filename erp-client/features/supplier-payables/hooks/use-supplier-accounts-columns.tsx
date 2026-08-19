"use client"

import * as React from "react"
import type { ColumnDef } from "@tanstack/react-table"

import type {
    PayableRow,
    PaymentRow,
    PurchaseInvoiceRow,
    ReverseTarget,
    SessionState,
    SupplierAccountsListView,
    SupplierRefundRequest,
    UnallocatedRow,
} from "@/features/supplier-payables/types"
import { buildPayableColumns } from "@/features/supplier-payables/lib/supplier-accounts-payable-columns"
import { buildPaymentColumns } from "@/features/supplier-payables/lib/supplier-accounts-payment-columns"
import { buildInvoiceColumns } from "@/features/supplier-payables/lib/supplier-accounts-invoice-columns"
import { buildUnallocatedColumns } from "@/features/supplier-payables/lib/supplier-accounts-unallocated-columns"

export function useSupplierAccountsColumns(input: {
    data: SupplierAccountsListView | undefined
    returnTo?: string
    fromWorkspace?: string
    openPreview: (payableAccountId: string) => void
    openPaymentPreview: (paymentId: string) => void
    openSession: (next: SessionState) => void
    setReverseTarget: React.Dispatch<React.SetStateAction<ReverseTarget | null>>
    setRedInvoiceNo: React.Dispatch<React.SetStateAction<string>>
    setRefundRequest?: React.Dispatch<
        React.SetStateAction<SupplierRefundRequest | null>
    >
}) {
    const {
        data,
        returnTo,
        fromWorkspace,
        openPreview,
        openPaymentPreview,
        openSession,
        setReverseTarget,
        setRedInvoiceNo,
        setRefundRequest,
    } = input

    const payableColumns = React.useMemo<ColumnDef<PayableRow>[]>(
        () =>
            buildPayableColumns({
                data,
                returnTo,
                fromWorkspace,
                openPreview,
                openSession,
            }),
        // eslint-disable-next-line react-hooks/exhaustive-deps
        [data?.canRegisterPayment, returnTo, fromWorkspace],
    )

    const paymentColumns = React.useMemo<ColumnDef<PaymentRow>[]>(
        () =>
            buildPaymentColumns({
                returnTo,
                fromWorkspace,
                openSession,
                openPaymentPreview,
                setReverseTarget,
                setRefundRequest,
            }),
        // eslint-disable-next-line react-hooks/exhaustive-deps
        [returnTo, fromWorkspace, setRefundRequest],
    )

    const invoiceColumns = React.useMemo<ColumnDef<PurchaseInvoiceRow>[]>(
        () => buildInvoiceColumns({ openSession, setReverseTarget, setRedInvoiceNo }),
        // eslint-disable-next-line react-hooks/exhaustive-deps
        [],
    )

    const unallocatedColumns = React.useMemo<ColumnDef<UnallocatedRow>[]>(
        () => buildUnallocatedColumns({ data, openSession }),
        // eslint-disable-next-line react-hooks/exhaustive-deps
        [data?.payments, data?.invoices],
    )

    return {
        payableColumns,
        paymentColumns,
        invoiceColumns,
        unallocatedColumns,
    }
}
