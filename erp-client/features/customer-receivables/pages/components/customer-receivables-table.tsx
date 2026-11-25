"use client"

import * as React from "react"
import type { ColumnDef, PaginationState } from "@tanstack/react-table"

import {
    BusinessFailureState,
    BusinessStatusBadge,
    DetailWorkspaceSubTabs,
    MoneyValue,
    TableIdentityCell,
    TableRowActions,
    TableTextCell,
} from "@/components/business"
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
import { toAutomationIdSegment } from "@/lib/automation-id"
import { formatDateTime } from "@/lib/datetime"

type TableKind = "receivable" | "receipt" | "invoice"
type UnallocatedKindFilter = "all" | "receipt" | "invoice"

type UnifiedUnallocatedRow =
    | {
          key: string
          kind: "receipt"
          id: string
          docNo: string
          counterpartyPartyName: string
          dateLabel: string
          totalAmount: string
          unallocatedAmount: string
          statusLabel: string
          statusTone: ReceiptRow["statusTone"]
          receipt: ReceiptRow
      }
    | {
          key: string
          kind: "invoice"
          id: string
          docNo: string
          counterpartyPartyName: string
          dateLabel: string
          totalAmount: string
          unallocatedAmount: string
          statusLabel: string
          statusTone: SalesInvoiceRow["statusTone"]
          invoice: SalesInvoiceRow
      }

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
    onContinueAllocateReceipt?: (row: ReceiptRow) => void
    canRegisterReceipt?: boolean
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
const PAGE_SIZE_OPTIONS = [20, 50, 100]
const RECEIVABLE_ORDER = [
    "party",
    "order",
    "open",
    "gross",
    "settled",
    "openInvoiceable",
    "invoiced",
    "due",
    "status",
    "actions",
]
const RECEIPT_ORDER = [
    "doc",
    "receivedAt",
    "amount",
    "alloc",
    "status",
    "actions",
]
const INVOICE_ORDER = ["doc", "date", "gross", "alloc", "status", "actions"]
const UNALLOCATED_ORDER = [
    "doc",
    "kind",
    "date",
    "unallocated",
    "status",
    "actions",
]

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
    onContinueAllocateReceipt,
    canRegisterReceipt = false,
    canPreview,
    toolbar,
    embedded,
}: CustomerReceivablesTableProps) {
    const [unallocatedKind, setUnallocatedKind] =
        React.useState<UnallocatedKindFilter>("all")
    const [unallocatedPagination, setUnallocatedPagination] =
        React.useState<PaginationState>({ pageIndex: 0, pageSize: 20 })

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

    const unifiedUnallocatedRows = React.useMemo<
        UnifiedUnallocatedRow[]
    >(() => {
        if (view !== "unallocated" || !data) return []
        const receiptRows: UnifiedUnallocatedRow[] =
            unallocatedKind === "invoice"
                ? []
                : data.unallocated.receipts.map((receipt) => ({
                      key: `receipt:${receipt.receiptId}`,
                      kind: "receipt",
                      id: receipt.receiptId,
                      docNo: receipt.receiptNo,
                      counterpartyPartyName: receipt.counterpartyPartyName,
                      dateLabel: receipt.receivedAt
                          ? formatDateTime(
                                receipt.receivedAt,
                                "full",
                                "passthrough",
                            )
                          : "—",
                      totalAmount: receipt.amount,
                      unallocatedAmount: receipt.unallocatedAmount,
                      statusLabel: receipt.statusLabel,
                      statusTone: receipt.statusTone,
                      receipt,
                  }))
        const invoiceRows: UnifiedUnallocatedRow[] =
            unallocatedKind === "receipt"
                ? []
                : data.unallocated.invoices.map((invoice) => ({
                      key: `invoice:${invoice.invoiceId}`,
                      kind: "invoice",
                      id: invoice.invoiceId,
                      docNo: invoice.invoiceNo,
                      counterpartyPartyName: invoice.counterpartyPartyName,
                      dateLabel: invoice.invoiceDate || "—",
                      totalAmount: invoice.grossAmount,
                      unallocatedAmount: invoice.unallocatedAmount,
                      statusLabel: invoice.statusLabel,
                      statusTone: invoice.statusTone,
                      invoice,
                  }))
        return [...receiptRows, ...invoiceRows]
    }, [data, unallocatedKind, view])

    const unallocatedColumns = React.useMemo<
        ColumnDef<UnifiedUnallocatedRow>[]
    >(
        () => [
            {
                id: "doc",
                header: "单据编号 / 往来主体",
                meta: { label: "单据", width: "flex" },
                cell: ({ row: { original } }) => (
                    <TableIdentityCell
                        primary={
                            <TableTextCell value={original.docNo} numeric />
                        }
                        primaryTitle={original.docNo}
                        secondary={original.counterpartyPartyName}
                        secondaryTitle={original.counterpartyPartyName}
                    />
                ),
            },
            {
                id: "kind",
                header: "单据类型",
                meta: { label: "单据类型", width: "status" },
                cell: ({ row: { original } }) => (
                    <BusinessStatusBadge
                        context="list"
                        label={
                            original.kind === "receipt"
                                ? "客户回款"
                                : "销项发票"
                        }
                        tone={original.kind === "receipt" ? "warning" : "info"}
                    />
                ),
            },
            {
                id: "date",
                header: "发生日期",
                meta: { label: "发生日期", width: "default" },
                cell: ({ row: { original } }) => (
                    <TableTextCell value={original.dateLabel} numeric />
                ),
            },
            {
                id: "unallocated",
                header: "待分配余额",
                meta: {
                    label: "待分配余额",
                    width: "amount",
                    align: "end",
                    numeric: true,
                },
                cell: ({ row: { original } }) => (
                    <div className="flex flex-col items-end">
                        <MoneyValue
                            display="exact"
                            value={original.unallocatedAmount}
                        />
                        <span className="text-xs text-muted-foreground">
                            原单{" "}
                            <MoneyValue
                                display="exact"
                                value={original.totalAmount}
                            />
                        </span>
                    </div>
                ),
            },
            {
                id: "status",
                header: "状态",
                meta: { label: "状态", width: "status" },
                cell: ({ row: { original } }) => (
                    <BusinessStatusBadge
                        context="list"
                        label={original.statusLabel}
                        tone={original.statusTone}
                        appearance="quiet"
                    />
                ),
            },
            {
                id: "actions",
                header: "操作",
                enableHiding: false,
                meta: { label: "操作", width: "status", align: "end" },
                cell: ({ row: { original } }) => {
                    const seg = toAutomationIdSegment(original.id)
                    const canContinue =
                        original.kind === "receipt" &&
                        canRegisterReceipt &&
                        Boolean(onContinueAllocateReceipt) &&
                        original.receipt.allowedActions.includes(
                            "CONTINUE_ALLOCATE",
                        )
                    return (
                        <TableRowActions
                            maxInline={2}
                            moreId={`customer-receivables-unallocated-${seg}-more`}
                            moreLabel={`${original.docNo} 更多操作`}
                            actions={[
                                ...(canContinue
                                    ? [
                                          {
                                              id: `customer-receivables-unallocated-${seg}-allocate`,
                                              label: "继续核销",
                                              onClick: () =>
                                                  onContinueAllocateReceipt?.(
                                                      original.receipt,
                                                  ),
                                          },
                                      ]
                                    : []),
                                ...(canPreview(original.kind)
                                    ? [
                                          {
                                              id: `customer-receivables-unallocated-${seg}-view`,
                                              label: "详情",
                                              onClick: () =>
                                                  onPreview({
                                                      kind: original.kind,
                                                      id: original.id,
                                                  }),
                                          },
                                      ]
                                    : []),
                            ]}
                        />
                    )
                },
            },
        ],
        [canPreview, canRegisterReceipt, onContinueAllocateReceipt, onPreview],
    )

    const renderReceivables = () => (
        <LedgerTable
            key="receivable"
            {...paged}
            kind="receivable"
            id="customer-receivables-list-receivable"
            caption="应收台账"
            rows={data?.receivables}
            columns={receivableColumns}
            defaultColumnVisibility={{ invoiced: false }}
            defaultColumnOrder={RECEIVABLE_ORDER}
            defaultColumnPinning={{ left: ["party"], right: ["actions"] }}
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
            defaultColumnPinning={{ left: ["doc"], right: ["actions"] }}
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
            defaultColumnPinning={{ left: ["doc"], right: ["actions"] }}
            getRowId={(row) => row.invoiceId}
            rowLabel={(row) =>
                `${row.invoiceNo} · ${row.counterpartyPartyName}`
            }
            canPreview={canPreview("invoice")}
        />
    )

    const unallocated = view === "unallocated"
    const unallocatedStart =
        unallocatedPagination.pageIndex * unallocatedPagination.pageSize
    const unallocatedPageRows = unifiedUnallocatedRows.slice(
        unallocatedStart,
        unallocatedStart + unallocatedPagination.pageSize,
    )
    const receiptCount = data?.unallocated.receipts.length ?? 0
    const invoiceCount = data?.unallocated.invoices.length ?? 0

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
                        <>
                            <div className="flex flex-wrap items-center justify-between gap-3 border-b border-border/60 bg-card px-(--scaffold-px) py-2">
                                <DetailWorkspaceSubTabs
                                    className="pt-0"
                                    value={unallocatedKind}
                                    onValueChange={(next) => {
                                        setUnallocatedKind(
                                            next as UnallocatedKindFilter,
                                        )
                                        setUnallocatedPagination((prev) => ({
                                            ...prev,
                                            pageIndex: 0,
                                        }))
                                    }}
                                    idPrefix="customer-receivables-unallocated-subtab"
                                    items={[
                                        {
                                            id: "all",
                                            label: `全部 (${receiptCount + invoiceCount})`,
                                        },
                                        {
                                            id: "receipt",
                                            label: `待分配回款 (${receiptCount})`,
                                        },
                                        {
                                            id: "invoice",
                                            label: `待分配销项发票 (${invoiceCount})`,
                                        },
                                    ]}
                                />
                                {data ? (
                                    <div className="flex flex-wrap items-center gap-x-4 gap-y-1 text-xs text-muted-foreground">
                                        <span>
                                            回款未分配{" "}
                                            <MoneyValue
                                                value={
                                                    data.metrics
                                                        .unallocatedReceiptTotal
                                                }
                                                display="compact"
                                            />
                                        </span>
                                        <span>
                                            发票未分配{" "}
                                            <MoneyValue
                                                value={
                                                    data.metrics
                                                        .unallocatedInvoiceTotal
                                                }
                                                display="compact"
                                            />
                                        </span>
                                    </div>
                                ) : null}
                            </div>
                            <ListDataTable
                                key="unallocated"
                                id="customer-receivables-list-unallocated"
                                caption="待分配回款与销项发票"
                                entityName="待分配记录"
                                data={unallocatedPageRows}
                                columns={unallocatedColumns}
                                defaultColumnOrder={UNALLOCATED_ORDER}
                                defaultColumnPinning={{
                                    left: ["doc"],
                                    right: ["actions"],
                                }}
                                getRowId={(row) => row.key}
                                rowLabel={(row) =>
                                    `${row.docNo} · ${row.counterpartyPartyName}`
                                }
                                rowCount={unifiedUnallocatedRows.length}
                                pagination={unallocatedPagination}
                                pageSizeOptions={PAGE_SIZE_OPTIONS}
                                onPaginationChange={setUnallocatedPagination}
                                showPagination={
                                    unifiedUnallocatedRows.length > 0
                                }
                                showColumnVisibility={!noScope}
                                enableColumnResizing={!noScope}
                                scrollable={!embedded}
                                className={
                                    !embedded
                                        ? "[&_[data-slot=data-table-surface]]:flex [&_[data-slot=data-table-surface]]:flex-col [&_[data-slot=table-container]]:min-h-0 [&_[data-slot=table-container]]:flex-1"
                                        : undefined
                                }
                                loading={loading}
                                noScope={noScope}
                                hasActiveFilters={hasActiveFilters}
                                onClearFilters={onClearFilters}
                                clearFiltersButtonId="customer-receivables-empty-clear-filters"
                                emptyCopy={{
                                    noScopeTitle: "暂无待分配记录查看范围",
                                    filterTitle: "无匹配的待分配记录",
                                    emptyTitle: "当前无待分配回款或销项发票",
                                    emptyDescription:
                                        "已确认且仍有未分配余额的回款与蓝票将出现在此。",
                                }}
                                highlightedRowId={
                                    preview
                                        ? `${preview.kind}:${preview.id}`
                                        : undefined
                                }
                                onRowPreview={(row) => {
                                    if (canPreview(row.kind)) {
                                        onPreview({
                                            kind: row.kind,
                                            id: row.id,
                                        })
                                    }
                                }}
                                onRowOpen={(row) => {
                                    if (canPreview(row.kind)) {
                                        onPreview({
                                            kind: row.kind,
                                            id: row.id,
                                        })
                                    }
                                }}
                            />
                        </>
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
