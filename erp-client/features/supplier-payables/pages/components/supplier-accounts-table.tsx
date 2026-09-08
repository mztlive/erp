"use client"

import * as React from "react"
import type { PaginationState, SortingState } from "@tanstack/react-table"

import {
    BusinessEmptyState,
    BusinessFailureState,
    DataTable,
} from "@/components/business"
import {
    ListWorkSurface,
    ListWorkspaceViews,
    listWorkspaceEmptyStateClassName,
} from "@/components/business/list-workspace"
import { Button } from "@/components/ui/button"
import { useSupplierAccountsColumns } from "@/features/supplier-payables/hooks/use-supplier-accounts-columns"
import {
    SUPPLIER_ACCOUNT_VIEWS,
    VIEW_LABEL,
    type PayableRow,
    type PaymentRow,
    type PurchaseInvoiceRow,
    type ReverseTarget,
    type SessionState,
    type SupplierAccountsListView,
    type SupplierAccountsView,
    type SupplierRefundRequest,
    type UnallocatedRow,
} from "@/features/supplier-payables/types"
import { toAutomationIdSegment } from "@/lib/automation-id"

export interface SupplierAccountsTableProps {
    view: SupplierAccountsView
    onViewChange: (view: SupplierAccountsView) => void
    data: SupplierAccountsListView | undefined
    pageRows: readonly (
        | PayableRow
        | PaymentRow
        | PurchaseInvoiceRow
        | UnallocatedRow
    )[]
    rowCount: number
    loading: boolean
    isError: boolean
    error: unknown
    onRetry: () => void
    pagination: PaginationState
    onPaginationChange: (next: PaginationState) => void
    sorting: SortingState
    onSortingChange: (next: SortingState) => void
    onClearFilters: () => void
    returnTo: string | undefined
    fromWorkspace: string | undefined
    paymentTaskPayableAccountId?: string
    openPreview: (payableAccountId: string) => void
    openPaymentPreview: (paymentId: string) => void
    openReversalPreview: (reversalId: string) => void
    openSession: (next: SessionState) => void
    setReverseTarget: React.Dispatch<React.SetStateAction<ReverseTarget | null>>
    setRedInvoiceNo: React.Dispatch<React.SetStateAction<string>>
    setRefundRequest?: React.Dispatch<
        React.SetStateAction<SupplierRefundRequest | null>
    >
    toolbar: React.ReactNode
}

export function SupplierAccountsTable({
    view,
    onViewChange,
    data,
    pageRows,
    rowCount,
    loading,
    isError,
    error,
    onRetry,
    pagination,
    onPaginationChange,
    sorting,
    onSortingChange,
    onClearFilters,
    returnTo,
    fromWorkspace,
    paymentTaskPayableAccountId,
    openPreview,
    openPaymentPreview,
    openReversalPreview,
    openSession,
    setReverseTarget,
    setRedInvoiceNo,
    setRefundRequest,
    toolbar,
}: SupplierAccountsTableProps) {
    const {
        payableColumns,
        paymentColumns,
        invoiceColumns,
        unallocatedColumns,
    } = useSupplierAccountsColumns({
        data,
        returnTo,
        fromWorkspace,
        paymentTaskPayableAccountId,
        openPreview,
        openPaymentPreview,
        openReversalPreview,
        openSession,
        setReverseTarget,
        setRedInvoiceNo,
        setRefundRequest,
    })

    return (
        <ListWorkSurface
            ariaLabel="供应商往来列表"
            views={
                <div id="supplier-payables-view-tabs">
                    <ListWorkspaceViews
                        ariaLabel="供应商往来工作视图"
                        hint="选择记录查看详情"
                        items={SUPPLIER_ACCOUNT_VIEWS.map((item) => ({
                            id: `supplier-payables-view-tabs-trigger-${toAutomationIdSegment(item)}`,
                            label: VIEW_LABEL[item],
                            count:
                                item === view
                                    ? rowCount.toLocaleString("zh-CN")
                                    : undefined,
                            active: item === view,
                            onClick: () => onViewChange(item),
                        }))}
                    />
                </div>
            }
            toolbar={toolbar}
            table={
                <SupplierAccountsTableBody
                    view={view}
                    data={data}
                    pageRows={pageRows}
                    rowCount={rowCount}
                    loading={loading}
                    isError={isError}
                    error={error}
                    onRetry={onRetry}
                    pagination={pagination}
                    onPaginationChange={onPaginationChange}
                    sorting={sorting}
                    onSortingChange={onSortingChange}
                    onClearFilters={onClearFilters}
                    payableColumns={payableColumns}
                    paymentColumns={paymentColumns}
                    invoiceColumns={invoiceColumns}
                    unallocatedColumns={unallocatedColumns}
                    openPreview={openPreview}
                    openPaymentPreview={openPaymentPreview}
                />
            }
        />
    )
}

