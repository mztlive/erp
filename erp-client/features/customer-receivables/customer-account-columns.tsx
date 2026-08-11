import type { ColumnDef } from "@tanstack/react-table"

import { BusinessStatusBadge, MoneyValue } from "@/components/business"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import type {
    AllocationMode,
    ReceiptRow,
    ReceivableAccountRow,
    SalesInvoiceRow,
} from "@/features/customer-receivables/types"
import { formatDateTime } from "@/lib/datetime"

export type CustomerAccountPreviewTarget = Readonly<{
    kind: "receivable" | "receipt" | "invoice"
    id: string
}>

type AllocationTarget = Readonly<{
    salesOrderId?: string
    receivableAccountId?: string
}>

type ColumnActions = Readonly<{
    onPreview: (target: CustomerAccountPreviewTarget) => void
    onStartSession: (
        mode: AllocationMode,
        partyId: string,
        existingFactId?: string,
        target?: AllocationTarget,
    ) => void | Promise<void>
}>

export function createReceivableColumns({
    onPreview,
    onStartSession,
}: ColumnActions): ColumnDef<ReceivableAccountRow>[] {
    return [
        {
            id: "party",
            header: "往来主体 / 客户",
            meta: { label: "往来主体", width: "reference" },
            cell: ({ row }) => (
                <div className="flex min-w-0 items-center gap-1.5">
                    <span className="truncate text-sm font-medium">
                        {row.original.counterpartyPartyName}
                    </span>
                    <span className="shrink-0 text-muted-foreground">·</span>
                    <span className="truncate text-xs text-muted-foreground">
                        {row.original.customerName}
                    </span>
                </div>
            ),
        },
        {
            id: "order",
            header: "销售单 / 子账",
            meta: { label: "销售单", width: "reference" },
            cell: ({ row }) => (
                <div className="flex items-center gap-1.5">
                    <span className="num text-sm">
                        {row.original.salesOrderNo}
                    </span>
                    <span className="text-xs text-muted-foreground">
                        子账 #{row.original.accountSeq} ·{" "}
                        {row.original.businessTypeLabel}
                    </span>
                </div>
            ),
        },
        {
            id: "open",
            header: "开放应收（含税）",
            meta: {
                label: "开放应收",
                width: "amount",
                align: "end",
                numeric: true,
            },
            cell: ({ row }) => <MoneyValue value={row.original.openTotal} />,
        },
        {
            id: "settled",
            header: "已核销回款（含税）",
            meta: {
                label: "已核销",
                width: "amount",
                align: "end",
                numeric: true,
            },
            cell: ({ row }) => <MoneyValue value={row.original.settledTotal} />,
        },
        {
            id: "invoice",
            header: "净已开票 / 可开票（含税）",
            meta: {
                label: "开票",
                width: "amount",
                align: "end",
                numeric: true,
            },
            cell: ({ row }) => (
                <div className="flex items-center justify-end gap-1.5 text-right">
                    <MoneyValue value={row.original.invoicedTotal} />
                    <span className="text-xs text-muted-foreground">
                        / 可开
                    </span>
                    <MoneyValue value={row.original.openInvoiceableTotal} />
                </div>
            ),
        },
        {
            id: "due",
            header: "到期",
            meta: { label: "到期" },
            cell: ({ row }) => (
                <div className="flex items-center gap-1.5">
                    <span className="num text-sm">{row.original.dueDate}</span>
                    <BusinessStatusBadge
                        context="list"
                        label={row.original.dueStateLabel}
                        tone={
                            row.original.dueState === "overdue"
                                ? "destructive"
                                : row.original.dueState === "due_today"
                                  ? "warning"
                                  : "neutral"
                        }
                    />
                </div>
            ),
        },
        {
            id: "status",
            header: "状态",
            meta: { label: "状态" },
            cell: ({ row }) => (
                <div className="flex items-center gap-1.5">
                    <BusinessStatusBadge
                        context="list"
                        label={row.original.statusLabel}
                        tone={row.original.statusTone}
                    />
                    {row.original.reviewStatus !== "na" ? (
                        <span className="text-xs text-muted-foreground">
                            {row.original.reviewStatusLabel}
                        </span>
                    ) : null}
                </div>
            ),
        },
        {
            id: "actions",
            header: "操作",
            meta: { label: "操作", width: "default" },
            cell: ({ row }) => (
                <div className="flex flex-nowrap justify-end gap-1">
                    <Button
                        type="button"
                        size="xs"
                        variant="ghost"
                        onClick={() =>
                            onPreview({
                                kind: "receivable",
                                id: row.original.accountId,
                            })
                        }
                    >
                        预览
                    </Button>
                    <Button
                        type="button"
                        size="xs"
                        variant="outline"
                        disabled={
                            !row.original.allowedActions.includes(
                                "REGISTER_RECEIPT",
                            )
                        }
                        title={
                            row.original.allowedActions.includes(
                                "REGISTER_RECEIPT",
                            )
                                ? undefined
                                : "当前无回款登记/核销权限"
                        }
                        onClick={() =>
                            void onStartSession(
                                "receipt",
                                row.original.counterpartyPartyId,
                                undefined,
                                {
                                    salesOrderId: row.original.salesOrderId,
                                    receivableAccountId: row.original.accountId,
                                },
                            )
                        }
                    >
                        核销
                    </Button>
                </div>
            ),
        },
    ]
}

