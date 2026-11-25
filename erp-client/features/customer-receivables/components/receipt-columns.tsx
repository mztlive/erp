import type { ColumnDef } from "@tanstack/react-table"

import {
    BusinessStatusBadge,
    MoneyValue,
    TableIdentityCell,
    TableRowActions,
    TableTextCell,
} from "@/components/business"
import type { ReceiptRow } from "@/features/customer-receivables/types"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { formatDateTime } from "@/lib/datetime"
import { allocationSummaryCell } from "./allocation-summary"

export type ReceiptColumnActions = {
    onPreview?: (row: ReceiptRow) => void
    onContinueAllocate?: (row: ReceiptRow) => void
    canRegisterReceipt?: boolean
}

export function createReceiptColumns(
    actions: ReceiptColumnActions = {},
): ColumnDef<ReceiptRow>[] {
    return [
        {
            id: "doc",
            header: "回款单号 / 往来主体",
            meta: { label: "回款单号", width: "flex" },
            cell: ({ row }) => (
                <TableIdentityCell
                    primary={
                        <TableTextCell value={row.original.receiptNo} numeric />
                    }
                    primaryTitle={row.original.receiptNo}
                    secondary={row.original.counterpartyPartyName}
                    secondaryTitle={row.original.counterpartyPartyName}
                />
            ),
        },
        {
            id: "receivedAt",
            header: "到账时间",
            meta: { label: "到账时间", width: "default" },
            cell: ({ row }) => (
                <TableTextCell
                    value={
                        row.original.receivedAt
                            ? formatDateTime(
                                  row.original.receivedAt,
                                  "full",
                                  "passthrough",
                              )
                            : undefined
                    }
                    numeric
                />
            ),
        },
        {
            id: "amount",
            header: "到账金额",
            meta: {
                label: "到账金额",
                width: "amount",
                align: "end",
                numeric: true,
            },
            cell: ({ row }) => (
                <MoneyValue display="exact" value={row.original.amount} />
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
                        row.original.status === "posted" ? "quiet" : "badge"
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
                const seg = toAutomationIdSegment(original.receiptId)
                const canContinue =
                    Boolean(actions.canRegisterReceipt) &&
                    Boolean(actions.onContinueAllocate) &&
                    original.allowedActions.includes("CONTINUE_ALLOCATE")
                return (
                    <TableRowActions
                        maxInline={2}
                        moreId={`customer-receivables-receipt-${seg}-more`}
                        moreLabel={`${original.receiptNo} 更多操作`}
                        actions={[
                            ...(canContinue
                                ? [
                                      {
                                          id: `customer-receivables-receipt-${seg}-allocate`,
                                          label: "继续核销",
                                          onClick: () =>
                                              actions.onContinueAllocate?.(
                                                  original,
                                              ),
                                      },
                                  ]
                                : []),
                            ...(actions.onPreview
                                ? [
                                      {
                                          id: `customer-receivables-receipt-${seg}-view`,
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
