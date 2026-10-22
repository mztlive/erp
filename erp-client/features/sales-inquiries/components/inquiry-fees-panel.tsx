"use client"

import * as React from "react"
import { useStore } from "@tanstack/react-form"
import { useQueries, useQuery } from "@tanstack/react-query"

import {
    BusinessFailureState,
    FormalActionConfirmDialog,
    MoneyValue,
    RateValue,
    TableRowActions,
} from "@/components/business"
import { useAppForm, toFieldErrors } from "@/components/form"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Button } from "@/components/ui/button"
import {
    Dialog,
    DialogContent,
    DialogHeader,
    DialogTitle,
    DialogDescription,
} from "@/components/ui/dialog"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { Field, FieldError, FieldLabel } from "@/components/ui/field"
import {
    Table,
    TableBody,
    TableCell,
    TableHead,
    TableHeader,
    TableRow,
} from "@/components/ui/table"
import { SupplierSearchCombobox } from "@/features/entity-selectors/components/supplier-search-combobox"
import { fetchExpenseSource } from "@/features/purchase-orders/api/expense-sources"
import { expensePurchaseCommandBody } from "@/features/purchase-orders/api/purchase-expenses"
import type { BackendCreateResult } from "@/features/purchase-orders/api/purchase-order-wire-types"
import {
    PURCHASE_EXPENSE_CATEGORY_LABEL,
    type PurchaseExpenseCategory,
} from "@/features/purchase-orders/types"
import { getErrorMessage } from "@/lib/api/errors"
import {
    SUPPLIER_PAYMENT_TERM_OPTIONS,
    paymentTermLabel,
} from "@/lib/business-options"
import {
    compareDecimal,
    minFixed,
    multiplyFixed,
    subtractFixed,
    sumFixed,
} from "@/lib/fixed-decimal"
import { hasPermission } from "@/lib/permissions"
import { fetchInquiryFeeSales, inquiryPath } from "../api"
import { useInquiryCommand } from "../hooks/use-inquiry-command"
import { inquiryDate } from "../lib/format"
import {
    inquiryStorageKey,
    readInquiryLocalValue,
    writeInquiryLocalValue,
} from "../lib/local-draft"
import type {
    InquiryFee,
    InquiryFeeAllocation,
    InquiryFeeSalesSource,
    ProcurementInquiry,
    SalesInquiry,
} from "../types"

type FeeInquiry = SalesInquiry | ProcurementInquiry

type FeeDraft = {
    action: "allocate" | "expense"
    fee_id: string
    sales_order_id: string
    allocation_amount: string
    expense_allocation_id: string
    expense_amount: string
    supplier_id: string
    supplier_name: string
    payment_term_code: string
    category: PurchaseExpenseCategory
}

const EMPTY_DRAFT: FeeDraft = {
    action: "allocate",
    fee_id: "",
    sales_order_id: "",
    allocation_amount: "",
    expense_allocation_id: "",
    expense_amount: "",
    supplier_id: "",
    supplier_name: "",
    payment_term_code: "",
    category: "other",
}
const DECIMAL_OPTIONS = { maxScale: 2, outputScale: 2 } as const

function sumAmounts(values: readonly string[]): string {
    return sumFixed(values, DECIMAL_OPTIONS)
}

function remaining(amount: string, used: readonly string[]): string {
    return subtractFixed(amount, sumAmounts(used), DECIMAL_OPTIONS)
}

function positiveAmount(value: string): boolean {
    try {
        return (
            /^(?:0|[1-9]\d*)(?:\.\d{1,2})?$/.test(value.trim()) &&
            compareDecimal(value.trim(), "0", 2) > 0
        )
    } catch {
        return false
    }
}

function taxPercent(value: string): string {
    return multiplyFixed(value, "100", {
        leftMaxScale: 6,
        rightMaxScale: 0,
        outputScale: 2,
    })
}

function feeUnavailable(
    inquiry: FeeInquiry,
    fee: InquiryFee,
    round: number,
): string | undefined {
    if (inquiry.status === "closed") return "询价已关闭，不能继续办理费用"
    const status = inquiry.fee_statuses.find(
        (status) =>
            status.fee_id === fee.fee_id && status.feedback_round === round,
    )
    if (!status) return "费用办理资格尚未核对，请刷新后重试"
    if (status.unavailable_reason) return status.unavailable_reason
    if (fee.valid_until <= Math.floor(Date.now() / 1000))
        return "费用结果已过期，请重新询价"
    return undefined
}

type FeeSummary = {
    fee: InquiryFee
    unallocated: string
    reserved: string
    handled: string
    available: string
    reason?: string
}
type AllocationSummary = {
    allocation: InquiryFeeAllocation
    fee: InquiryFee
    balance: string
    reason?: string
}
type FeeConfirmation = {
    kind: "allocate" | "expense"
    path: string
    body: Record<string, unknown>
    fee_name: string
    sales_order_no: string
    amount: string
    supplier_name?: string
    payment_term_code?: string
}

