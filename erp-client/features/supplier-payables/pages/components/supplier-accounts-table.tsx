"use client"

import * as React from "react"
import type { PaginationState, SortingState } from "@tanstack/react-table"

import {
    BusinessEmptyState,
    BusinessTableFrame,
    DataTable,
} from "@/components/business"
import { Button } from "@/components/ui/button"
import { useSupplierAccountsColumns } from "@/features/supplier-payables/hooks/use-supplier-accounts-columns"
import {
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

/** 工具条摘要去掉「N 条」计数：分页条已展示「共 N 条」，避免重复 */
function stripSummaryCount(summary: string): string {
    return summary.replace(/ · [\d,]+ 条$/, "")
}

export interface SupplierAccountsTableProps {
    view: SupplierAccountsView
    data: SupplierAccountsListView
    pageRows: readonly (
        | PayableRow
        | PaymentRow
        | PurchaseInvoiceRow
        | UnallocatedRow
    )[]
    unallocatedRowCount: number
    pagination: PaginationState
    onPaginationChange: (next: PaginationState) => void
    sorting: SortingState
    onSortingChange: (next: SortingState) => void
    onClearFilters: () => void
    returnTo: string | undefined
    fromWorkspace: string | undefined
    openPreview: (payableAccountId: string) => void
    openPaymentPreview: (paymentId: string) => void
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
    data,
    pageRows,
    unallocatedRowCount,
    pagination,
    onPaginationChange,
    sorting,
    onSortingChange,
    onClearFilters,
    returnTo,
    fromWorkspace,
    openPreview,
    openPaymentPreview,
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
        openPreview,
        openPaymentPreview,
        openSession,
        setReverseTarget,
        setRedInvoiceNo,
        setRefundRequest,
    })

    return (
        <BusinessTableFrame
            title={VIEW_LABEL[view]}
            description={`${stripSummaryCount(data.filterSummary)} · 金额与状态均来自系统最新数据；付款与进项票轨道独立。`}
            toolbar={toolbar}
            table={
                data.emptyReason === "FILTER_NO_RESULT" ? (
                    <BusinessEmptyState
                        kind="filter"
                        title="当前筛选无结果"
                        description={`没有符合「${stripSummaryCount(data.filterSummary)}」的记录。`}
                        className="rounded-lg border-0 bg-transparent p-6 shadow-none ring-0"
                        action={
                            <Button
                                type="button"
                                variant="secondary"
                                size="sm"
                                className="rounded-lg shadow-none"
                                onClick={onClearFilters}
                                title="清除全部筛选条件，保留当前视图与排序"
                            >
                                清除筛选
                            </Button>
                        }
                    />
                ) : data.emptyReason === "NO_DATA" ? (
                    <BusinessEmptyState
                        kind="no-data"
                        title="当前范围尚无供应商往来记录"
                        description="应付形成后刷新；可从采购单或结算来源进入。"
                        className="rounded-lg border-0 bg-transparent p-6 shadow-none ring-0"
                    />
                ) : (
                    <>
                        {view === "payable" ? (
                            <DataTable
                                columns={payableColumns}
                                data={pageRows as PayableRow[]}
                                getRowId={(r) => r.payableAccountId}
                                pagination={pagination}
                                onPaginationChange={onPaginationChange}
                                sorting={sorting}
                                onSortingChange={onSortingChange}
                                rowCount={data.payables.length}
                                layout="flush"
                            />
                        ) : null}
                        {view === "payment" ? (
                            <DataTable
                                columns={paymentColumns}
                                data={pageRows as PaymentRow[]}
                                getRowId={(r) => r.paymentId}
                                pagination={pagination}
                                onPaginationChange={onPaginationChange}
                                rowCount={data.payments.length}
                                layout="flush"
                            />
                        ) : null}
                        {view === "purchase_invoice" ? (
                            <DataTable
                                columns={invoiceColumns}
                                data={pageRows as PurchaseInvoiceRow[]}
                                getRowId={(r) => r.invoiceId}
                                pagination={pagination}
                                onPaginationChange={onPaginationChange}
                                rowCount={data.invoices.length}
                                layout="flush"
                            />
                        ) : null}
                        {view === "unallocated" ? (
                            <DataTable
                                columns={unallocatedColumns}
                                data={pageRows as UnallocatedRow[]}
                                getRowId={(r) => r.id}
                                pagination={pagination}
                                onPaginationChange={onPaginationChange}
                                rowCount={unallocatedRowCount}
                                layout="flush"
                            />
                        ) : null}
                    </>
                )
            }
        />
    )
}
