/** W12 供应商往来 · 应付台账列定义（纯构建函数，供 useSupplierAccountsColumns 组装）。 */

import type { ColumnDef } from "@tanstack/react-table"

import { BusinessStatusBadge, MoneyValue } from "@/components/business"
import type { PayableRow } from "@/features/supplier-payables/types"

export function buildPayableColumns(): ColumnDef<PayableRow>[] {
    return [
        {
            id: "supplier",
            header: "供应商 / 来源",
            meta: { label: "供应商", width: "reference" },
            cell: ({ row }) => (
                <div className="flex min-w-0 max-w-64 flex-col items-start gap-1 whitespace-normal text-sm">
                    <span className="truncate font-medium">
                        {row.original.supplierName}
                    </span>
                    <span className="sr-only">·</span>
                    <span className="break-words text-xs text-muted-foreground">
                        {row.original.sourceTypeLabel} ·{" "}
                        <span className="num">
                            {row.original.sourceDocumentNo}
                        </span>
                    </span>
                </div>
            ),
        },
        {
            id: "amounts",
            header: "应付（含税）/ 开放（含税）",
            meta: {
                label: "金额",
                width: "amount",
                align: "end",
                numeric: true,
            },
            cell: ({ row }) => (
                <div className="space-y-1 text-end text-sm">
                    <div>
                        <span className="mr-2 text-xs text-muted-foreground">
                            开放
                        </span>
                        <MoneyValue value={row.original.openTotal} />
                    </div>
                    <div>
                        <span className="mr-2 text-xs text-muted-foreground">
                            应付
                        </span>
                        <MoneyValue value={row.original.grossTotal} />
                    </div>
                </div>
            ),
        },
        {
            id: "tracks",
            header: "已付（净）/ 已收票（净）",
            meta: {
                label: "进度",
                width: "amount",
                align: "end",
                numeric: true,
            },
            cell: ({ row }) => (
                <div className="space-y-1 text-end text-sm">
                    <div>
                        <span className="mr-2 text-xs text-muted-foreground">
                            付款
                        </span>
                        <MoneyValue value={row.original.settledTotal} />
                    </div>
                    <div>
                        <span className="mr-2 text-xs text-muted-foreground">
                            收票
                        </span>
                        <MoneyValue value={row.original.invoicedTotal} />
                    </div>
                </div>
            ),
        },
        {
            id: "due",
            header: "到期",
            meta: { label: "到期", width: "default" },
            cell: ({ row }) => (
                <div className="flex flex-col items-start gap-1 text-sm">
                    <span className="num">{row.original.dueDate}</span>
                    <span className="text-xs text-muted-foreground">
                        {row.original.dueStateLabel}
                    </span>
                </div>
            ),
        },
        {
            id: "status",
            header: "状态",
            meta: { label: "状态", width: "status" },
            cell: ({ row }) => (
                <div className="flex flex-col items-start gap-1.5">
                    <BusinessStatusBadge
                        context="list"
                        label={row.original.statusLabel}
                        tone={row.original.statusTone}
                    />
                    {row.original.paymentGateSummary &&
                    row.original.paymentGateSummary.state !==
                        "NOT_APPLICABLE" ? (
                        <span className="text-xs text-muted-foreground">
                            先款条件{" "}
                            {row.original.paymentGateSummary.state ===
                            "SATISFIED"
                                ? "已满足"
                                : "未满足"}
                        </span>
                    ) : null}
                </div>
            ),
        },
    ]
}
