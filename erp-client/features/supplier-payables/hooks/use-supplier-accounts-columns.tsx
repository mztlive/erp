"use client"

import * as React from "react"
import type { ColumnDef } from "@tanstack/react-table"

import { BusinessStatusBadge, MoneyValue } from "@/components/business"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { formatDateTime } from "@/lib/datetime"
import type {
    PayableRow,
    PaymentRow,
    PurchaseInvoiceRow,
    ReverseTarget,
    SessionState,
    SupplierAccountsListView,
    UnallocatedRow,
} from "@/features/supplier-payables/types"

export function useSupplierAccountsColumns(input: {
    data: SupplierAccountsListView | undefined
    returnTo?: string
    fromWorkspace?: string
    openPreview: (payableAccountId: string) => void
    openSession: (next: SessionState) => void
    setReverseTarget: React.Dispatch<React.SetStateAction<ReverseTarget | null>>
    setRedInvoiceNo: React.Dispatch<React.SetStateAction<string>>
}) {
    const {
        data,
        returnTo,
        fromWorkspace,
        openPreview,
        openSession,
        setReverseTarget,
        setRedInvoiceNo,
    } = input

    const payableColumns = React.useMemo<ColumnDef<PayableRow>[]>(
        () => [
            {
                id: "supplier",
                header: "供应商 / 来源",
                meta: { label: "供应商", width: "reference" },
                cell: ({ row }) => (
                    <div className="flex min-w-0 items-center gap-1.5 text-sm">
                        <span className="truncate font-medium">
                            {row.original.supplierName}
                        </span>
                        <span className="shrink-0 text-muted-foreground">
                            ·
                        </span>
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
                cell: ({ row }) => (
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
                            disabled={!data?.canRegisterPayment}
                            title={
                                data?.canRegisterPayment
                                    ? undefined
                                    : "当前无付款登记/核销权限"
                            }
                        >
                            核销付款
                        </Button>
                    </div>
                ),
            },
        ],
        // eslint-disable-next-line react-hooks/exhaustive-deps
        [data?.canRegisterPayment, returnTo, fromWorkspace],
    )

    const paymentColumns = React.useMemo<ColumnDef<PaymentRow>[]>(
        () => [
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
                header: "金额 / 未分配",
                meta: {
                    label: "金额",
                    width: "amount",
                    align: "end",
                    numeric: true,
                },
                cell: ({ row }) => (
                    <div className="text-end text-sm">
                        <MoneyValue
                            value={row.original.amount}
                            taxBasis="gross"
                        />
                        <div className="text-xs text-muted-foreground">
                            未分配{" "}
                            <MoneyValue
                                value={row.original.unallocatedAmount}
                            />
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
                                ? "已确认不可编辑；纠错请冲正"
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
                    <span className="num text-xs text-muted-foreground">
                        {formatDateTime(
                            row.original.paidAt,
                            "full",
                            "passthrough",
                        )}
                    </span>
                ),
            },
            {
                id: "actions",
                header: "操作",
                meta: { label: "操作", width: "default", align: "end" },
                cell: ({ row }) => (
                    <div className="flex flex-wrap justify-end gap-1">
                        {row.original.allowedActions.includes(
                            "CONTINUE_ALLOCATE",
                        ) ? (
                            <Button
                                type="button"
                                size="xs"
                                onClick={() =>
                                    openSession({
                                        track: "payment",
                                        supplierId: row.original.supplierId,
                                        existingPaymentId:
                                            row.original.paymentId,
                                        returnTo,
                                        fromWorkspace,
                                    })
                                }
                            >
                                继续核销
                            </Button>
                        ) : null}
                        {row.original.allowedActions.includes("REVERSE") ? (
                            <Button
                                type="button"
                                size="xs"
                                variant="outline"
                                onClick={() =>
                                    setReverseTarget({
                                        kind: "payment",
                                        id: row.original.paymentId,
                                        no: row.original.paymentNo,
                                    })
                                }
                            >
                                冲正
                            </Button>
                        ) : null}
                    </div>
                ),
            },
        ],
        // eslint-disable-next-line react-hooks/exhaustive-deps
        [returnTo, fromWorkspace],
    )

    const invoiceColumns = React.useMemo<ColumnDef<PurchaseInvoiceRow>[]>(
        () => [
            {
                id: "doc",
                header: "进项发票",
                meta: { label: "发票", width: "reference" },
                cell: ({ row }) => (
                    <div className="text-sm">
                        <div className="font-medium">
                            <span className="num">
                                {row.original.invoiceCode}-
                                {row.original.invoiceNo}
                            </span>
                            <Badge variant="neutral" className="ml-2">
                                {row.original.invoiceKindLabel}
                            </Badge>
                        </div>
                        <div className="text-xs text-muted-foreground">
                            {row.original.supplierName}
                        </div>
                    </div>
                ),
            },
            {
                id: "amount",
                header: "含税 / 未分配",
                meta: {
                    label: "金额",
                    width: "amount",
                    align: "end",
                    numeric: true,
                },
                cell: ({ row }) => (
                    <div className="text-end text-sm">
                        <MoneyValue
                            value={row.original.grossAmount}
                            taxBasis="gross"
                        />
                        <div className="text-xs text-muted-foreground">
                            未分配{" "}
                            <MoneyValue
                                value={row.original.unallocatedAmount}
                            />
                        </div>
                    </div>
                ),
            },
            {
                id: "alloc",
                header: "净已分配",
                meta: {
                    label: "分配",
                    width: "amount",
                    align: "end",
                    numeric: true,
                },
                cell: ({ row }) => (
                    <div className="text-end">
                        <MoneyValue value={row.original.allocatedTotal} />
                    </div>
                ),
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
                        description="与付款进度独立"
                    />
                ),
            },
            {
                id: "actions",
                header: "操作",
                meta: { label: "操作", width: "default", align: "end" },
                cell: ({ row }) => (
                    <div className="flex flex-wrap justify-end gap-1">
                        {row.original.allowedActions.includes(
                            "CONTINUE_ALLOCATE",
                        ) ? (
                            <Button
                                type="button"
                                size="xs"
                                onClick={() =>
                                    openSession({
                                        track: "purchase_invoice",
                                        supplierId: row.original.supplierId,
                                        existingInvoiceId:
                                            row.original.invoiceId,
                                    })
                                }
                            >
                                继续核销
                            </Button>
                        ) : null}
                        {row.original.allowedActions.includes("RED_INVOICE") ? (
                            <Button
                                type="button"
                                size="xs"
                                variant="outline"
                                onClick={() => {
                                    setRedInvoiceNo(
                                        `R${row.original.invoiceNo}`,
                                    )
                                    setReverseTarget({
                                        kind: "invoice",
                                        id: row.original.invoiceId,
                                        no: `${row.original.invoiceCode}-${row.original.invoiceNo}`,
                                    })
                                }}
                            >
                                红票
                            </Button>
                        ) : null}
                    </div>
                ),
            },
        ],
        // eslint-disable-next-line react-hooks/exhaustive-deps
        [],
    )

    const unallocatedColumns = React.useMemo<ColumnDef<UnallocatedRow>[]>(
        () => [
            {
                id: "track",
                header: "轨道",
                meta: { label: "轨道", width: "default" },
                cell: ({ row }) => (
                    <Badge
                        variant={
                            row.original.track === "payment"
                                ? "warning"
                                : "info"
                        }
                    >
                        {row.original.trackLabel}
                    </Badge>
                ),
            },
            {
                id: "doc",
                header: "单据 / 供应商",
                meta: { label: "单据", width: "reference" },
                cell: ({ row }) => (
                    <div className="text-sm">
                        <div className="num font-medium">
                            {row.original.documentNo}
                        </div>
                        <div className="text-xs text-muted-foreground">
                            {row.original.supplierName}
                        </div>
                    </div>
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
                    <div className="text-end">
                        <MoneyValue
                            value={row.original.unallocatedAmount}
                            taxBasis="gross"
                        />
                        <div className="text-xs text-muted-foreground">
                            记录 <MoneyValue value={row.original.amount} />
                        </div>
                    </div>
                ),
            },
            {
                id: "actions",
                header: "操作",
                meta: { label: "操作", width: "default", align: "end" },
                cell: ({ row }) => {
                    const payment = data?.payments.find(
                        (p) => p.paymentNo === row.original.documentNo,
                    )
                    const invoice = data?.invoices.find(
                        (p) =>
                            `${p.invoiceCode}-${p.invoiceNo}` ===
                            row.original.documentNo,
                    )
                    const resolved =
                        row.original.track === "payment" ? payment : invoice
                    return (
                        <Button
                            type="button"
                            size="xs"
                            disabled={!resolved}
                            title={
                                resolved
                                    ? undefined
                                    : "未找到原付款/发票，请回到对应视图操作"
                            }
                            onClick={() =>
                                openSession({
                                    track: row.original.track,
                                    supplierId: row.original.supplierId,
                                    existingPaymentId:
                                        row.original.track === "payment"
                                            ? payment?.paymentId
                                            : undefined,
                                    existingInvoiceId:
                                        row.original.track ===
                                        "purchase_invoice"
                                            ? invoice?.invoiceId
                                            : undefined,
                                })
                            }
                        >
                            继续核销
                        </Button>
                    )
                },
            },
        ],
        // eslint-disable-next-line react-hooks/exhaustive-deps
        [data?.payments, data?.invoices],
    )

    return {
        payableColumns,
        paymentColumns,
        invoiceColumns,
        unallocatedColumns,
    }
}
