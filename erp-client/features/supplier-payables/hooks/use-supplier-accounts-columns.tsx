"use client"

import * as React from "react"
import { buildPayableColumns } from "../lib/supplier-accounts-payable-columns"
import { buildPaymentColumns } from "../lib/supplier-accounts-payment-columns"
import { buildInvoiceColumns } from "../lib/supplier-accounts-invoice-columns"
import { buildUnallocatedColumns } from "../lib/supplier-accounts-unallocated-columns"

export function useSupplierAccountsColumns(input: {
    openReversalPreview: (reversalId: string) => void
}) {
    const { openReversalPreview } = input
    const payableColumns = React.useMemo(buildPayableColumns, [])
    const paymentColumns = React.useMemo(
        () => buildPaymentColumns({ openReversalPreview }),
        [openReversalPreview],
    )
    const invoiceColumns = React.useMemo(buildInvoiceColumns, [])
    const unallocatedColumns = React.useMemo(buildUnallocatedColumns, [])
    return {
        payableColumns,
        paymentColumns,
        invoiceColumns,
        unallocatedColumns,
    }
}
