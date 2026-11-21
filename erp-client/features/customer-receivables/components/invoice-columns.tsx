import type { ColumnDef } from "@tanstack/react-table"

import {
    BusinessStatusBadge,
    MoneyValue,
    TableIdentityCell,
    TableTextCell,
    taxAmountToneClass,
} from "@/components/business"
import type { SalesInvoiceRow } from "@/features/customer-receivables/types"
import { allocationSummaryCell } from "./allocation-summary"

/** 销项发票列表只展示事实，操作由行预览 Sheet 承载。 */
export function createInvoiceColumns(): ColumnDef<SalesInvoiceRow>[] {
    return [
        {
            id: "doc",
            header: "发票",
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
            meta: { width: "default" },
            cell: ({ row }) => (
                <TableTextCell value={row.original.invoiceDate} numeric />
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
                <MoneyValue
                    value={row.original.grossAmount}
                    display="exact"
                    taxBasis="gross"
                    className={taxAmountToneClass("含税金额")}
                />
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
            cell: ({ row }) => allocationSummaryCell(row.original),
        },
        {
            id: "status",
            header: "状态",
            meta: { width: "status" },
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
    ]
}
