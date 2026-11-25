import type { ColumnDef } from "@tanstack/react-table"

import {
    BusinessStatusBadge,
    MoneyValue,
    TableIdentityCell,
    TableRowActions,
    TableTextCell,
} from "@/components/business"
import type { ReceivableAccountRow } from "@/features/customer-receivables/types"
import { toAutomationIdSegment } from "@/lib/automation-id"

export type ReceivableColumnActions = {
    onPreview?: (row: ReceivableAccountRow) => void
    onRegisterReceipt?: (row: ReceivableAccountRow) => void
    canRegisterReceipt?: boolean
}

export function createReceivableColumns(
    actions: ReceivableColumnActions = {},
): ColumnDef<ReceivableAccountRow>[] {
    return [
        {
            id: "party",
            header: "往来主体 / 客户",
            meta: { label: "往来主体", width: "flex" },
            cell: ({ row }) => (
                <TableIdentityCell
                    primary={row.original.counterpartyPartyName}
                    primaryTitle={row.original.counterpartyPartyName}
                    secondary={row.original.customerName}
                    secondaryTitle={row.original.customerName}
                />
            ),
        },
        {
            id: "order",
            header: "销售单 / 应收款项",
            meta: { label: "销售单", width: "default" },
            cell: ({ row }) => {
                const reference = `应收款项 #${row.original.accountSeq} · ${row.original.businessTypeLabel}`
                return (
                    <TableIdentityCell
                        primary={
                            <TableTextCell
                                value={row.original.salesOrderNo}
                                numeric
                            />
                        }
                        primaryTitle={row.original.salesOrderNo}
                        secondary={reference}
                        secondaryTitle={reference}
                        numericSecondary
                    />
                )
            },
        },
        {
            id: "open",
            header: "待收金额",
            meta: {
                label: "待收金额（含税）",
                width: "amount",
                align: "end",
                numeric: true,
            },
            cell: ({ row }) => (
                <MoneyValue display="exact" value={row.original.openTotal} />
            ),
        },
        {
            id: "gross",
            header: "应收总额",
            meta: {
                label: "应收总额（含税）",
                width: "amount",
                align: "end",
                numeric: true,
            },
            cell: ({ row }) => (
                <MoneyValue display="exact" value={row.original.grossTotal} />
            ),
        },
        {
            id: "settled",
            header: "已核销回款",
            meta: {
                label: "已核销回款",
                width: "amount",
                align: "end",
                numeric: true,
            },
            cell: ({ row }) => (
                <MoneyValue display="exact" value={row.original.settledTotal} />
            ),
        },
        {
            id: "openInvoiceable",
            header: "待开票",
            meta: {
                label: "待开票（含税）",
                width: "amount",
                align: "end",
                numeric: true,
            },
            cell: ({ row }) => (
                <MoneyValue
                    display="exact"
                    value={row.original.openInvoiceableTotal}
                />
            ),
        },
        {
            id: "invoiced",
            header: "净已开票",
            meta: {
                label: "净已开票（含税）",
                width: "amount",
                align: "end",
                numeric: true,
            },
            cell: ({ row }) => (
                <MoneyValue
                    display="exact"
                    value={row.original.invoicedTotal}
                />
            ),
        },
        {
            id: "due",
            header: "到期",
            meta: { label: "到期", width: "default" },
            cell: ({ row }) => (
                <div className="flex flex-col items-start gap-0.5">
                    <TableTextCell value={row.original.dueDate} numeric />
                    <BusinessStatusBadge
                        context="list"
                        label={row.original.dueStateLabel}
                        appearance={
                            row.original.dueState === "not_due" ||
                            row.original.dueState === "unknown"
                                ? "quiet"
                                : "badge"
                        }
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
            meta: { label: "状态", width: "status" },
            cell: ({ row }) => (
                <BusinessStatusBadge
                    context="list"
                    label={row.original.statusLabel}
                    tone={row.original.statusTone}
                    appearance={
                        row.original.status === "settled" ? "quiet" : "badge"
                    }
                />
            ),
        },
        {
            id: "actions",
            header: "操作",
            enableHiding: false,
            meta: { label: "操作", width: "status", align: "end" },
            cell: ({ row: { original } }) => {
                const seg = toAutomationIdSegment(original.accountId)
                const canReceipt =
                    Boolean(actions.canRegisterReceipt) &&
                    Boolean(actions.onRegisterReceipt) &&
                    original.allowedActions.includes("REGISTER_RECEIPT")
                return (
                    <TableRowActions
                        maxInline={2}
                        moreId={`customer-receivables-row-${seg}-more`}
                        moreLabel={`${original.salesOrderNo} 更多操作`}
                        actions={[
                            ...(canReceipt
                                ? [
                                      {
                                          id: `customer-receivables-row-${seg}-receipt`,
                                          label: "登记回款",
                                          onClick: () =>
                                              actions.onRegisterReceipt?.(
                                                  original,
                                              ),
                                      },
                                  ]
                                : []),
                            ...(actions.onPreview
                                ? [
                                      {
                                          id: `customer-receivables-row-${seg}-view`,
                                          label: "详情",
                                          onClick: () =>
                                              actions.onPreview?.(original),
                                      },
                                  ]
                                : []),
                        ]}
                    />
                )
            },
        },
    ]
}
