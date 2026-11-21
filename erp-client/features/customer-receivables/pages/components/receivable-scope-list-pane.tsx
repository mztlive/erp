"use client"

import * as React from "react"

import { BusinessFailureState } from "@/components/business"
import {
    ListDataTable,
    ListWorkSurface,
    type ListDataTableProps,
} from "@/components/business/list-workspace"
import { Button } from "@/components/ui/button"
import type {
    ReceivableScopeDetail,
    ReceivableScopeListView,
} from "@/features/customer-receivables/api/scoped"
import {
    createScopedInvoiceColumns,
    createScopedReceiptColumns,
    createScopedReceivableColumns,
    scopedReceivableOrderNo,
} from "@/features/customer-receivables/components/scoped-columns"
import { ReceivableScopeSummary } from "@/features/customer-receivables/components/scoped-summary"
import type { useReceivableScopeUrlState } from "@/features/customer-receivables/pages/hooks/use-receivable-scope-url-state"
import { VIEW_LABEL } from "@/features/customer-receivables/types"

import { ReceivableScopeToolbar } from "./receivable-scope-toolbar"

type Props = {
    urlState: ReturnType<typeof useReceivableScopeUrlState>
    data: ReceivableScopeListView | undefined
    loading: boolean
    isError: boolean
    error: unknown
    placeholder: boolean
    onRetry: () => void
    onPreview?: (kind: ReceivableScopeDetail["kind"], id: string) => void
    previewId: string | undefined
}

const ORDER_PINNING = { left: ["order"] }
const DOC_PINNING = { left: ["doc"] }
const ORDER_COLUMNS = ["order", "visible", "whole", "status"]
const DOC_COLUMNS = ["doc", "visible", "whole", "status"]
const EMPTY_COPY = {
    noScopeTitle: "暂无可查看的客户往来数据",
    noScopeDescription:
        "您当前尚未配置客户往来的数据查看范围，请联系管理员开通相应权限后再查询。",
    filterTitle: "无匹配往来记录",
    filterDescription: "无匹配记录，可清除筛选后重试。",
    emptyTitle: "暂无客户往来记录",
}

function ScopeTable<T extends { id: string }>({
    rows,
    ...props
}: Omit<ListDataTableProps<T>, "data" | "getRowId"> & {
    rows?: readonly T[]
}) {
    const data = React.useMemo(() => (rows ? [...rows] : []), [rows])
    return <ListDataTable {...props} data={data} getRowId={(row) => row.id} />
}

/** 客户往来范围列表：汇总口径与当前视图明细同时保留。 */
export function ReceivableScopeListPane({
    urlState,
    data,
    loading,
    isError,
    error,
    placeholder,
    onRetry,
    onPreview,
    previewId,
}: Props) {
    const receivableColumns = React.useMemo(createScopedReceivableColumns, [])
    const receiptColumns = React.useMemo(createScopedReceiptColumns, [])
    const invoiceColumns = React.useMemo(createScopedInvoiceColumns, [])
    const { view, pagination, canReadView, hasActiveFilters, clearFilters } =
        urlState
    const denied = urlState.account.isSuccess && !canReadView
    const noScope = data?.emptyReason === "no_scope" || data?.hasScope === false
    const total = data?.total ?? 0
    const errorState = denied ? (
        <BusinessFailureState
            kind="permission"
            title={`无权查看${VIEW_LABEL[view]}`}
            description={
                urlState.visibleViews.length > 0
                    ? "请切换至可查看的往来类型，或联系管理员开通权限。"
                    : "请联系管理员开通客户往来查看权限。"
            }
        />
    ) : isError ? (
        <BusinessFailureState
            title="客户往来范围加载失败"
            error={error}
            action={
                <Button
                    id="customer-receivables-scope-list-retry"
                    type="button"
                    variant="outline"
                    size="sm"
                    onClick={onRetry}
                >
                    重试
                </Button>
            }
        />
    ) : undefined

    const tableProps = {
        caption: `${VIEW_LABEL[view]}范围查询结果`,
        rowCount: total,
        pagination,
        onPaginationChange: (next: typeof pagination) => {
            if (!placeholder && next.pageIndex !== pagination.pageIndex) {
                urlState.handlePaginationChange(next, data?.scopeVersion)
            }
        },
        pageSizeOptions: [pagination.pageSize],
        scrollable: true,
        showPagination: !noScope && !errorState && total > 0,
        loading,
        errorState,
        noScope,
        hasActiveFilters,
        onClearFilters: clearFilters,
        clearFiltersButtonId: "customer-receivables-scope-empty-clear-filters",
        emptyCopy: EMPTY_COPY,
        highlightedRowId: previewId,
    }

    const table =
        view === "receivable" ? (
            <ScopeTable
                {...tableProps}
                key={view}
                id="customer-receivables-scope-list-receivable"
                columns={receivableColumns}
                rows={noScope ? undefined : data?.receivables}
                defaultColumnOrder={ORDER_COLUMNS}
                defaultColumnPinning={ORDER_PINNING}
                rowLabel={(row) =>
                    `${scopedReceivableOrderNo(row)}，应收款项 ${row.account_seq}`
                }
                onRowPreview={
                    onPreview && ((row) => onPreview("receivable", row.id))
                }
            />
        ) : view === "receipt" ? (
            <ScopeTable
                {...tableProps}
                key={view}
                id="customer-receivables-scope-list-receipt"
                columns={receiptColumns}
                rows={noScope ? undefined : data?.receipts}
                defaultColumnOrder={DOC_COLUMNS}
                defaultColumnPinning={DOC_PINNING}
                rowLabel={(row) => `回款单 ${row.receipt_no}`}
                onRowPreview={
                    onPreview && ((row) => onPreview("receipt", row.id))
                }
            />
        ) : (
            <ScopeTable
                {...tableProps}
                key={view}
                id="customer-receivables-scope-list-invoice"
                columns={invoiceColumns}
                rows={noScope ? undefined : data?.invoices}
                defaultColumnOrder={DOC_COLUMNS}
                defaultColumnPinning={DOC_PINNING}
                rowLabel={(row) =>
                    row.invoice_no ? `发票 ${row.invoice_no}` : "附件登记发票"
                }
                onRowPreview={
                    onPreview && ((row) => onPreview("invoice", row.id))
                }
            />
        )

    return (
        <>
            {!errorState ? <ReceivableScopeSummary data={data} /> : null}
            <ListWorkSurface
                ariaLabel={tableProps.caption}
                toolbar={
                    canReadView && urlState.account.isSuccess ? (
                        <ReceivableScopeToolbar
                            urlState={urlState}
                            loading={loading}
                            failed={isError}
                            showEmptyClear={
                                !errorState &&
                                !loading &&
                                !!data &&
                                !noScope &&
                                total === 0 &&
                                hasActiveFilters
                            }
                        />
                    ) : undefined
                }
                table={
                    denied || !urlState.account.isSuccess ? errorState : table
                }
            />
        </>
    )
}
