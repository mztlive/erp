/** W12 供应商往来 · 应付台账列定义（纯构建函数，供 useSupplierAccountsColumns 组装）。 */

import type { ColumnDef } from "@tanstack/react-table"

import { BusinessStatusBadge, MoneyValue } from "@/components/business"
import { Button } from "@/components/ui/button"
import type {
    PayableRow,
    SessionState,
    SupplierAccountsListView,
} from "@/features/supplier-payables/types"

export function buildPayableColumns(input: {
    data: SupplierAccountsListView | undefined
    returnTo?: string
    fromWorkspace?: string
    paymentTaskPayableAccountId?: string
    openPreview: (payableAccountId: string) => void
    openSession: (next: SessionState) => void
}): ColumnDef<PayableRow>[] {
    const {
        data,
        returnTo,
        fromWorkspace,
        paymentTaskPayableAccountId,
        openPreview,
        openSession,
    } = input
    return [
        {
            id: "supplier",
            header: "供应商 / 来源",
            meta: { label: "供应商", width: "reference" },
            cell: ({ row }) => (
                <div className="flex min-w-0 items-center gap-1.5 text-sm">
                    <span className="truncate font-medium">
                        {row.original.supplierName}
                    </span>
                    <span className="shrink-0 text-muted-foreground">·</span>
                    <span className="truncate text-xs text-muted-foreground">
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
                <div className="flex items-center justify-end gap-1 text-end text-sm">
                    <MoneyValue value={row.original.grossTotal} />
                    <span className="text-xs text-muted-foreground">
                        / 开放
                    </span>
                    <MoneyValue
                        className="text-xs"
                        value={row.original.openTotal}
                    />
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
                <div className="flex items-center justify-end gap-1.5 text-end text-xs text-muted-foreground">
                    <span>付款</span>{" "}
                    <MoneyValue value={row.original.settledTotal} />
                    <span>/ 收票</span>{" "}
                    <MoneyValue value={row.original.invoicedTotal} />
                </div>
            ),
        },
        {
            id: "due",
            header: "到期",
            meta: { label: "到期", width: "default" },
            cell: ({ row }) => (
                <div className="flex items-center gap-1.5 text-sm">
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
                <div className="flex items-center gap-1.5">
                    <BusinessStatusBadge
                        context="list"
                        label={row.original.statusLabel}
                        tone={row.original.statusTone}
                    />
                    {row.original.paymentGateSummary &&
                    row.original.paymentGateSummary.state !==
                        "NOT_APPLICABLE" ? (
                        <span className="text-tiny text-muted-foreground">
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
        {
            id: "actions",
            header: "操作",
            meta: { label: "操作", width: "default", align: "end" },
            cell: ({ row }) => {
                const canExecutePayment =
                    Boolean(data?.canRegisterPayment) &&
                    row.original.payableAccountId ===
                        paymentTaskPayableAccountId
                return (
                    <div className="flex flex-nowrap justify-end gap-1">
                        <Button
                            type="button"
                            size="xs"
                            variant="outline"
                            onClick={() =>
                                openPreview(row.original.payableAccountId)
                            }
                        >
                            预览
                        </Button>
                        <Button
                            type="button"
                            size="xs"
                            onClick={() =>
                                openSession({
                                    track: "payment",
                                    supplierId: row.original.supplierId,
                                    preselectPayableAccountId:
                                        row.original.payableAccountId,
                                    purchaseOrderId:
                                        row.original.sourceType ===
                                        "PURCHASE_ORDER"
                                            ? row.original.sourceDocumentId
                                            : undefined,
                                    returnTo,
                                    fromWorkspace,
                                })
                            }
                            disabled={!canExecutePayment}
                            title={
                                canExecutePayment
                                    ? undefined
                                    : "付款必须由当前负责人从对应付款任务进入"
                            }
                        >
                            核销付款
                        </Button>
                    </div>
                )
            },
        },
    ]
}