function SupplierAccountsTableBody({
    view,
    data,
    pageRows,
    rowCount,
    loading,
    isError,
    error,
    onRetry,
    pagination,
    onPaginationChange,
    sorting,
    onSortingChange,
    onClearFilters,
    payableColumns,
    paymentColumns,
    invoiceColumns,
    unallocatedColumns,
    openPreview,
    openPaymentPreview,
}: Pick<
    SupplierAccountsTableProps,
    | "view"
    | "data"
    | "pageRows"
    | "rowCount"
    | "loading"
    | "isError"
    | "error"
    | "onRetry"
    | "pagination"
    | "onPaginationChange"
    | "sorting"
    | "onSortingChange"
    | "onClearFilters"
    | "openPreview"
    | "openPaymentPreview"
> & {
    payableColumns: ReturnType<
        typeof useSupplierAccountsColumns
    >["payableColumns"]
    paymentColumns: ReturnType<
        typeof useSupplierAccountsColumns
    >["paymentColumns"]
    invoiceColumns: ReturnType<
        typeof useSupplierAccountsColumns
    >["invoiceColumns"]
    unallocatedColumns: ReturnType<
        typeof useSupplierAccountsColumns
    >["unallocatedColumns"]
}) {
    if (isError && !data) {
        return (
            <BusinessFailureState
                title="供应商往来加载失败"
                error={error}
                onRetry={onRetry}
            />
        )
    }

    if (loading && !data) {
        return <div className="h-64 animate-pulse rounded-lg bg-muted" />
    }

    if (!data) return null

    if (data.emptyReason === "FILTER_NO_RESULT") {
        return (
            <BusinessEmptyState
                kind="filter"
                className={listWorkspaceEmptyStateClassName}
                title="当前筛选无结果"
                description="换一个关键词或清除筛选后再试。"
                action={
                    <Button
                        id="supplier-payables-table-clear-filters"
                        type="button"
                        variant="outline"
                        size="sm"
                        onClick={onClearFilters}
                        title="清除全部筛选条件，保留当前视图与排序"
                    >
                        清除筛选
                    </Button>
                }
            />
        )
    }

    if (data.emptyReason === "NO_DATA") {
        return (
            <BusinessEmptyState
                kind="no-data"
                className={listWorkspaceEmptyStateClassName}
                title="当前范围尚无供应商往来记录"
                description="应付形成后刷新；可从采购单或结算来源进入。"
            />
        )
    }

    if (view === "payable") {
        return (
            <DataTable
                id="supplier-payables-table-payable"
                columns={payableColumns}
                data={pageRows as PayableRow[]}
                getRowId={(r) => r.payableAccountId}
                pagination={pagination}
                onPaginationChange={onPaginationChange}
                sorting={sorting}
                onSortingChange={onSortingChange}
                rowCount={rowCount}
                layout="flush"
                loading={loading}
                onRowPreview={(row) => openPreview(row.payableAccountId)}
                rowLabel={(row) =>
                    `${row.supplierName} ${row.sourceDocumentNo}`
                }
            />
        )
    }

    if (view === "payment") {
        return (
            <DataTable
                id="supplier-payables-table-payment"
                columns={paymentColumns}
                defaultColumnVisibility={{ bank: false, reversal: false }}
                data={pageRows as PaymentRow[]}
                getRowId={(r) => r.paymentId}
                pagination={pagination}
                onPaginationChange={onPaginationChange}
                rowCount={rowCount}
                layout="flush"
                loading={loading}
                onRowPreview={(row) => openPaymentPreview(row.paymentId)}
                rowLabel={(row) => `${row.paymentNo} ${row.supplierName}`}
            />
        )
    }

    if (view === "purchase_invoice") {
        return (
            <DataTable
                id="supplier-payables-table-invoice"
                columns={invoiceColumns}
                data={pageRows as PurchaseInvoiceRow[]}
                getRowId={(r) => r.invoiceId}
                pagination={pagination}
                onPaginationChange={onPaginationChange}
                rowCount={rowCount}
                layout="flush"
                loading={loading}
            />
        )
    }

    return (
        <DataTable
            id="supplier-payables-table-unallocated"
            columns={unallocatedColumns}
            data={pageRows as UnallocatedRow[]}
            getRowId={(r) => r.id}
            pagination={pagination}
            onPaginationChange={onPaginationChange}
            rowCount={rowCount}
            layout="flush"
            loading={loading}
        />
    )
}
