import type { ColumnDef } from "@tanstack/react-table"

import { BusinessStatusBadge, MoneyValue } from "@/components/business"
import { Button } from "@/components/ui/button"
import type { ReceiptRow } from "@/features/customer-receivables/types"
import { formatDateTime } from "@/lib/datetime"
import { toAutomationIdSegment } from "@/lib/automation-id"
import type { ColumnActions } from "./column-types"

export function createReceiptColumns({
    onPreview,
    onStartSession,
    canStartSession = () => true,
    permissionReason,
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
                        id={`customer-receivables-receipt-row-${toAutomationIdSegment(row.original.receiptId)}-preview`}
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
                            id={`customer-receivables-receipt-row-${toAutomationIdSegment(row.original.receiptId)}-continue-allocate`}
                            type="button"
                            size="sm"
                            variant="outline"
                            disabled={!canStartSession("receipt")}
                            title={
                                canStartSession("receipt")
                                    ? undefined
                                    : permissionReason
                            }
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
