"use client"

import * as React from "react"
import type { PayableRow, PurchaseInvoiceRow, UnallocatedRow } from "../types"
import { buildPayableColumns } from "../lib/supplier-accounts-payable-columns"
import { buildPaymentColumns } from "../lib/supplier-accounts-payment-columns"
import { buildInvoiceColumns } from "../lib/supplier-accounts-invoice-columns"
import { buildUnallocatedColumns } from "../lib/supplier-accounts-unallocated-columns"

export function useSupplierAccountsColumns(input: {
    canRegisterInvoice: boolean
    canPostAllocation: boolean
    openPreview: (payableAccountId: string) => void
    openPaymentPreview: (paymentId: string) => void
    openInvoicePreview: (invoiceId: string) => void
    openUnallocatedPreview: (id: string) => void
    openReversalPreview: (reversalId: string, paymentId: string) => void
    onRegisterInvoiceForPayable: (row: PayableRow) => void
    onContinueAllocateInvoice: (row: PurchaseInvoiceRow) => void
    onContinueAllocateUnallocatedInvoice: (row: UnallocatedRow) => void
}) {
    const {
        canRegisterInvoice,
        canPostAllocation,
        openPreview,
        openPaymentPreview,
        openInvoicePreview,
        openUnallocatedPreview,
        openReversalPreview,
        onRegisterInvoiceForPayable,
        onContinueAllocateInvoice,
        onContinueAllocateUnallocatedInvoice,
    } = input
    const payableColumns = React.useMemo(
        () =>
            buildPayableColumns({
                canRegisterInvoice,
                onOpenPreview: openPreview,
                onRegisterInvoice: onRegisterInvoiceForPayable,
            }),
        [canRegisterInvoice, onRegisterInvoiceForPayable, openPreview],
    )
    const paymentColumns = React.useMemo(
        () =>
            buildPaymentColumns({
                openPaymentPreview,
                openReversalPreview,
            }),
        [openPaymentPreview, openReversalPreview],
    )
    const invoiceColumns = React.useMemo(
        () =>
            buildInvoiceColumns({
                canPostAllocation,
                onOpenInvoicePreview: openInvoicePreview,
                onContinueAllocate: onContinueAllocateInvoice,
            }),
        [canPostAllocation, onContinueAllocateInvoice, openInvoicePreview],
    )
    const unallocatedColumns = React.useMemo(
        () =>
            buildUnallocatedColumns({
                canPostAllocation,
                onOpenUnallocatedPreview: openUnallocatedPreview,
                onContinueAllocateInvoice: onContinueAllocateUnallocatedInvoice,
            }),
        [
            canPostAllocation,
            onContinueAllocateUnallocatedInvoice,
            openUnallocatedPreview,
        ],
    )
    return {
        payableColumns,
        paymentColumns,
        invoiceColumns,
        unallocatedColumns,
    }
}