/** 销售只读费用和办理进度；采购收款资料仅在采购作业面加载和填写。 */
export function InquiryFeesPanel({
    inquiry,
    userId,
    permissions,
    procurement = false,
    onChanged,
}: {
    inquiry: FeeInquiry
    userId: string
    permissions: readonly string[]
    procurement?: boolean
    onChanged?: () => void
}) {
    const storageKey = inquiryStorageKey(
        userId,
        `${inquiry.id}:${procurement ? "procurement" : "sales"}:fee-draft`,
    )
    const [initial, setInitial] = React.useState<FeeDraft | null>(null)
    React.useEffect(() => {
        setInitial({
            ...EMPTY_DRAFT,
            ...readInquiryLocalValue<FeeDraft>(storageKey),
        })
    }, [storageKey])
    return initial ? (
        <InquiryFeesContent
            key={storageKey}
            inquiry={inquiry}
            userId={userId}
            permissions={permissions}
            procurement={procurement}
            onChanged={onChanged}
            storageKey={storageKey}
            initial={initial}
        />
    ) : (
        <p role="status">正在恢复费用输入…</p>
    )
}

function InquiryFeesContent({
    inquiry,
    userId,
    permissions,
    procurement,
    onChanged,
    storageKey,
    initial,
}: {
    inquiry: FeeInquiry
    userId: string
    permissions: readonly string[]
    procurement: boolean
    onChanged?: () => void
    storageKey: string
    initial: FeeDraft
}) {
    const allocationCommand = useInquiryCommand(
        userId,
        `${inquiry.id}:fee-allocations`,
    )
    const expenseCommand = useInquiryCommand(
        userId,
        `${inquiry.id}:fee-expense`,
    )
    const canAllocate =
        !procurement && hasPermission(permissions, "sales_inquiry:update")
    const canCreateExpense =
        procurement && hasPermission(permissions, "purchase_order:create")
    const sales = useQuery({
        queryKey: ["sales-inquiries", "fee-sales", inquiry.id],
        queryFn: () => fetchInquiryFeeSales(inquiry.id),
        enabled:
            !procurement && hasPermission(permissions, "sales_inquiry:detail"),
        staleTime: 0,
    })
    const linkedIds = [
        ...new Set([
            ...("quantity_ledger" in inquiry ? inquiry.quantity_ledger : [])
                .filter((row) => row.state !== "released")
                .map((row) => row.sales_order_id),
            ...(inquiry.fee_allocations ?? []).map((row) => row.sales_order_id),
        ]),
    ]
    const procurementSales = useQueries({
        queries: linkedIds.map((id) => ({
            queryKey: ["purchase-orders", "expense-sources", "detail", id],
            queryFn: () => fetchExpenseSource(id),
            enabled: canCreateExpense,
            staleTime: 0,
            retry: false,
        })),
    })
    const sources: InquiryFeeSalesSource[] = procurement
        ? procurementSales.flatMap((query) =>
              query.data && !query.isError
                  ? [{ ...query.data, eligible_for_fee_allocation: true }]
                  : [],
          )
        : (sales.data ?? [])
    const sourceById = new Map(sources.map((source) => [source.id, source]))
    const sourceLoading = procurement
        ? canCreateExpense && procurementSales.some((query) => query.isFetching)
        : sales.isFetching
    const sourceError = procurement
        ? procurementSales.find((query) => query.isError)?.error
        : sales.error
    const acceptedRound =
        "acceptances" in inquiry
            ? inquiry.acceptances.at(-1)?.feedback_round
            : undefined
    const feeRound =
        acceptedRound ??
        inquiry.fee_statuses[0]?.feedback_round ??
        inquiry.feedback_round
    const feedback = inquiry.feedbacks.find((row) => row.round === feeRound)
    const round = feedback?.round ?? 0
    const summaries: FeeSummary[] = (feedback?.data.fees ?? []).map((fee) => {
        const ledger = (inquiry.fee_ledger ?? []).filter(
            (row) => row.fee_id === fee.fee_id,
        )
        const reserved = sumAmounts(
            ledger
                .filter((row) => row.state === "reserved")
                .map((row) => row.amount),
        )
        const handled = sumAmounts(
            ledger
                .filter((row) => row.state === "handled")
                .map((row) => row.amount),
        )
        return {
            fee,
            reserved,
            handled,
            unallocated: remaining(
                fee.amount_gross,
                (inquiry.fee_allocations ?? [])
                    .filter((row) => row.fee_id === fee.fee_id)
                    .map((row) => row.amount),
            ),
            available: remaining(fee.amount_gross, [reserved, handled]),
            reason: feeUnavailable(inquiry, fee, round),
        }
    })
    const byFee = new Map(
        summaries.map((summary) => [summary.fee.fee_id, summary]),
    )
    const allocationRows: AllocationSummary[] = (
        inquiry.fee_allocations ?? []
    ).flatMap((allocation) => {
        const summary = byFee.get(allocation.fee_id)
        if (!summary) return []
        const balance = minFixed(
            remaining(
                allocation.amount,
                (inquiry.fee_ledger ?? [])
                    .filter(
                        (row) =>
                            row.allocation_id === allocation.allocation_id &&
                            row.state !== "released",
                    )
                    .map((row) => row.amount),
            ),
            summary.available,
            DECIMAL_OPTIONS,
        )
        const reason =
            summary.reason ??
            (compareDecimal(balance, "0", 2) <= 0
                ? "本笔分摊已无可转余额"
                : undefined) ??
            (!sourceById.has(allocation.sales_order_id) && !sourceLoading
                ? "归属销售单尚未生效或不在当前费用办理范围"
                : undefined)
        return [{ allocation, fee: summary.fee, balance, reason }]
    })
    const [confirmation, setConfirmation] =
        React.useState<FeeConfirmation | null>(null)
    const [editor, setEditor] = React.useState<"allocate" | "expense" | null>(
        null,
    )
    const editorTrigger = React.useRef<HTMLButtonElement | null>(null)
    const [storageFailure, setStorageFailure] = React.useState(false)
    const [success, setSuccess] = React.useState<string | null>(null)
    const form = useAppForm({
        defaultValues: initial,
        listeners: {
            onChange: ({ formApi }) => {
                try {
                    writeInquiryLocalValue(storageKey, formApi.state.values)
                    setStorageFailure(false)
                } catch {
                    setStorageFailure(true)
                }
            },
        },
        validators: {
            onSubmit: ({ value }) => {
                if (value.action === "allocate") {
                    const summary = byFee.get(value.fee_id)
                    if (!summary)
                        return { fields: { fee_id: "请选择本轮费用" } }
                    if (summary.reason)
                        return { fields: { fee_id: summary.reason } }
                    if (
                        !sourceById.get(value.sales_order_id)
                            ?.eligible_for_fee_allocation
                    )
                        return {
                            fields: {
                                sales_order_id: "请选择关联且已生效的销售单",
                            },
                        }
                    if (!positiveAmount(value.allocation_amount))
                        return {
                            fields: {
                                allocation_amount:
                                    "填写大于 0 且最多两位小数的金额",
                            },
                        }
                    if (
                        compareDecimal(
                            value.allocation_amount,
                            summary.unallocated,
                            2,
                        ) > 0
                    )
                        return {
                            fields: {
                                allocation_amount:
                                    "分摊金额超过本项费用未分摊余额",
                            },
                        }
                } else {
                    const row = allocationRows.find(
                        (row) =>
                            row.allocation.allocation_id ===
                            value.expense_allocation_id,
                    )
                    if (!row)
                        return {
                            fields: {
                                expense_allocation_id:
                                    "请选择已明确销售归属的费用",
                            },
                        }
                    if (row.reason)
                        return { fields: { expense_allocation_id: row.reason } }
                    if (!positiveAmount(value.expense_amount))
                        return {
                            fields: {
                                expense_amount:
                                    "填写大于 0 且最多两位小数的金额",
                            },
                        }
                    if (
                        compareDecimal(value.expense_amount, row.balance, 2) > 0
                    )
                        return {
                            fields: {
                                expense_amount:
                                    "转换金额超过本笔分摊的可用余额",
                            },
                        }
                    if (!value.supplier_id)
                        return { fields: { supplier_id: "请选择收款供应商" } }
                    if (!value.payment_term_code)
                        return {
                            fields: { payment_term_code: "请选择付款条件" },
                        }
                }
                return undefined
            },
        },
        onSubmit: ({ value }) => {
            if (storageFailure || sourceLoading) return
            if (value.action === "allocate") {
                if (!canAllocate || allocationCommand.unresolved) return
                const summary = byFee.get(value.fee_id)!
                const source = sourceById.get(value.sales_order_id)!
                setConfirmation({
                    kind: "allocate",
                    fee_name: summary.fee.name,
                    sales_order_no: source.order_no,
                    amount: value.allocation_amount.trim(),
                    path: `${inquiryPath(false, inquiry.id)}/fee-allocations`,
                    body: {
                        expected_version: inquiry.version,
                        allocations: [
                            {
                                allocation_id: crypto.randomUUID(),
                                fee_id: value.fee_id,
                                sales_order_id: value.sales_order_id,
                                feedback_round: round,
                                amount: value.allocation_amount.trim(),
                            },
                        ],
                    },
                })
                setEditor(null)
            } else {
                if (!canCreateExpense || expenseCommand.unresolved) return
                const row = allocationRows.find(
                    (row) =>
                        row.allocation.allocation_id ===
                        value.expense_allocation_id,
                )!
                const source = sourceById.get(row.allocation.sales_order_id)!
                setConfirmation({
                    kind: "expense",
                    fee_name: row.fee.name,
                    sales_order_no: source.order_no,
                    amount: value.expense_amount.trim(),
                    supplier_name: value.supplier_name || "已选择的收款供应商",
                    payment_term_code: value.payment_term_code,
                    path: `/admin/sales-orders/${encodeURIComponent(row.allocation.sales_order_id)}/expense-purchases`,
                    body: expensePurchaseCommandBody({
                        supplierId: value.supplier_id,
                        paymentTermCode: value.payment_term_code,
                        lines: [
                            {
                                category: value.category,
                                description: row.fee.name,
                                grossAmount: value.expense_amount.trim(),
                                inputTaxRate: row.fee.tax_rate,
                                inquirySource: {
                                    inquiry_id: inquiry.id,
                                    feedback_round: round,
                                    fee_id: row.fee.fee_id,
                                    allocation_id: row.allocation.allocation_id,
                                },
                            },
                        ],
                    }),
                })
                setEditor(null)
            }
        },
    })
    const values = useStore(form.store, (state) => state.values)
    const selectedFee = byFee.get(values.fee_id)
    const selectedAllocation = allocationRows.find(
        (row) => row.allocation.allocation_id === values.expense_allocation_id,
    )
    const pending = allocationCommand.isPending || expenseCommand.isPending
    const locked =
        pending ||
        storageFailure ||
        !allocationCommand.ready ||
        !expenseCommand.ready ||
        allocationCommand.unresolved ||
        expenseCommand.unresolved
    const error = allocationCommand.error ?? expenseCommand.error
    const actionFailure = !canAllocate
        ? "当前账号不能保存费用归属，请由销售负责人办理"
        : !sources.some((source) => source.eligible_for_fee_allocation)
          ? "尚无关联且已生效的销售单，可在销售单生效后明确费用归属"
          : undefined

    return (
        <section className="space-y-5" aria-label="询价费用归属与办理">
            <div>
                <h2 className="text-lg font-semibold">费用与办理</h2>
                <p className="mt-1 text-sm text-muted-foreground">
                    先按金额明确费用归属，再由采购办理。暂估费用仅供定价参考；收款对象和付款条件不同的费用分别建单。
                </p>
                {round > 0 && (
                    <p className="mt-1 text-xs text-muted-foreground">
                        费用依据：第 {round} 轮
                        {acceptedRound != null
                            ? "已接受反馈。"
                            : procurement
                              ? "反馈，办理资格以各项费用为准。"
                              : "反馈（尚未接受，仅供查看）。"}
                        {acceptedRound != null &&
                        inquiry.feedback_round > acceptedRound
                            ? "新反馈尚未接受，继续按已接受结果办理未受影响的费用；暂停办理的费用会显示具体原因。"
                            : ""}
                    </p>
                )}
            </div>
            {!summaries.length ? (
                <p className="text-sm text-muted-foreground">
                    暂无可展示的费用结果。采购发布额外费用后，可在此查看分摊与办理进度。
                </p>
            ) : (
                <>
                    <div className="overflow-x-auto rounded-lg border">
                        <Table>
                            <TableHeader>
                                <TableRow>
                                    <TableHead>费用及条件</TableHead>
                                    <TableHead>反馈金额</TableHead>
                                    <TableHead>未分摊</TableHead>
                                    <TableHead>办理中</TableHead>
                                    <TableHead>已办理</TableHead>
                                    <TableHead>可用余额</TableHead>
                                    {canAllocate && (
                                        <TableHead className="text-right">
                                            操作
                                        </TableHead>
                                    )}
                                </TableRow>
                            </TableHeader>
                            <TableBody>
                                {summaries.map((summary) => (
                                    <TableRow key={summary.fee.fee_id}>
                                        <TableCell className="min-w-64 whitespace-normal">
                                            <p className="font-medium">
                                                {summary.fee.name}{" "}
                                            </p>
                                            <p className="mt-1 text-xs text-muted-foreground">
                                                税率{" "}
                                                <RateValue
                                                    value={taxPercent(
                                                        summary.fee.tax_rate,
                                                    )}
                                                    precision={2}
                                                />{" "}
                                                · 有效至{" "}
                                                {inquiryDate(
                                                    summary.fee.valid_until,
                                                )}
                                            </p>
                                            <p className="mt-1 text-xs text-muted-foreground">
                                                {summary.reason}{" "}
                                                {summary.fee.public_note}
                                            </p>
                                        </TableCell>
                                        <TableCell>
                                            <MoneyValue
                                                value={summary.fee.amount_gross}
                                                display="exact"
                                            />
                                        </TableCell>
                                        <TableCell>
                                            <MoneyValue
                                                value={summary.unallocated}
                                                display="exact"
                                            />
                                        </TableCell>
                                        <TableCell>
                                            <MoneyValue
                                                value={summary.reserved}
                                                display="exact"
                                            />
                                        </TableCell>
                                        <TableCell>
                                            <MoneyValue
                                                value={summary.handled}
                                                display="exact"
                                            />
                                        </TableCell>
                                        <TableCell>
                                            <MoneyValue
                                                value={summary.available}
                                                display="exact"
                                            />
                                        </TableCell>
                                        {canAllocate && (
                                            <TableCell>
                                                <TableRowActions
                                                    moreId={`inquiry-fee-${toAutomationIdSegment(summary.fee.fee_id)}-more`}
                                                    moreLabel={`${summary.fee.name}操作`}
                                                    actions={[
                                                        {
                                                            id: `inquiry-fee-${toAutomationIdSegment(summary.fee.fee_id)}-allocate`,
                                                            label: "分摊归属",
                                                            disabled:
                                                                locked ||
                                                                sourceLoading ||
                                                                Boolean(
                                                                    actionFailure,
                                                                ) ||
                                                                Boolean(
                                                                    summary.reason,
                                                                ) ||
                                                                compareDecimal(
                                                                    summary.unallocated,
                                                                    "0",
                                                                    2,
                                                                ) <= 0,
                                                            disabledReason:
                                                                summary.reason ??
                                                                actionFailure ??
                                                                (compareDecimal(
                                                                    summary.unallocated,
                                                                    "0",
                                                                    2,
                                                                ) <= 0
                                                                    ? "本项费用已全部分摊"
                                                                    : undefined),
                                                            onClick: (
                                                                event,
                                                            ) => {
                                                                editorTrigger.current =
                                                                    event.currentTarget as HTMLButtonElement
                                                                form.setFieldValue(
                                                                    "action",
                                                                    "allocate",
                                                                )
                                                                if (
                                                                    form.state
                                                                        .values
                                                                        .fee_id !==
                                                                    summary.fee
                                                                        .fee_id
                                                                ) {
                                                                    form.setFieldValue(
                                                                        "fee_id",
                                                                        summary
                                                                            .fee
                                                                            .fee_id,
                                                                    )
                                                                    form.setFieldValue(
                                                                        "allocation_amount",
                                                                        "",
                                                                    )
                                                                }
                                                                setEditor(
                                                                    "allocate",
                                                                )
                                                            },
                                                        },
                                                    ]}
                                                />
                                            </TableCell>
                                        )}
                                    </TableRow>
                                ))}
                            </TableBody>
                        </Table>
                    </div>
                    {sourceError ? (
                        <BusinessFailureState
                            id="inquiry-fees-sources-retry"
                            title="部分归属销售单暂不可用"
                            error={sourceError}
                            onRetry={() => {
                                if (procurement)
                                    procurementSales.forEach(
                                        (query) => void query.refetch(),
                                    )
                                else void sales.refetch()
                            }}
                        />
                    ) : null}
                    {storageFailure ? (
                        <Alert variant="destructive">
                            <AlertDescription>
                                费用输入无法保存到本机，请检查浏览器存储后重试；当前输入仍保留，尚未提交。
                            </AlertDescription>
                        </Alert>
                    ) : null}
                    {error ? (
                        <Alert variant="destructive">
                            <AlertDescription>
                                {getErrorMessage(
                                    error,
                                    "费用操作失败，请保留输入后重试",
                                )}
                            </AlertDescription>
                        </Alert>
                    ) : null}
                    {success ? (
                        <Alert variant="success">
                            <AlertDescription>{success}</AlertDescription>
                        </Alert>
                    ) : null}
                    {allocationCommand.unresolved ||
                    expenseCommand.unresolved ? (
                        <Alert variant="warning">
                            <AlertTitle>上次费用操作结果待确认</AlertTitle>
                            <AlertDescription>
                                已保留原操作内容，请先核对结果再继续办理。
                            </AlertDescription>
                            <div className="mt-2 flex gap-2">
                                {allocationCommand.unresolved ? (
                                    <Button
                                        id="inquiry-fees-recover-allocation"
                                        disabled={pending}
                                        onClick={async () => {
                                            try {
                                                await allocationCommand.recover()
                                                setSuccess(
                                                    "已查到原费用归属操作结果。",
                                                )
                                                onChanged?.()
                                            } catch {
                                                // 原命令和错误由 Query 保留，允许继续核对。
                                            }
                                        }}
                                    >
                                        核对费用归属结果
                                    </Button>
                                ) : null}
                                {expenseCommand.unresolved ? (
                                    <Button
                                        id="inquiry-fees-recover-expense"
                                        disabled={pending}
                                        onClick={async () => {
                                            try {
                                                const result =
                                                    await expenseCommand.recover<BackendCreateResult>()
                                                setSuccess(
                                                    result
                                                        ? `费用采购 ${result.purchase_no} 已创建并提交审批。`
                                                        : "已核对原费用采购操作结果。",
                                                )
                                                onChanged?.()
                                            } catch {
                                                // 原命令和错误由 Query 保留，允许继续核对。
                                            }
                                        }}
                                    >
                                        核对费用采购结果
                                    </Button>
                                ) : null}
                            </div>
                        </Alert>
                    ) : null}
                    <Dialog
                        open={editor === "allocate"}
                        onOpenChange={(open) => {
                            if (!open && !pending) setEditor(null)
                        }}
                    >
                        <DialogContent
                            closeButtonId="inquiry-fees-allocation-close"
                            finalFocus={editorTrigger}
                            className="max-h-[85dvh] overflow-y-auto sm:max-w-2xl"
                        >
                            <DialogHeader>
                                <DialogTitle>分摊费用归属</DialogTitle>
                                <DialogDescription>
                                    选择归属销售单并填写本次分摊金额，预览核对后保存。
                                </DialogDescription>
                            </DialogHeader>
                            <form
                                className="space-y-4"
                                onSubmit={(event) => {
                                    event.preventDefault()
                                    form.setFieldValue("action", "allocate")
                                    void form.handleSubmit()
                                }}
                            >
                                <div className="grid gap-4 lg:grid-cols-3">
                                    <form.AppField name="fee_id">
                                        {(field) => (
                                            <field.SelectField
                                                id="inquiry-fees-allocation-fee"
                                                label="费用"
                                                disabled={
                                                    locked || !canAllocate
                                                }
                                                options={summaries.map(
                                                    (summary) => ({
                                                        value: summary.fee
                                                            .fee_id,
                                                        label: `${summary.fee.name}${summary.reason ? ` · ${summary.reason}` : ""}`,
                                                        disabled:
                                                            Boolean(
                                                                summary.reason,
                                                            ) ||
                                                            compareDecimal(
                                                                summary.unallocated,
                                                                "0",
                                                                2,
                                                            ) <= 0,
                                                    }),
                                                )}
                                            />
                                        )}
                                    </form.AppField>
                                    <form.AppField name="sales_order_id">
                                        {(field) => (
                                            <field.SelectField
                                                id="inquiry-fees-allocation-sales"
                                                label="归属销售单"
                                                disabled={
                                                    locked || !canAllocate
                                                }
                                                loading={sourceLoading}
                                                options={sources.map(
                                                    (source) => ({
                                                        value: source.id,
                                                        label: `${source.order_no} · ${source.status_label}`,
                                                        disabled:
                                                            !source.eligible_for_fee_allocation,
                                                    }),
                                                )}
                                            />
                                        )}
                                    </form.AppField>
                                    <form.AppField name="allocation_amount">
                                        {(field) => (
                                            <field.TextField
                                                id="inquiry-fees-allocation-amount"
                                                label="分摊含税金额"
                                                inputMode="decimal"
                                                disabled={
                                                    locked || !canAllocate
                                                }
                                                placeholder="0.00"
                                            />
                                        )}
                                    </form.AppField>
                                </div>
                                {selectedFee ? (
                                    <p className="text-sm text-muted-foreground">
                                        本项尚未分摊：
                                        <MoneyValue
                                            value={selectedFee.unallocated}
                                            display="exact"
                                        />
                                        。{selectedFee.reason}
                                    </p>
                                ) : null}
                                {actionFailure ? (
                                    <p className="text-sm text-muted-foreground">
                                        {actionFailure}
                                    </p>
                                ) : null}
                                <Button
                                    id="inquiry-fees-allocation-preview"
                                    type="submit"
                                    disabled={
                                        locked ||
                                        !canAllocate ||
                                        sourceLoading ||
                                        Boolean(actionFailure)
                                    }
                                >
                                    预览费用归属
                                </Button>
                            </form>
                        </DialogContent>
                    </Dialog>
                    {allocationRows.length ? (
                        <div className="overflow-x-auto rounded-lg border">
                            <Table>
                                <TableHeader>
                                    <TableRow>
                                        <TableHead>费用</TableHead>
                                        <TableHead>归属销售单</TableHead>
                                        <TableHead>已分摊金额</TableHead>
                                        <TableHead>本笔可用</TableHead>
                                        <TableHead>办理条件</TableHead>
                                        {canCreateExpense && (
                                            <TableHead className="text-right">
                                                操作
                                            </TableHead>
                                        )}
                                    </TableRow>
                                </TableHeader>
                                <TableBody>
                                    {allocationRows.map((row) => (
                                        <TableRow
                                            key={row.allocation.allocation_id}
                                        >
                                            <TableCell>
                                                {row.fee.name}
                                            </TableCell>
                                            <TableCell>
                                                {sourceById.get(
                                                    row.allocation
                                                        .sales_order_id,
                                                )?.order_no ??
                                                    "关联销售单待核对"}
                                            </TableCell>
                                            <TableCell>
                                                <MoneyValue
                                                    value={
                                                        row.allocation.amount
                                                    }
                                                    display="exact"
                                                />
                                            </TableCell>
                                            <TableCell>
                                                <MoneyValue
                                                    value={row.balance}
                                                    display="exact"
                                                />
                                            </TableCell>
                                            <TableCell className="whitespace-normal">
                                                {row.reason ?? "可转费用采购"}
                                            </TableCell>
                                            {canCreateExpense && (
                                                <TableCell>
                                                    <TableRowActions
                                                        moreId={`inquiry-fee-allocation-${toAutomationIdSegment(row.allocation.allocation_id)}-more`}
                                                        moreLabel={`${row.fee.name}办理操作`}
                                                        actions={[
                                                            {
                                                                id: `inquiry-fee-allocation-${toAutomationIdSegment(row.allocation.allocation_id)}-expense`,
                                                                label: "办理费用采购",
                                                                disabled:
                                                                    locked ||
                                                                    sourceLoading ||
                                                                    Boolean(
                                                                        row.reason,
                                                                    ),
                                                                disabledReason:
                                                                    row.reason,
                                                                onClick: (
                                                                    event,
                                                                ) => {
                                                                    editorTrigger.current =
                                                                        event.currentTarget as HTMLButtonElement
                                                                    form.setFieldValue(
                                                                        "action",
                                                                        "expense",
                                                                    )
                                                                    if (
                                                                        form
                                                                            .state
                                                                            .values
                                                                            .expense_allocation_id !==
                                                                        row
                                                                            .allocation
                                                                            .allocation_id
                                                                    ) {
                                                                        form.setFieldValue(
                                                                            "expense_allocation_id",
                                                                            row
                                                                                .allocation
                                                                                .allocation_id,
                                                                        )
                                                                        form.setFieldValue(
                                                                            "expense_amount",
                                                                            "",
                                                                        )
                                                                        form.setFieldValue(
                                                                            "supplier_id",
                                                                            "",
                                                                        )
                                                                        form.setFieldValue(
                                                                            "supplier_name",
                                                                            "",
                                                                        )
                                                                        form.setFieldValue(
                                                                            "payment_term_code",
                                                                            "",
                                                                        )
                                                                    }
                                                                    setEditor(
                                                                        "expense",
                                                                    )
                                                                },
                                                            },
                                                        ]}
                                                    />
                                                </TableCell>
                                            )}
                                        </TableRow>
                                    ))}
                                </TableBody>
                            </Table>
                        </div>
                    ) : null}
                    {procurement ? (
                        <Dialog
                            open={editor === "expense"}
                            onOpenChange={(open) => {
                                if (!open && !pending) setEditor(null)
                            }}
                        >
                            <DialogContent
                                closeButtonId="inquiry-fees-expense-close"
                                finalFocus={editorTrigger}
                                className="max-h-[85dvh] overflow-y-auto sm:max-w-2xl"
                            >
                                <DialogHeader>
                                    <DialogTitle>办理费用采购</DialogTitle>
                                    <DialogDescription>
                                        核对销售归属、收款供应商和付款条件，预览后提交审批。
                                    </DialogDescription>
                                </DialogHeader>
                                <form
                                    className="space-y-4"
                                    onSubmit={(event) => {
                                        event.preventDefault()
                                        form.setFieldValue("action", "expense")
                                        void form.handleSubmit()
                                    }}
                                >
                                    <p className="text-sm text-muted-foreground">
                                        每次为一笔费用分摊创建费用采购；收款供应商和付款条件须与本轮采购确认的费用依据一致。创建后立即提交审批。
                                    </p>
                                    <div className="grid gap-4 sm:grid-cols-2">
                                        <form.AppField name="expense_allocation_id">
                                            {(field) => (
                                                <field.SelectField
                                                    id="inquiry-fees-expense-source"
                                                    label="选择费用及销售归属"
                                                    disabled={
                                                        locked ||
                                                        !canCreateExpense
                                                    }
                                                    loading={sourceLoading}
                                                    options={allocationRows.map(
                                                        (row) => ({
                                                            value: row
                                                                .allocation
                                                                .allocation_id,
                                                            label: `${row.fee.name} · ${sourceById.get(row.allocation.sales_order_id)?.order_no ?? "销售单待核对"}${row.reason ? ` · ${row.reason}` : ""}`,
                                                            disabled: Boolean(
                                                                row.reason,
                                                            ),
                                                        }),
                                                    )}
                                                />
                                            )}
                                        </form.AppField>
                                        <form.AppField name="expense_amount">
                                            {(field) => (
                                                <field.TextField
                                                    id="inquiry-fees-expense-amount"
                                                    label="本次转换含税金额"
                                                    inputMode="decimal"
                                                    disabled={
                                                        locked ||
                                                        !canCreateExpense
                                                    }
                                                    placeholder="0.00"
                                                />
                                            )}
                                        </form.AppField>
                                        <form.AppField name="supplier_id">
                                            {(field) => (
                                                <Field>
                                                    <FieldLabel htmlFor="inquiry-fees-expense-supplier">
                                                        收款供应商
                                                    </FieldLabel>
                                                    <SupplierSearchCombobox
                                                        id="inquiry-fees-expense-supplier"
                                                        value={
                                                            field.state.value ||
                                                            undefined
                                                        }
                                                        disabled={
                                                            locked ||
                                                            !canCreateExpense
                                                        }
                                                        onValueChange={(id) => {
                                                            field.handleChange(
                                                                id ?? "",
                                                            )
                                                            field.handleBlur()
                                                            form.setFieldValue(
                                                                "supplier_name",
                                                                "",
                                                            )
                                                        }}
                                                        onItemChange={(
                                                            supplier,
                                                        ) =>
                                                            form.setFieldValue(
                                                                "supplier_name",
                                                                supplier?.supplierName ??
                                                                    "",
                                                            )
                                                        }
                                                    />
                                                    <FieldError
                                                        errors={toFieldErrors(
                                                            field.state.meta
                                                                .errors,
                                                        )}
                                                    />
                                                </Field>
                                            )}
                                        </form.AppField>
                                        <form.AppField
                                            name="category"
                                            children={(field) => (
                                                <field.SelectField
                                                    id="inquiry-fee-expense-category"
                                                    label="费用类别"
                                                    options={Object.entries(
                                                        PURCHASE_EXPENSE_CATEGORY_LABEL,
                                                    ).map(([value, label]) => ({
                                                        value,
                                                        label,
                                                    }))}
                                                />
                                            )}
                                        />
                                        <form.AppField name="payment_term_code">
                                            {(field) => (
                                                <field.SelectField
                                                    id="inquiry-fees-expense-payment-term"
                                                    label="付款条件"
                                                    options={
                                                        SUPPLIER_PAYMENT_TERM_OPTIONS
                                                    }
                                                    disabled={
                                                        locked ||
                                                        !canCreateExpense
                                                    }
                                                />
                                            )}
                                        </form.AppField>
                                    </div>
                                    {selectedAllocation ? (
                                        <p className="text-sm text-muted-foreground">
                                            本笔可转：
                                            <MoneyValue
                                                value={
                                                    selectedAllocation.balance
                                                }
                                                display="exact"
                                            />
                                            ；税率沿用已接受费用{" "}
                                            <RateValue
                                                value={taxPercent(
                                                    selectedAllocation.fee
                                                        .tax_rate,
                                                )}
                                                precision={2}
                                            />
                                            。{selectedAllocation.reason}
                                        </p>
                                    ) : null}
                                    {!canCreateExpense ? (
                                        <p className="text-sm text-muted-foreground">
                                            当前账号没有创建费用采购的权限，请由有权限的采购同事办理。
                                        </p>
                                    ) : !allocationRows.length ? (
                                        <p className="text-sm text-muted-foreground">
                                            请先由销售明确费用归属和分摊金额。
                                        </p>
                                    ) : null}
                                    <Button
                                        id="inquiry-fees-expense-preview"
                                        type="submit"
                                        disabled={
                                            locked ||
                                            !canCreateExpense ||
                                            sourceLoading ||
                                            !allocationRows.some(
                                                (row) => !row.reason,
                                            )
                                        }
                                    >
                                        预览费用采购
                                    </Button>
                                </form>
                            </DialogContent>
                        </Dialog>
                    ) : null}
                </>
            )}
            <FormalActionConfirmDialog
                idPrefix="inquiry-fees-confirm"
                open={Boolean(confirmation)}
                onOpenChange={(open) => {
                    if (!open && !pending) setConfirmation(null)
                }}
                cancelLabel="返回修改"
                onCancel={() => setEditor(confirmation?.kind ?? null)}
                title={
                    confirmation?.kind === "expense"
                        ? "核对费用采购"
                        : "核对费用归属"
                }
                actionLabel={
                    confirmation?.kind === "expense"
                        ? "创建费用采购"
                        : "保存费用归属"
                }
                confirmLabel={
                    confirmation?.kind === "expense"
                        ? "创建并提交审批"
                        : "确认保存归属"
                }
                fromStatus={{ label: "已接受费用", tone: "info" }}
                toStatus={{
                    label:
                        confirmation?.kind === "expense"
                            ? "待采购审批"
                            : "归属已明确",
                    tone: "success",
                }}
                showStatusChange={false}
                pending={pending}
                summary={
                    confirmation
                        ? [
                              <span key="fee">
                                  费用：{confirmation.fee_name}
                              </span>,
                              <span key="sales">
                                  归属销售单：{confirmation.sales_order_no}
                              </span>,
                              <span key="amount">
                                  含税金额：
                                  <MoneyValue
                                      value={confirmation.amount}
                                      display="exact"
                                  />
                              </span>,
                              ...(confirmation.kind === "expense"
                                  ? [
                                        <span key="supplier">
                                            收款供应商：
                                            {confirmation.supplier_name}
                                        </span>,
                                        <span key="payment">
                                            付款条件：
                                            {paymentTermLabel(
                                                confirmation.payment_term_code ??
                                                    "",
                                            )}
                                        </span>,
                                    ]
                                  : []),
                          ]
                        : []
                }
                effects={
                    confirmation?.kind === "expense"
                        ? [
                              "核对来源余额、供应商结算和审批资格后，创建一张费用采购并立即提交审批。",
                              "审批中、撤回或驳回仍保留费用占用；已办理金额不能直接释放。",
                          ]
                        : [
                              "新增这笔费用对本销售单的金额归属；其他销售单不会重复复制整笔费用。",
                          ]
                }
                onConfirm={async () => {
                    if (!confirmation) return
                    if (confirmation.kind === "expense") {
                        const result =
                            await expenseCommand.run<BackendCreateResult>(
                                confirmation.path,
                                confirmation.body,
                                "POST",
                                "idempotency_key",
                            )
                        setSuccess(
                            `费用采购 ${result.purchase_no} 已创建并提交审批。`,
                        )
                        form.setFieldValue("expense_amount", "")
                    } else {
                        await allocationCommand.run(
                            confirmation.path,
                            confirmation.body,
                        )
                        setSuccess("费用归属已保存，可继续分摊剩余金额。")
                        form.setFieldValue("allocation_amount", "")
                    }
                    setConfirmation(null)
                    setEditor(null)
                    onChanged?.()
                }}
            />
        </section>
    )
}
