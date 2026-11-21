"use client"

import type { ReactNode } from "react"
import type { PaginationState, SortingState } from "@tanstack/react-table"

import {
    ListDataTable,
    ListWorkSurface,
    type ListDataTableEmptyCopy,
} from "@/components/business/list-workspace"
import { useSupplierAccountsColumns } from "@/features/supplier-payables/hooks/use-supplier-accounts-columns"
import {
    VIEW_LABEL,
    type AllocationTrack,
    type PayableRow,
    type SupplierAccountsListView,
    type SupplierAccountsView,
} from "@/features/supplier-payables/types"

interface SupplierAccountsTableProps {
    view: SupplierAccountsView
    data: SupplierAccountsListView | undefined
    sortedPayables: readonly PayableRow[]
    track: AllocationTrack | "all"
    loading: boolean
    isError: boolean
    error: unknown
    onRetry: () => void
    pagination: PaginationState
    onPaginationChange: (next: PaginationState) => void
    sorting: SortingState
    onSortingChange: (next: SortingState) => void
    hasActiveFilters: boolean
    onClearFilters: () => void
    previewRowId: string | null
    openPreview: (payableAccountId: string) => void
    openPaymentPreview: (paymentId: string) => void
    openInvoicePreview: (invoiceId: string) => void
    openUnallocatedPreview: (id: string) => void
    openReversalPreview: (reversalId: string, paymentId: string) => void
    toolbar: ReactNode
}

const emptyCopy: ListDataTableEmptyCopy = {
    filterDescription: "换一个关键词或清除筛选后再试。",
    emptyTitle: "当前视图尚无供应商往来记录",
    emptyDescription: "应付形成后刷新；可从采购单或结算来源进入。",
}

export function SupplierAccountsTable({
    view,
    data,
    sortedPayables,
    track,
    loading,
    isError,
    error,
    onRetry,
    pagination,
    onPaginationChange,
    sorting,
    onSortingChange,
    hasActiveFilters,
    onClearFilters,
    previewRowId,
    openPreview,
    openPaymentPreview,
    openInvoicePreview,
    openUnallocatedPreview,
    openReversalPreview,
    toolbar,
}: SupplierAccountsTableProps) {
    const {
        payableColumns,
        paymentColumns,
        invoiceColumns,
        unallocatedColumns,
    } = useSupplierAccountsColumns({ openReversalPreview })
    const start = pagination.pageIndex * pagination.pageSize
    const end = start + pagination.pageSize
    const commonProps = {
        caption: VIEW_LABEL[view],
        entityName: VIEW_LABEL[view],
        pagination,
        onPaginationChange,
        loading,
        isError,
        error,
        onRetry,
        showColumnVisibility: toolbar != null,
        showPagination: !isError && (!loading || data != null),
        hasActiveFilters,
        onClearFilters,
        clearFiltersButtonId: "supplier-payables-table-clear-filters",
        emptyCopy,
        highlightedRowId: previewRowId ?? undefined,
    }

    let table: ReactNode
    if (view === "payable") {
        table = (
            <ListDataTable
                {...commonProps}
                key={view}
                id="supplier-payables-table-payable"
                columns={payableColumns}
                defaultColumnOrder={[
                    "supplier",
                    "openTotal",
                    "grossTotal",
                    "settledTotal",
                    "invoicedTotal",
                    "due",
                    "status",
                ]}
                defaultColumnPinning={{ left: ["supplier"] }}
                data={sortedPayables.slice(start, end)}
                rowCount={sortedPayables.length}
                getRowId={(row) => row.payableAccountId}
                sorting={sorting}
                onSortingChange={onSortingChange}
                onRowPreview={(row) => openPreview(row.payableAccountId)}
                rowLabel={(row) =>
                    `${row.supplierName} ${row.sourceDocumentNo}`
                }
            />
        )
    } else if (view === "payment") {
        const rows = data?.payments ?? []
        table = (
            <ListDataTable
                {...commonProps}
                key={view}
                id="supplier-payables-table-payment"
                columns={paymentColumns}
                defaultColumnOrder={[
                    "doc",
                    "amount",
                    "bank",
                    "reversal",
                    "status",
                    "time",
                ]}
                defaultColumnVisibility={{ bank: false, reversal: false }}
                defaultColumnPinning={{ left: ["doc"], right: ["reversal"] }}
                data={rows.slice(start, end)}
                rowCount={rows.length}
                getRowId={(row) => row.paymentId}
                onRowPreview={(row) => openPaymentPreview(row.paymentId)}
                rowLabel={(row) => `${row.paymentNo} ${row.supplierName}`}
            />
        )
    } else if (view === "purchase_invoice") {
        const rows = data?.invoices ?? []
        table = (
            <ListDataTable
                {...commonProps}
                key={view}
                id="supplier-payables-table-invoice"
                columns={invoiceColumns}
                defaultColumnOrder={["doc", "amount", "alloc", "status"]}
                defaultColumnPinning={{ left: ["doc"] }}
                data={rows.slice(start, end)}
                rowCount={rows.length}
                getRowId={(row) => row.invoiceId}
                onRowPreview={(row) => openInvoicePreview(row.invoiceId)}
                rowLabel={(row) => `${row.invoiceNo} ${row.supplierName}`}
            />
        )
    } else {
        const rows = (data?.unallocated ?? []).filter(
            (row) => track === "all" || row.track === track,
        )
        table = (
            <ListDataTable
                {...commonProps}
                key={view}
                id="supplier-payables-table-unallocated"
                columns={unallocatedColumns}
                defaultColumnOrder={["doc", "track", "amount"]}
                defaultColumnPinning={{ left: ["doc"] }}
                data={rows.slice(start, end)}
                rowCount={rows.length}
                getRowId={(row) => row.id}
                onRowPreview={(row) => openUnallocatedPreview(row.id)}
                rowLabel={(row) => `${row.documentNo} ${row.supplierName}`}
            />
        )
    }

    return (
        <ListWorkSurface
            ariaLabel={`${VIEW_LABEL[view]}列表`}
            toolbar={toolbar}
            table={table}
        />
    )
}