export function createReceiptColumns({
    onPreview,
    onStartSession,
}: ColumnActions): ColumnDef<ReceiptRow>[] {
    return [
        {
            id: "doc",
            header: "回款单号",
            meta: { label: "回款单号", width: "reference" },
            cell: ({ row }) => (
                <div>
                    <div className="num text-sm font-medium">
                        {row.original.receiptNo}
                    </div>
                    <div className="truncate text-xs text-muted-foreground">
                        {row.original.counterpartyPartyName}
                    </div>
                </div>
            ),
        },
        {
            id: "receivedAt",
            header: "到账时间",
            cell: ({ row }) => (
                <span className="num text-sm">
                    {formatDateTime(
                        row.original.receivedAt,
                        "full",
                        "passthrough",
                    )}
                </span>
            ),
        },
        {
            id: "amount",
            header: "到账金额",
            meta: {
                label: "金额",
                width: "amount",
                align: "end",
                numeric: true,
            },
            cell: ({ row }) => (
                <MoneyValue value={row.original.amount} taxBasis="gross" />
            ),
        },
        {
            id: "alloc",
            header: "净已分配 / 未分配",
            meta: {
                label: "分配",
                width: "amount",
                align: "end",
                numeric: true,
            },
            cell: ({ row }) => (
                <div className="text-right">
                    <MoneyValue value={row.original.allocatedTotal} />
                    <div className="text-xs text-muted-foreground">
                        未分配{" "}
                        <MoneyValue value={row.original.unallocatedAmount} />
                    </div>
                </div>
            ),
        },
        {
            id: "status",
            header: "状态",
            cell: ({ row }) => (
                <BusinessStatusBadge
                    context="list"
                    label={row.original.statusLabel}
                    tone={row.original.statusTone}
                />
            ),
        },
        {
            id: "actions",
            header: "操作",
            meta: { label: "操作", width: "default" },
            cell: ({ row }) => (
                <div className="flex flex-wrap justify-end gap-1">
                    <Button
                        type="button"
                        size="sm"
                        variant="ghost"
                        onClick={() =>
                            onPreview({
                                kind: "receipt",
                                id: row.original.receiptId,
                            })
                        }
                    >
                        预览
                    </Button>
                    {row.original.allowedActions.includes(
                        "CONTINUE_ALLOCATE",
                    ) ? (
                        <Button
                            type="button"
                            size="sm"
                            variant="outline"
                            onClick={() =>
                                void onStartSession(
                                    "receipt",
                                    row.original.counterpartyPartyId,
                                    row.original.receiptId,
                                )
                            }
                        >
                            继续核销
                        </Button>
                    ) : null}
                </div>
            ),
        },
    ]
}

export function createInvoiceColumns({
    onPreview,
    onStartSession,
}: ColumnActions): ColumnDef<SalesInvoiceRow>[] {
    return [
        {
            id: "doc",
            header: "发票",
            meta: { label: "发票", width: "reference" },
            cell: ({ row }) => (
                <div>
                    <div className="flex items-center gap-2">
                        <span className="num text-sm font-medium">
                            {row.original.invoiceNo}
                        </span>
                        <Badge
                            variant={
                                row.original.invoiceKind === "red"
                                    ? "warning"
                                    : "secondary"
                            }
                        >
                            {row.original.invoiceKindLabel}
                        </Badge>
                    </div>
                    <div className="truncate text-xs text-muted-foreground">
                        {row.original.invoiceCode
                            ? `代码 ${row.original.invoiceCode} · `
                            : ""}
                        {row.original.counterpartyPartyName}
                    </div>
                </div>
            ),
        },
        {
            id: "date",
            header: "开票日期",
            cell: ({ row }) => (
                <span className="num text-sm">{row.original.invoiceDate}</span>
            ),
        },
        {
            id: "gross",
            header: "含税金额",
            meta: {
                label: "含税",
                width: "amount",
                align: "end",
                numeric: true,
            },
            cell: ({ row }) => (
                <MoneyValue value={row.original.grossAmount} taxBasis="gross" />
            ),
        },
        {
            id: "alloc",
            header: "净已分配 / 未分配",
            meta: {
                label: "分配",
                width: "amount",
                align: "end",
                numeric: true,
            },
            cell: ({ row }) => (
                <div className="text-right">
                    <MoneyValue value={row.original.allocatedTotal} />
                    <div className="text-xs text-muted-foreground">
                        未分配{" "}
                        <MoneyValue value={row.original.unallocatedAmount} />
                    </div>
                </div>
            ),
        },
        {
            id: "status",
            header: "状态",
            cell: ({ row }) => (
                <BusinessStatusBadge
                    context="list"
                    label={row.original.statusLabel}
                    tone={row.original.statusTone}
                />
            ),
        },
        {
            id: "actions",
            header: "操作",
            meta: { label: "操作", width: "default" },
            cell: ({ row }) => (
                <div className="flex flex-wrap justify-end gap-1">
                    <Button
                        type="button"
                        size="sm"
                        variant="ghost"
                        onClick={() =>
                            onPreview({
                                kind: "invoice",
                                id: row.original.invoiceId,
                            })
                        }
                    >
                        预览
                    </Button>
                    {row.original.allowedActions.includes(
                        "CONTINUE_ALLOCATE",
                    ) ? (
                        <Button
                            type="button"
                            size="sm"
                            variant="outline"
                            onClick={() =>
                                void onStartSession(
                                    "invoice",
                                    row.original.counterpartyPartyId,
                                    row.original.invoiceId,
                                )
                            }
                        >
                            继续分配
                        </Button>
                    ) : null}
                </div>
            ),
        },
    ]
}
