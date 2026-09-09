/** W12 供应商往来 · 付款列定义（纯构建函数，供 useSupplierAccountsColumns 组装）。 */

import type { ColumnDef } from "@tanstack/react-table"

import { BusinessStatusBadge, MoneyValue } from "@/components/business"
import { Button } from "@/components/ui/button"
import { formatDateTime } from "@/lib/datetime"
import { toAutomationIdSegment } from "@/lib/automation-id"
import type { PaymentRow } from "@/features/supplier-payables/types"

export function buildPaymentColumns(input: {
    openReversalPreview: (reversalId: string) => void
}): ColumnDef<PaymentRow>[] {
    const { openReversalPreview } = input
    return [
        {
            id: "doc",
            header: "付款单",
            meta: { label: "付款单", width: "reference" },
            cell: ({ row }) => (
                <div className="text-sm">
                    <div className="num font-medium">
                        {row.original.paymentNo}
                    </div>
                    <div className="text-xs text-muted-foreground">
                        {row.original.supplierName}
                    </div>
                </div>
            ),
        },
        {
            id: "amount",
            header: "金额 / 未付款",
            meta: {
                label: "金额",
                width: "amount",
                align: "end",
                numeric: true,
            },
            cell: ({ row }) => (
                <div className="text-end text-sm">
                    <MoneyValue value={row.original.amount} taxBasis="gross" />
                    <div className="text-xs text-muted-foreground">
                        未付款{" "}
                        <MoneyValue value={row.original.unallocatedAmount} />
                    </div>
                </div>
            ),
        },
        {
            id: "bank",
            header: "银行引用",
            meta: { label: "银行", width: "default" },
            cell: ({ row }) => (
                <span className="num text-sm">
                    {row.original.bankReferenceMasked}
                </span>
            ),
        },
        {
            id: "reversal",
            header: "关联冲正",
            meta: { label: "关联冲正", width: "default" },
            cell: ({ row }) => {
                const [latest, ...older] = row.original.relatedReversals
                if (!latest) {
                    return (
                        <span className="text-sm text-muted-foreground">—</span>
                    )
                }
                return (
                    <div className="flex flex-col items-start gap-1">
                        <Button
                            id={`supplier-payables-table-row-${toAutomationIdSegment(row.original.paymentId)}-reversal-${toAutomationIdSegment(latest.reversalId)}-open`}
                            type="button"
                            size="xs"
                            variant="ghost"
                            className="num h-auto px-0 text-sm font-medium"
                            onClick={(event) => {
                                event.stopPropagation()
                                openReversalPreview(latest.reversalId)
                            }}
                        >
                            {latest.reversalNo}
                        </Button>
                        <span className="flex items-center gap-1">
                            <BusinessStatusBadge
                                context="list"
                                label={latest.statusLabel}
                                tone={latest.statusTone}
                            />
                            {older.length > 0 ? (
                                <span className="text-xs text-muted-foreground">
                                    另 {older.length} 条
                                </span>
                            ) : null}
                        </span>
                    </div>
                )
            },
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
                    description={
                        row.original.status === "POSTED"
                            ? "已过账不可编辑；纠错请冲正"
                            : undefined
                    }
                />
            ),
        },
        {
            id: "time",
            header: "付款时间",
            meta: { label: "时间", width: "default", numeric: true },
            cell: ({ row }) => (
                <span className="num text-[13px] text-muted-foreground">
                    {formatDateTime(row.original.paidAt, "full", "passthrough")}
                </span>
            ),
        },
    ]
}
