import type { ColumnDef } from "@tanstack/react-table"

import { BusinessStatusBadge, MoneyValue } from "@/components/business"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import type { SalesInvoiceRow } from "@/features/customer-receivables/types"
import type { ColumnActions } from "./column-types"

/**
 * 销项发票列表列。操作仅预览与继续分配，不含审批流程选择或审批动作。
 *
 * @param onPreview 打开发票预览。
 * @param onStartSession 继续分配已登记发票。
 */
export function createInvoiceColumns({
    onPreview,
    onStartSession,
    canStartSession = () => true,
    permissionReason,
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
                            disabled={!canStartSession("invoice")}
                            title={
                                canStartSession("invoice")
                                    ? undefined
                                    : permissionReason
                            }
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
