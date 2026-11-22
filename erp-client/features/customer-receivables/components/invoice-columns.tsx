import type { ColumnDef } from "@tanstack/react-table"

import {
    BusinessStatusBadge,
    MoneyValue,
    TableIdentityCell,
    TableRowActions,
    TableTextCell,
    taxAmountToneClass,
} from "@/components/business"
import type { SalesInvoiceRow } from "@/features/customer-receivables/types"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { allocationSummaryCell } from "./allocation-summary"

export type InvoiceColumnActions = {
    onPreview?: (row: SalesInvoiceRow) => void
}

/** 销项发票列表展示核心票面与分配事实，并保留详情快捷出口。 */
export function createInvoiceColumns(
    actions: InvoiceColumnActions = {},
): ColumnDef<SalesInvoiceRow>[] {
    return [
        {
            id: "doc",
            header: "发票号码 / 往来主体",
            meta: { label: "发票", width: "flex" },
            cell: ({ row }) => {
                const {
                    invoiceNo,
                    invoiceCode,
                    invoiceKindLabel,
                    invoiceKind,
                    counterpartyPartyName,
                } = row.original
                const reference = [
                    invoiceCode ? `代码 ${invoiceCode}` : undefined,
                    counterpartyPartyName,
                ]
                    .filter(Boolean)
                    .join(" · ")
                return (
                    <TableIdentityCell
                        primary={
                            <span className="flex items-center gap-2">
                                <TableTextCell
                                    value={invoiceNo}
                                    numeric
                                    clamp={1}
                                />
                                <BusinessStatusBadge
                                    context="list"
                                    label={invoiceKindLabel}
                                    tone={
                                        invoiceKind === "red"
                                            ? "warning"
                                            : "neutral"
                                    }
                                    appearance={
                                        invoiceKind === "red"
                                            ? "badge"
                                            : "quiet"
                                    }
                                />
                            </span>
                        }
                        primaryTitle={invoiceNo}
                        secondary={reference}
                        secondaryTitle={reference}
                        numericSecondary
                    />
                )
            },
        },
        {
            id: "date",
            header: "开票日期",
            meta: { label: "开票日期", width: "default" },
            cell: ({ row }) => (
                <TableTextCell value={row.original.invoiceDate} numeric />
            ),
        },
        {
            id: "gross",
            header: "含税金额",
            meta: {
                label: "含税金额",
                width: "amount",
                align: "end",
                numeric: true,
            },
            cell: ({ row }) => (
                <MoneyValue
                    value={row.original.grossAmount}
                    display="exact"
                    className={taxAmountToneClass("含税金额")}
                />
            ),
        },
        {
            id: "alloc",
            header: "未分配 / 净已分配",
            meta: {
                label: "分配情况",
                width: "amount",
                align: "end",
                numeric: true,
            },
            cell: ({ row }) => allocationSummaryCell(row.original),
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
                        row.original.status === "registered" ? "quiet" : "badge"
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
                const seg = toAutomationIdSegment(original.invoiceId)
                return (
                    <TableRowActions
                        maxInline={1}
                        moreId={`customer-receivables-invoice-${seg}-more`}
                        moreLabel={`${original.invoiceNo} 更多操作`}
                        actions={
                            actions.onPreview
                                ? [
                                      {
                                          id: `customer-receivables-invoice-${seg}-view`,
                                          label: "详情",
                                          onClick: () =>
                                              actions.onPreview?.(original),
                                      },
                                  ]
                                : []
                        }
                    />
                )
            },
        },
    ]
}
