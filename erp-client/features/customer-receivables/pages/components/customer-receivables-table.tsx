"use client"

import type { ColumnDef, PaginationState } from "@tanstack/react-table"

import { BusinessFailureState, MoneyValue } from "@/components/business"
import {
    ListDataTable,
    ListWorkSurface,
    type ListDataTableEmptyCopy,
    type ListDataTableProps,
} from "@/components/business/list-workspace"
import { Button } from "@/components/ui/button"
import type { CustomerAccountsLedgerListView } from "@/features/customer-receivables/api/list-view"
import type { CustomerAccountPreviewTarget } from "@/features/customer-receivables/components/column-types"
import {
    VIEW_LABEL,
    type CustomerAccountsView,
    type ReceiptRow,
    type ReceivableAccountRow,
    type SalesInvoiceRow,
} from "@/features/customer-receivables/types"

type TableKind = "receivable" | "receipt" | "invoice"
type TableState = Pick<
    ListDataTableProps<never>,
    | "loading"
    | "isError"
    | "error"
    | "onRetry"
    | "noScope"
    | "hasActiveFilters"
    | "onClearFilters"
>

type CustomerReceivablesTableProps = TableState & {
    view: CustomerAccountsView
    data: CustomerAccountsLedgerListView | undefined
    pagination: PaginationState
    onPaginationChange: (next: PaginationState) => void
    receivableColumns: ColumnDef<ReceivableAccountRow>[]
    receiptColumns: ColumnDef<ReceiptRow>[]
    invoiceColumns: ColumnDef<SalesInvoiceRow>[]
    preview: CustomerAccountPreviewTarget | null
    onPreview: (target: CustomerAccountPreviewTarget) => void
    canPreview: (kind: TableKind) => boolean
    toolbar: React.ReactNode
    embedded: boolean
}

const EMPTY_COPY: ListDataTableEmptyCopy = {
    filterTitle: "无匹配往来记录",
    emptyTitle: "当前范围尚无客户往来记录",
    emptyDescription: "可从销售单进入登记；登记后刷新查看。",
    noScopeTitle: "暂无可查看的客户往来数据",
    noScopeDescription:
        "您当前尚未配置客户往来的数据查看范围，请联系管理员开通相应权限后再查询。",
}
const PAGE_SIZE_OPTIONS = [20]
const PAIRED_PAGINATION = { pageIndex: 0, pageSize: PAGE_SIZE_OPTIONS[0] }
const RECEIVABLE_ORDER = [
    "party",
    "order",
    "open",
    "settled",
    "invoice",
    "due",
    "status",
]
const RECEIPT_ORDER = ["doc", "receivedAt", "amount", "alloc", "status"]
const INVOICE_ORDER = ["doc", "date", "gross", "alloc", "status"]

function LedgerTable<T>({
    kind,
    rows,
    preview,
    onPreview,
    canPreview,
    ...props
}: Omit<
    ListDataTableProps<T>,
    "data" | "highlightedRowId" | "onRowPreview" | "onRowOpen"
> & {
    kind: TableKind
    rows?: readonly T[]
    preview: CustomerAccountPreviewTarget | null
    onPreview: (target: CustomerAccountPreviewTarget) => void
    canPreview: boolean
    getRowId: (row: T) => string
}) {
    const open = canPreview
        ? (row: T) => onPreview({ kind, id: props.getRowId(row) })
        : undefined
    return (
        <ListDataTable
            {...props}
            data={[...(rows ?? [])]}
            layout="flush"
            pageSizeOptions={PAGE_SIZE_OPTIONS}
            onRowPreview={open}
            onRowOpen={open}
            highlightedRowId={preview?.kind === kind ? preview.id : undefined}
        />
    )
}

