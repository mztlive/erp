/** W12 供应商往来 · 应付台账列定义（纯构建函数，供 useSupplierAccountsColumns 组装）。 */

import type { ColumnDef } from "@tanstack/react-table"

import { BusinessStatusBadge, MoneyValue } from "@/components/business"
import type { PayableRow } from "@/features/supplier-payables/types"
import { cn } from "@/lib/utils"

export function buildPayableColumns(): ColumnDef<PayableRow>[] {
    return [
        {
            id: "supplier",
            header: "供应商 / 来源单据",
            meta: { label: "供应商 / 来源单据", width: "flex" },
            cell: ({ row }) => (
                <div className="flex w-0 min-w-full flex-col gap-1 text-sm">
                    <span
                        className="truncate font-medium"
                        title={row.original.supplierName}
                    >
                        {row.original.supplierName}
                    </span>
                    <span className="sr-only">·</span>
                    <span
                        className="truncate select-text text-xs text-muted-foreground"
                        title={`${row.original.sourceTypeLabel} · ${row.original.sourceDocumentNo}`}
                    >
                        {row.original.sourceTypeLabel} ·{" "}
                        <span className="num">
                            {row.original.sourceDocumentNo}
                        </span>
                    </span>
                </div>
            ),
        },
        {
            id: "openTotal",
            header: "剩余应付（含税）",
            meta: {
                label: "剩余应付（含税）",
                width: "amount",
                align: "end",
                numeric: true,
            },
            cell: ({ row }) => (
                <MoneyValue
                    value={row.original.openTotal}
                    display="exact"
                    className="font-semibold"
                />
            ),
        },
        {
            id: "grossTotal",
            header: "应付总额（含税）",
            meta: {
                label: "应付总额（含税）",
                width: "amount",
                align: "end",
                numeric: true,
            },
            cell: ({ row }) => (
                <MoneyValue value={row.original.grossTotal} display="exact" />
            ),
        },
        {
            id: "settledTotal",
            header: "已付净额",
            meta: {
                label: "已付净额",
                width: "amount",
                align: "end",
                numeric: true,
            },
            cell: ({ row }) => (
                <MoneyValue value={row.original.settledTotal} display="exact" />
            ),
        },
        {
            id: "invoicedTotal",
            header: "已收票净额",
            meta: {
                label: "已收票净额",
                width: "amount",
                align: "end",
                numeric: true,
            },
            cell: ({ row }) => (
                <MoneyValue
                    value={row.original.invoicedTotal}
                    display="exact"
                />
            ),
        },
        {
            id: "due",
            header: "到期情况",
            meta: { label: "到期情况", width: "default" },
            cell: ({ row }) => (
                <div className="flex flex-col items-start gap-1 text-sm">
                    {row.original.dueDate ? (
                        <span className="num">{row.original.dueDate}</span>
                    ) : null}
                    <span
                        className={cn(
                            "text-xs",
                            row.original.dueState === "overdue"
                                ? "text-destructive"
                                : "text-muted-foreground",
                        )}
                    >
                        {row.original.dueStateLabel}
                    </span>
                </div>
            ),
        },
        {
            id: "status",
            header: "结清状态",
            meta: { label: "结清状态", width: "status" },
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
