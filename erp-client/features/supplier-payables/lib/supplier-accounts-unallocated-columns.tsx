/** W12 供应商往来 · 待核销列定义（纯构建函数，供 useSupplierAccountsColumns 组装）。 */

import type { ColumnDef } from "@tanstack/react-table"

import {
    BusinessStatusBadge,
    MoneyValue,
    TableIdentityCell,
    TableRowActions,
} from "@/components/business"
import type { UnallocatedRow } from "@/features/supplier-payables/types"
import { toAutomationIdSegment } from "@/lib/automation-id"

export function buildUnallocatedColumns(input: {
    canPostAllocation: boolean
    onOpenUnallocatedPreview: (id: string) => void
    onContinueAllocateInvoice: (row: UnallocatedRow) => void
}): ColumnDef<UnallocatedRow>[] {
    const {
        canPostAllocation,
        onOpenUnallocatedPreview,
        onContinueAllocateInvoice,
    } = input
    return [
        {
            id: "track",
            header: "单据类型",
            meta: { label: "单据类型", width: "default" },
            cell: ({ row }) => (
                <BusinessStatusBadge
                    context="list"
                    label={row.original.trackLabel}
                    tone={row.original.track === "payment" ? "warning" : "info"}
                />
            ),
        },
        {
            id: "doc",
            header: "单据 / 供应商",
            meta: { label: "单据", width: "flex" },
            cell: ({ row }) => (
                <TableIdentityCell
                    primary={
                        <span className="num">{row.original.documentNo}</span>
                    }
                    primaryTitle={row.original.documentNo}
                    secondary={row.original.supplierName}
                    secondaryTitle={row.original.supplierName}
                />
            ),
        },
        {
            id: "amount",
            header: "未分配余额",
            meta: {
                label: "余额",
                width: "amount",
                align: "end",
                numeric: true,
            },
            cell: ({ row }) => (
                <div>
                    <MoneyValue
                        value={row.original.unallocatedAmount}
                        taxBasis="gross"
                        display="exact"
                    />
                    <div className="text-xs text-muted-foreground">
                        记录{" "}
                        <MoneyValue
                            value={row.original.amount}
                            display="exact"
                        />
                    </div>
                </div>
            ),
        },
        {
            id: "actions",
            header: () => <span className="sr-only">操作</span>,
            meta: {
                label: "操作",
                width: "status",
                align: "end",
            },
            cell: ({ row }) => {
                const item = row.original
                const canAllocateInvoice =
                    canPostAllocation && item.track === "purchase_invoice"
                const prefix = `supplier-payables-table-unallocated-row-${toAutomationIdSegment(item.id)}`
                return (
                    <TableRowActions
                        moreId={`${prefix}-more`}
                        moreLabel={`${item.documentNo} 更多操作`}
                        actions={[
                            {
                                id: `${prefix}-allocate`,
                                label: "继续核销",
                                hidden: !canAllocateInvoice,
                                onClick: () => onContinueAllocateInvoice(item),
                            },
                            {
                                id: `${prefix}-detail`,
                                label: "详情",
                                onClick: () =>
                                    onOpenUnallocatedPreview(item.id),
                            },
                        ]}
                    />
                )
            },
        },
    ]
}