export function CustomerReceivablesTable({
    view,
    data,
    loading,
    isError,
    error,
    onRetry,
    noScope,
    hasActiveFilters,
    onClearFilters,
    pagination,
    onPaginationChange,
    receivableColumns,
    receiptColumns,
    invoiceColumns,
    preview,
    onPreview,
    canPreview,
    toolbar,
    embedded,
}: CustomerReceivablesTableProps) {
    const state = {
        loading,
        isError,
        error,
        onRetry,
        noScope,
        hasActiveFilters,
        onClearFilters,
    }
    const paged = {
        ...state,
        entityName: VIEW_LABEL[view],
        retryButtonId: "customer-receivables-list-retry",
        clearFiltersButtonId: "customer-receivables-empty-clear-filters",
        emptyCopy: EMPTY_COPY,
        scrollable: !embedded,
        className: !embedded
            ? "[&_[data-slot=data-table-surface]]:flex [&_[data-slot=data-table-surface]]:flex-col [&_[data-slot=table-container]]:min-h-0 [&_[data-slot=table-container]]:flex-1"
            : undefined,
        showPagination: Boolean(data && !isError && !noScope && data.total > 0),
        showColumnVisibility: !isError && !noScope,
        enableColumnResizing: !isError && !noScope,
        rowCount: data?.total ?? 0,
        pagination,
        onPaginationChange,
        preview,
        onPreview,
    }
    const renderReceivables = () => (
        <LedgerTable
            key="receivable"
            {...paged}
            kind="receivable"
            id="customer-receivables-list-receivable"
            caption="应收台账"
            rows={data?.receivables}
            columns={receivableColumns}
            defaultColumnVisibility={{ settled: false }}
            defaultColumnOrder={RECEIVABLE_ORDER}
            defaultColumnPinning={{ left: ["party"] }}
            getRowId={(row) => row.accountId}
            rowLabel={(row) =>
                `${row.counterpartyPartyName} · ${row.salesOrderNo} · 应收款项 ${row.accountSeq}`
            }
            canPreview={canPreview("receivable")}
        />
    )
    const renderReceipts = () => (
        <LedgerTable
            key="receipt"
            {...paged}
            kind="receipt"
            id="customer-receivables-list-receipt"
            caption="客户回款"
            rows={data?.receipts}
            columns={receiptColumns}
            defaultColumnOrder={RECEIPT_ORDER}
            defaultColumnPinning={{ left: ["doc"] }}
            getRowId={(row) => row.receiptId}
            rowLabel={(row) =>
                `${row.receiptNo} · ${row.counterpartyPartyName}`
            }
            canPreview={canPreview("receipt")}
        />
    )
    const renderInvoices = () => (
        <LedgerTable
            key="invoice"
            {...paged}
            kind="invoice"
            id="customer-receivables-list-invoice"
            caption="销项发票"
            rows={data?.invoices}
            columns={invoiceColumns}
            defaultColumnOrder={INVOICE_ORDER}
            defaultColumnPinning={{ left: ["doc"] }}
            getRowId={(row) => row.invoiceId}
            rowLabel={(row) =>
                `${row.invoiceNo} · ${row.counterpartyPartyName}`
            }
            canPreview={canPreview("invoice")}
        />
    )

    const unallocated = view === "unallocated"
    const sectionProps = {
        loading,
        hasActiveFilters,
        onClearFilters,
        preview,
        onPreview,
    }
    return (
        <ListWorkSurface
            ariaLabel={VIEW_LABEL[view]}
            className={embedded ? undefined : "min-h-64"}
            tableClassName={!embedded ? "flex flex-col" : undefined}
            toolbar={toolbar}
            table={
                unallocated ? (
                    isError ? (
                        <BusinessFailureState
                            idPrefix="customer-receivables-unallocated-failure"
                            title="客户往来加载失败"
                            error={error}
                            action={
                                <Button
                                    id="customer-receivables-list-retry"
                                    type="button"
                                    variant="outline"
                                    size="sm"
                                    onClick={onRetry}
                                >
                                    重试
                                </Button>
                            }
                        />
                    ) : (
                        <div className="min-h-0 flex-1 overflow-y-auto">
                            {data?.unallocated.note ? (
                                <p className="px-(--scaffold-px) py-2 text-xs text-muted-foreground">
                                    {data.unallocated.note}
                                </p>
                            ) : null}
                            <UnallocatedTable
                                {...sectionProps}
                                kind="receipt"
                                title="待分配回款"
                                amount={data?.metrics.unallocatedReceiptTotal}
                                id="customer-receivables-unallocated-receipts"
                                rows={data?.unallocated.receipts}
                                columns={receiptColumns}
                                defaultColumnOrder={RECEIPT_ORDER}
                                getRowId={(row) => row.receiptId}
                                rowLabel={(row) =>
                                    `${row.receiptNo} · ${row.counterpartyPartyName}`
                                }
                                noScope={data?.unallocated.receiptsNoScope}
                                clearFiltersButtonId="customer-receivables-empty-clear-filters"
                                emptyDescription="已确认且仍有未分配余额的回款将出现在此。"
                                canPreview={canPreview("receipt")}
                            />
                            <UnallocatedTable
                                {...sectionProps}
                                kind="invoice"
                                title="待分配销项发票"
                                amount={data?.metrics.unallocatedInvoiceTotal}
                                id="customer-receivables-unallocated-invoices"
                                rows={data?.unallocated.invoices}
                                columns={invoiceColumns}
                                defaultColumnOrder={INVOICE_ORDER}
                                getRowId={(row) => row.invoiceId}
                                rowLabel={(row) =>
                                    `${row.invoiceNo} · ${row.counterpartyPartyName}`
                                }
                                noScope={data?.unallocated.invoicesNoScope}
                                clearFiltersButtonId="customer-receivables-unallocated-invoices-empty-clear-filters"
                                emptyDescription="已登记蓝票且仍有未分配余额的发票将出现在此。"
                                canPreview={canPreview("invoice")}
                            />
                        </div>
                    )
                ) : view === "receivable" ? (
                    renderReceivables()
                ) : view === "receipt" ? (
                    renderReceipts()
                ) : (
                    renderInvoices()
                )
            }
        />
    )
}

function UnallocatedTable<T>({
    title,
    amount,
    emptyDescription,
    ...props
}: Omit<React.ComponentProps<typeof LedgerTable<T>>, "rowCount"> & {
    title: string
    amount?: string | null
    emptyDescription: string
}) {
    const rowCount = props.rows?.length ?? 0
    return (
        <ListWorkSurface
            ariaLabel={title}
            className="rounded-none border-0 border-t border-border/60"
            toolbar={
                <div className="flex flex-wrap items-baseline gap-x-3 gap-y-1">
                    <h3 className="text-body-compact font-medium">{title}</h3>
                    {amount !== undefined ? (
                        <span className="text-xs text-muted-foreground">
                            未分配{" "}
                            <MoneyValue value={amount} display="compact" />
                            {props.kind === "invoice" ? "（独立统计）" : null}
                        </span>
                    ) : null}
                </div>
            }
            table={
                <LedgerTable
                    {...props}
                    caption={title}
                    rowCount={rowCount}
                    pagination={PAIRED_PAGINATION}
                    showPagination={rowCount > 0}
                    showColumnVisibility={!props.noScope}
                    enableColumnResizing={!props.noScope}
                    defaultColumnPinning={{ left: ["doc"] }}
                    emptyCopy={{
                        noScopeTitle: `暂无${title}查看范围`,
                        filterTitle: `本页无匹配的${title}`,
                        emptyTitle: `本页无${title}`,
                        emptyDescription,
                    }}
                />
            }
        />
    )
}
