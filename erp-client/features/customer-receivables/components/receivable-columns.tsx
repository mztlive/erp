import type { ColumnDef } from "@tanstack/react-table"

import { BusinessStatusBadge, MoneyValue } from "@/components/business"
import { Button } from "@/components/ui/button"
import type { ReceivableAccountRow } from "@/features/customer-receivables/types"
import type { ColumnActions } from "./column-types"

export function createReceivableColumns({
    onPreview,
    onStartSession,
    canStartSession = () => true,
    permissionReason,
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
                            ) || !canStartSession("receipt")
                        }
                        title={
                            !canStartSession("receipt")
                                ? permissionReason
                                : row.original.allowedActions.includes(
                                        "REGISTER_RECEIPT",
                                    )
                                  ? undefined
                                  : "当前不能登记回款并核销"
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
