"use client"

import * as React from "react"
import { ArrowLeftIcon, ChevronRightIcon, LockIcon } from "lucide-react"
import { z } from "zod"

import {
    BusinessFailureState,
    MoneyValue,
    RateValue,
    detailWorkspaceStyles,
} from "@/components/business"
import { toFieldErrors, useAppForm } from "@/components/form"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { DialogFooter } from "@/components/ui/dialog"
import {
    Field,
    FieldDescription,
    FieldError,
    FieldLabel,
} from "@/components/ui/field"
import { LoadingButton } from "@/components/ui/loading-button"
import { RequiredMark } from "@/components/ui/required-mark"
import { StatusBadge } from "@/components/ui/status-badge"
import { useAccountProfileQuery } from "@/features/auth/queries"
import { SupplierSearchCombobox } from "@/features/entity-selectors/components/supplier-search-combobox"
import { getErrorMessage } from "@/lib/api/errors"
import { toAutomationIdSegment } from "@/lib/automation-id"
import {
    SUPPLIER_PAYMENT_TERM_OPTIONS,
    paymentTermLabel,
} from "@/lib/business-options"
import {
    compactFixed,
    compareDecimal,
    multiplyFixed,
    splitFeeGrossByPercentRate,
    subtractFixed,
} from "@/lib/fixed-decimal"
import { classifyFormalCommandError } from "@/lib/formal-command"
import { hasPermission } from "@/lib/permissions"
import { cn } from "@/lib/utils"
import type { InquiryExpenseCandidate } from "../api/expense-sources"
import type { CreateExpensePurchaseInput } from "../api/purchase-expenses"
import { useInquiryExpenseCandidatesQuery } from "../hooks/expense-source-queries"
import { useCreateExpensePurchaseMutation } from "../hooks/queries"
import {
    PURCHASE_EXPENSE_CATEGORY_LABEL,
    type PurchaseExpenseCategory,
} from "../types"
import type { ExpensePurchaseCreated } from "./expense-purchase-create-dialog"

const CATEGORY_OPTIONS = (
    Object.keys(PURCHASE_EXPENSE_CATEGORY_LABEL) as PurchaseExpenseCategory[]
).map((value) => ({
    value,
    label: PURCHASE_EXPENSE_CATEGORY_LABEL[value],
}))

const amountPattern = /^(?:0|[1-9]\d*)(?:\.\d{1,2})?$/

function candidateReason(candidate: InquiryExpenseCandidate): string | null {
    return candidate.unavailable_reason ?? null
}

function formatRatePercent(rate: string): string {
    try {
        const percent = multiplyFixed(rate, "100", {
            leftMaxScale: 6,
            rightMaxScale: 0,
            outputScale: 2,
        })
        return `${compactFixed(percent)}%`
    } catch {
        return "—"
    }
}

/** 管理费用采购单的询价费用候选、待确认命令恢复与转换提交，与弹窗共享单一状态源。 */
export function useInquiryExpenseSourceState({
    salesOrderId,
    open,
    onCreated,
}: {
    salesOrderId: string
    open: boolean
    onCreated?: (created: ExpensePurchaseCreated) => void
}) {
    const profile = useAccountProfileQuery()
    const allowed =
        hasPermission(profile.data?.permissions, "purchase_order:create") &&
        hasPermission(
            profile.data?.permissions,
            "procurement_inquiry:convert_expenses",
        )
    const userId = profile.data?.userid ?? ""
    const query = useInquiryExpenseCandidatesQuery(
        salesOrderId,
        open && allowed,
    )
    const mutation = useCreateExpensePurchaseMutation(salesOrderId)
    const [selected, setSelected] =
        React.useState<InquiryExpenseCandidate | null>(null)
    const [pending, setPending] =
        React.useState<CreateExpensePurchaseInput | null>(null)
    const [failure, setFailure] = React.useState<string | null>(null)
    const commandKey = `erp.inquiry-expense-purchase.v1:${encodeURIComponent(userId)}:${encodeURIComponent(salesOrderId)}:command`

    React.useEffect(() => {
        if (!open) return
        setSelected(null)
        setFailure(null)
    }, [open, salesOrderId])

    React.useEffect(() => {
        setPending(null)
        if (!open || !allowed || !userId) return
        try {
            const value = localStorage.getItem(commandKey)
            setPending(
                value
                    ? (JSON.parse(value) as CreateExpensePurchaseInput)
                    : null,
            )
        } catch {
            setFailure("无法恢复原费用操作，请检查本机存储后重试。")
        }
    }, [allowed, commandKey, open, userId])

    async function submit(input: CreateExpensePurchaseInput) {
        if (!allowed || !userId) {
            throw new Error("当前账号不能转换询价费用，请重新核对权限。")
        }
        setFailure(null)
        // 原载荷和操作身份保存成功后才允许发出创建命令。
        localStorage.setItem(commandKey, JSON.stringify(input))
        setPending(input)
        let created
        try {
            created = await mutation.mutateAsync(input)
        } catch (error) {
            if (classifyFormalCommandError(error) === "failed") {
                localStorage.removeItem(commandKey)
                setPending(null)
            }
            setFailure(
                getErrorMessage(error, "费用采购创建失败，请保留输入后重试。"),
            )
            throw error
        }
        try {
            localStorage.removeItem(commandKey)
        } catch {
            /* 服务端已确认；旧本机记录后续仍只能重放原结果。 */
        }
        setPending(null)
        setSelected(null)
        onCreated?.({
            purchaseOrderId: created.purchase_order_id,
            purchaseNo: created.purchase_no,
        })
    }

    const candidates = query.data?.items ?? []
    const availableCount = candidates.filter(
        (candidate) => !candidateReason(candidate),
    ).length

    return {
        allowed,
        userId,
        query,
        candidates,
        availableCount,
        selected,
        setSelected,
        pending,
        failure,
        clearFailure: () => setFailure(null),
        isSubmitting: mutation.isPending,
        submit,
    }
}

export type InquiryExpenseSourceState = ReturnType<
    typeof useInquiryExpenseSourceState
>

/** 在费用采购弹窗内承载询价来源费用：候选选择 → 填写信息 → 核对确认（无嵌套弹窗）。 */
export function InquiryExpenseSourceSection({
    state,
    salesOrderId,
    sourceLabel,
    onCancel,
    onSwitchManual,
}: {
    state: InquiryExpenseSourceState
    salesOrderId: string
    sourceLabel?: string
    onCancel: () => void
    onSwitchManual: () => void
}) {
    if (!state.allowed) return null

    const recoveryAlert = state.pending ? (
        <Alert variant="warning">
            <AlertTitle>上次费用采购结果待确认</AlertTitle>
            <AlertDescription>
                已保留原费用、金额和操作内容，请先核对原结果。
            </AlertDescription>
            <div className="mt-2">
                <Button
                    id="expense-inquiry-source-recover"
                    type="button"
                    size="sm"
                    variant="outline"
                    disabled={state.isSubmitting}
                    onClick={() => {
                        if (!state.pending) return
                        void state.submit(state.pending).catch(() => undefined)
                    }}
                >
                    使用原内容核对结果
                </Button>
            </div>
        </Alert>
    ) : null

    const failureAlert = state.failure ? (
        <Alert variant="destructive">
            <AlertDescription>{state.failure}</AlertDescription>
        </Alert>
    ) : null

    if (state.selected && state.userId) {
        return (
            <InquiryExpenseSourceEditor
                key={`${state.selected.inquiry_id}:${state.selected.allocation_id}`}
                candidate={state.selected}
                salesOrderId={salesOrderId}
                sourceLabel={sourceLabel}
                userId={state.userId}
                pending={state.isSubmitting}
                unresolved={Boolean(state.pending)}
                recoveryAlert={recoveryAlert}
                failureAlert={failureAlert}
                onBack={() => {
                    state.clearFailure()
                    state.setSelected(null)
                }}
                onCancel={onCancel}
                onConfirm={state.submit}
            />
        )
    }

    return (
        <div
            className="flex min-h-0 flex-1 flex-col"
            aria-label="已接受询价费用来源"
        >
            <div className="min-h-0 flex-1 space-y-4 overflow-y-auto px-6 py-5">
                {recoveryAlert}
                {failureAlert}
                {state.query.isError ? (
                    <BusinessFailureState
                        id="expense-inquiry-source-retry"
                        title="询价费用来源加载失败"
                        error={state.query.error}
                        onRetry={() => void state.query.refetch()}
                    />
                ) : state.query.isPending ? (
                    <div
                        className="space-y-2.5"
                        role="status"
                        aria-label="正在核对本销售单的询价费用"
                    >
                        <div className="h-20 animate-pulse rounded-lg bg-muted/60" />
                        <div className="h-20 animate-pulse rounded-lg bg-muted/45" />
                    </div>
                ) : state.candidates.length > 0 ? (
                    <>
                        <p className="text-xs text-muted-foreground">
                            选择本单已确认分摊的询价费用创建费用采购单；税率及已确认条件将自动带入。
                        </p>
                        <ul className="space-y-2.5">
                            {state.candidates.map((candidate) => {
                                const reason = candidateReason(candidate)
                                const disabled =
                                    Boolean(reason) ||
                                    Boolean(state.pending) ||
                                    state.isSubmitting ||
                                    state.query.isFetching
                                return (
                                    <li
                                        key={`${candidate.inquiry_id}:${candidate.allocation_id}`}
                                        className={cn(
                                            "flex flex-col gap-3 rounded-lg border px-4 py-3 transition-colors sm:flex-row sm:items-center sm:justify-between",
                                            reason
                                                ? "border-border/60 bg-muted/25"
                                                : "border-border/80 bg-card hover:border-primary/45 hover:bg-muted/20",
                                        )}
                                    >
                                        <div className="min-w-0 flex-1 space-y-1">
                                            <div className="flex flex-wrap items-center gap-2">
                                                <span className="font-medium text-foreground">
                                                    {candidate.name}
                                                </span>
                                                <StatusBadge
                                                    tone={
                                                        reason
                                                            ? "neutral"
                                                            : "info"
                                                    }
                                                    label={
                                                        reason
                                                            ? "暂不可办"
                                                            : "可转采购"
                                                    }
                                                />
                                            </div>
                                            <p className="num text-xs text-muted-foreground">
                                                {candidate.inquiry_no} · 第{" "}
                                                {candidate.feedback_round} 轮 ·
                                                税率{" "}
                                                {formatRatePercent(
                                                    candidate.tax_rate,
                                                )}
                                                {candidate.payment_term_code
                                                    ? ` · ${paymentTermLabel(candidate.payment_term_code)}`
                                                    : ""}
                                            </p>
                                            {reason ? (
                                                <p className="text-xs text-warning-soft-foreground">
                                                    {reason}
                                                </p>
                                            ) : candidate.public_note ? (
                                                <p className="text-xs text-muted-foreground">
                                                    {candidate.public_note}
                                                </p>
                                            ) : null}
                                        </div>
                                        <div className="flex shrink-0 items-center justify-between gap-4 sm:justify-end">
                                            <div className="text-left sm:text-right">
                                                <p className="text-xs text-muted-foreground">
                                                    可转金额
                                                </p>
                                                <MoneyValue
                                                    value={
                                                        candidate.available_amount
                                                    }
                                                    display="exact"
                                                />
                                            </div>
                                            <Button
                                                id={`expense-inquiry-source-select-${toAutomationIdSegment(`${candidate.inquiry_id}-${candidate.allocation_id}`)}`}
                                                type="button"
                                                variant={
                                                    reason ? "ghost" : "outline"
                                                }
                                                size="sm"
                                                disabled={disabled}
                                                onClick={() => {
                                                    state.clearFailure()
                                                    state.setSelected(candidate)
                                                }}
                                            >
                                                选择并办理
                                                <ChevronRightIcon
                                                    aria-hidden="true"
                                                    data-icon="inline-end"
                                                />
                                            </Button>
                                        </div>
                                    </li>
                                )
                            })}
                        </ul>
                    </>
                ) : (
                    <div className="rounded-lg bg-muted/35 px-4 py-6 text-center">
                        <p className="text-body-compact text-foreground">
                            本销售单暂无可转换的询价费用
                        </p>
                        <p className="mt-1 text-xs text-muted-foreground">
                            有询价费用时由销售先确认归属；其他临时费用可直接录入。
                        </p>
                        <Button
                            id="expense-inquiry-source-switch-manual"
                            type="button"
                            variant="outline"
                            size="sm"
                            className="mt-3"
                            onClick={onSwitchManual}
                        >
                            转为直接录入
                        </Button>
                    </div>
                )}
            </div>
            <DialogFooter className="shrink-0 border-t border-grid bg-card px-6 py-3.5 sm:items-center sm:justify-between">
                <p className="text-xs text-muted-foreground">
                    其他临时费用可切换至「直接录入」；收款方不同的费用请分别建单。
                </p>
                <Button
                    id="expense-inquiry-source-close"
                    type="button"
                    variant="outline"
                    size="sm"
                    disabled={state.isSubmitting}
                    onClick={onCancel}
                >
                    取消
                </Button>
            </DialogFooter>
        </div>
    )
}

const inputSchema = z.object({
    category: z
        .enum(
            Object.keys(PURCHASE_EXPENSE_CATEGORY_LABEL) as [
                PurchaseExpenseCategory,
                ...PurchaseExpenseCategory[],
            ],
        )
        .default("other"),
    amount: z.string(),
    supplier_id: z.string(),
    supplier_name: z.string(),
    payment_term_code: z.string(),
})

type SourceFormValues = z.infer<typeof inputSchema>

function InquiryExpenseSourceEditor({
    candidate,
    salesOrderId,
    sourceLabel,
    userId,
    pending,
    unresolved,
    recoveryAlert,
    failureAlert,
    onBack,
    onCancel,
    onConfirm,
}: {
    candidate: InquiryExpenseCandidate
    salesOrderId: string
    sourceLabel?: string
    userId: string
    pending: boolean
    unresolved: boolean
    recoveryAlert: React.ReactNode
    failureAlert: React.ReactNode
    onBack: () => void
    onCancel: () => void
    onConfirm: (input: CreateExpensePurchaseInput) => Promise<void>
}) {
    const draftKey = `erp.inquiry-expense-purchase.v1:${encodeURIComponent(userId)}:${encodeURIComponent(salesOrderId)}:${encodeURIComponent(candidate.allocation_id)}:draft`
    const [initial, setInitial] = React.useState<SourceFormValues | null>(null)

    React.useEffect(() => {
        const fallback: SourceFormValues = {
            category: "other",
            amount: candidate.available_amount,
            supplier_id: candidate.payee_supplier_id ?? "",
            supplier_name: "",
            payment_term_code: candidate.payment_term_code ?? "",
        }
        try {
            const stored = localStorage.getItem(draftKey)
            const parsed = stored
                ? inputSchema.safeParse(JSON.parse(stored))
                : null
            setInitial(parsed?.success ? parsed.data : fallback)
        } catch {
            setInitial(fallback)
        }
        // 切换来源才恢复输入，背景刷新不重置已编辑字段。
        // eslint-disable-next-line react-hooks/exhaustive-deps
    }, [draftKey])

    if (!initial) {
        return (
            <div className="px-6 py-5">
                <div
                    className="h-44 animate-pulse rounded-lg bg-muted/50"
                    role="status"
                    aria-label="加载询价费用采购表单"
                />
            </div>
        )
    }

    return (
        <InquiryExpenseSourceForm
            candidate={candidate}
            sourceLabel={sourceLabel}
            initial={initial}
            draftKey={draftKey}
            pending={pending}
            unresolved={unresolved}
            recoveryAlert={recoveryAlert}
            failureAlert={failureAlert}
            onBack={onBack}
            onCancel={onCancel}
            onConfirm={onConfirm}
        />
    )
}

function InquiryExpenseSourceForm({
    candidate,
    sourceLabel,
    initial,
    draftKey,
    pending,
    unresolved,
    recoveryAlert,
    failureAlert,
    onBack,
    onCancel,
    onConfirm,
}: {
    candidate: InquiryExpenseCandidate
    sourceLabel?: string
    initial: SourceFormValues
    draftKey: string
    pending: boolean
    unresolved: boolean
    recoveryAlert: React.ReactNode
    failureAlert: React.ReactNode
    onBack: () => void
    onCancel: () => void
    onConfirm: (input: CreateExpensePurchaseInput) => Promise<void>
}) {
    const [preview, setPreview] = React.useState<{
        input: CreateExpensePurchaseInput
        supplier_name: string
    } | null>(null)
    const [storageFailure, setStorageFailure] = React.useState(false)
    const taxPercent = multiplyFixed(candidate.tax_rate, "100", {
        leftMaxScale: 6,
        rightMaxScale: 0,
        outputScale: 2,
    })
    const lockedTaxDisplay = compactFixed(taxPercent)

    const form = useAppForm({
        defaultValues: initial,
        listeners: {
            onChange: ({ formApi }) => {
                try {
                    localStorage.setItem(
                        draftKey,
                        JSON.stringify(formApi.state.values),
                    )
                    setStorageFailure(false)
                } catch {
                    setStorageFailure(true)
                }
            },
        },
        validators: {
            onSubmit: ({ value }) => {
                try {
                    if (
                        !amountPattern.test(value.amount.trim()) ||
                        compareDecimal(value.amount.trim(), "0", 2) <= 0
                    ) {
                        return {
                            fields: {
                                amount: "请填写大于 0 且最多两位小数的金额",
                            },
                        }
                    }
                    if (
                        compareDecimal(
                            value.amount.trim(),
                            candidate.available_amount,
                            2,
                        ) > 0
                    ) {
                        return {
                            fields: {
                                amount: "金额超过该笔询价费用的可转余额",
                            },
                        }
                    }
                } catch {
                    return { fields: { amount: "请填写有效的金额" } }
                }
                if (!value.supplier_id) {
                    return {
                        fields: { supplier_id: "请选择本轮费用的收款供应商" },
                    }
                }
                if (
                    candidate.payee_supplier_id &&
                    value.supplier_id !== candidate.payee_supplier_id
                ) {
                    return {
                        fields: {
                            supplier_id: "收款供应商须与本轮已确认费用依据一致",
                        },
                    }
                }
                if (!value.payment_term_code) {
                    return { fields: { payment_term_code: "请选择付款条件" } }
                }
                if (
                    candidate.payment_term_code &&
                    value.payment_term_code !== candidate.payment_term_code
                ) {
                    return {
                        fields: {
                            payment_term_code:
                                "付款条件须与本轮已确认费用依据一致",
                        },
                    }
                }
                return undefined
            },
        },
        onSubmit: ({ value }) => {
            if (
                storageFailure ||
                pending ||
                unresolved ||
                candidateReason(candidate)
            ) {
                return
            }
            setPreview({
                supplier_name: value.supplier_name || "本轮已确认收款供应商",
                input: {
                    supplierId: value.supplier_id,
                    paymentTermCode: value.payment_term_code,
                    idempotencyKey: `inquiry-expense:${crypto.randomUUID()}`,
                    lines: [
                        {
                            category: value.category,
                            description: candidate.name,
                            grossAmount: value.amount.trim(),
                            inputTaxRate: candidate.tax_rate,
                            inquirySource: {
                                inquiry_id: candidate.inquiry_id,
                                feedback_round: candidate.feedback_round,
                                fee_id: candidate.fee_id,
                                allocation_id: candidate.allocation_id,
                            },
                        },
                    ],
                },
            })
        },
    })

    if (preview) {
        const line = preview.input.lines[0]
        const breakdown = (() => {
            try {
                return splitFeeGrossByPercentRate(line.grossAmount, taxPercent)
            } catch {
                return null
            }
        })()

        return (
            <div className="flex min-h-0 flex-1 flex-col">
                <div className="min-h-0 flex-1 space-y-4 overflow-y-auto px-6 py-5">
                    {recoveryAlert}
                    {failureAlert}
                    <Alert variant="info">
                        <AlertTitle>核对询价费用采购</AlertTitle>
                        <AlertDescription>
                            确认无误后将生成费用采购单并直接提交审批，同时锁定该笔询价费用的对应额度。
                        </AlertDescription>
                    </Alert>

                    <dl className="grid grid-cols-1 gap-x-6 gap-y-3 rounded-lg border border-border/70 bg-muted/25 p-4 sm:grid-cols-2">
                        <div className="space-y-0.5">
                            <dt className="text-xs text-muted-foreground">
                                询价来源
                            </dt>
                            <dd className="num text-body-compact text-foreground">
                                {candidate.inquiry_no} · 第{" "}
                                {candidate.feedback_round} 轮
                            </dd>
                        </div>
                        <div className="space-y-0.5">
                            <dt className="text-xs text-muted-foreground">
                                归属销售单
                            </dt>
                            <dd className="num text-body-compact text-foreground">
                                {sourceLabel || "当前销售单"}
                            </dd>
                        </div>
                        <div className="space-y-0.5">
                            <dt className="text-xs text-muted-foreground">
                                费用类别
                            </dt>
                            <dd className="text-body-compact text-foreground">
                                {PURCHASE_EXPENSE_CATEGORY_LABEL[line.category]}
                            </dd>
                        </div>
                        <div className="space-y-0.5">
                            <dt className="text-xs text-muted-foreground">
                                费用说明
                            </dt>
                            <dd className="break-words text-body-compact text-foreground">
                                {candidate.name}
                            </dd>
                        </div>
                        <div className="space-y-0.5">
                            <dt className="text-xs text-muted-foreground">
                                含税金额
                            </dt>
                            <dd>
                                <MoneyValue
                                    value={line.grossAmount}
                                    display="exact"
                                />
                            </dd>
                        </div>
                        <div className="space-y-0.5">
                            <dt className="text-xs text-muted-foreground">
                                进项税率 / 不含税金额
                            </dt>
                            <dd className="num text-body-compact text-foreground">
                                {lockedTaxDisplay}%
                                {breakdown ? (
                                    <>
                                        <span className="mx-1.5 text-muted-foreground">
                                            ·
                                        </span>
                                        <MoneyValue
                                            value={breakdown.net}
                                            display="exact"
                                        />
                                    </>
                                ) : null}
                            </dd>
                        </div>
                        <div className="space-y-0.5">
                            <dt className="text-xs text-muted-foreground">
                                收款供应商
                            </dt>
                            <dd className="text-body-compact text-foreground">
                                {preview.supplier_name}
                            </dd>
                        </div>
                        <div className="space-y-0.5">
                            <dt className="text-xs text-muted-foreground">
                                付款条件
                            </dt>
                            <dd className="text-body-compact text-foreground">
                                {paymentTermLabel(
                                    preview.input.paymentTermCode,
                                )}
                            </dd>
                        </div>
                    </dl>

                    <div className="space-y-1 rounded-lg bg-muted/35 px-4 py-2.5 text-xs text-muted-foreground">
                        <p>
                            ·
                            核对关联销售单与询价费用余额，生成费用采购单并提交审批。
                        </p>
                        <p>· 提交后将锁定该笔询价费用的可转金额。</p>
                    </div>
                </div>

                <DialogFooter className="shrink-0 border-t border-grid bg-card px-6 py-3.5 sm:items-center sm:justify-between">
                    <Button
                        id="expense-inquiry-create-confirm-cancel"
                        type="button"
                        variant="ghost"
                        size="sm"
                        className={detailWorkspaceStyles.quietButton}
                        disabled={pending}
                        onClick={() => setPreview(null)}
                    >
                        <ArrowLeftIcon
                            aria-hidden="true"
                            data-icon="inline-start"
                        />
                        返回修改
                    </Button>
                    <div className="flex items-center justify-end gap-2">
                        <Button
                            id="expense-inquiry-create-confirm-close"
                            type="button"
                            variant="outline"
                            size="sm"
                            disabled={pending}
                            onClick={onCancel}
                        >
                            取消
                        </Button>
                        <LoadingButton
                            id="expense-inquiry-create-confirm-confirm"
                            type="button"
                            size="sm"
                            loading={pending}
                            disabled={pending || unresolved}
                            onClick={() => {
                                void onConfirm(preview.input)
                                    .then(() => {
                                        try {
                                            localStorage.removeItem(draftKey)
                                        } catch {
                                            /* 草稿清理失败不影响已创建结果。 */
                                        }
                                        setPreview(null)
                                    })
                                    .catch(() => undefined)
                            }}
                        >
                            创建并提交审批
                        </LoadingButton>
                    </div>
                </DialogFooter>
            </div>
        )
    }

    return (
        <form
            className="flex min-h-0 flex-1 flex-col"
            onSubmit={(event) => {
                event.preventDefault()
                void form.handleSubmit()
            }}
        >
            <div className="min-h-0 flex-1 space-y-4 overflow-y-auto px-6 py-5">
                {recoveryAlert}
                {failureAlert}

                <div className="flex flex-wrap items-start justify-between gap-3 rounded-lg border border-info-border/75 bg-info-soft/45 px-4 py-3">
                    <div className="min-w-0 flex-1 space-y-1">
                        <div className="flex flex-wrap items-center gap-2">
                            <Badge
                                variant="secondary"
                                className="bg-card font-normal text-info-soft-foreground"
                            >
                                询价来源
                            </Badge>
                            <span className="font-medium text-foreground">
                                {candidate.name}
                            </span>
                            <span className="num text-xs text-muted-foreground">
                                {candidate.inquiry_no} · 第{" "}
                                {candidate.feedback_round} 轮
                            </span>
                        </div>
                        <p className="text-xs text-muted-foreground">
                            可转金额：
                            <MoneyValue
                                value={candidate.available_amount}
                                display="exact"
                            />
                            <span className="mx-1.5">·</span>
                            确认税率：
                            <RateValue
                                value={taxPercent}
                                precision={2}
                                className="font-medium text-foreground"
                            />
                            {candidate.public_note ? (
                                <>
                                    <span className="mx-1.5">·</span>
                                    <span>{candidate.public_note}</span>
                                </>
                            ) : null}
                        </p>
                    </div>
                    <Button
                        id="expense-inquiry-create-change-source"
                        type="button"
                        variant="ghost"
                        size="sm"
                        className={detailWorkspaceStyles.inlineLinkButton}
                        disabled={pending}
                        onClick={onBack}
                    >
                        更换来源
                    </Button>
                </div>

                <div className="grid grid-cols-1 items-start gap-x-4 gap-y-3.5 sm:grid-cols-12">
                    <form.AppField
                        name="category"
                        children={(field) => (
                            <field.SelectField
                                id="inquiry-expense-source-category"
                                label="费用类别"
                                required
                                allowClear={false}
                                disabled={pending || unresolved}
                                options={CATEGORY_OPTIONS}
                                className="sm:col-span-4"
                            />
                        )}
                    />
                    <form.AppField name="amount">
                        {(field) => (
                            <field.TextField
                                id="expense-inquiry-create-amount"
                                money
                                label="本次转换含税金额（元）"
                                required
                                inputMode="decimal"
                                inputClassName="num"
                                disabled={pending || unresolved}
                                className="sm:col-span-5"
                            />
                        )}
                    </form.AppField>
                    <Field className="sm:col-span-3">
                        <FieldLabel>进项税率</FieldLabel>
                        <div
                            title="沿用询价确认税率，不可修改"
                            aria-label={`进项税率 ${lockedTaxDisplay}%，沿用询价确认税率`}
                            className="flex h-control items-center justify-between rounded-md border border-border/80 bg-muted/45 px-3 text-body-compact"
                        >
                            <span className="num font-medium text-foreground">
                                {lockedTaxDisplay}%
                            </span>
                            <LockIcon
                                aria-hidden="true"
                                className="size-3.5 shrink-0 text-muted-foreground"
                            />
                        </div>
                    </Field>
                </div>

                <form.Subscribe selector={(state) => state.values.amount}>
                    {(amount) => (
                        <InquiryExpenseAmountPreview
                            amount={amount}
                            taxPercent={taxPercent}
                            quotaLimit={candidate.available_amount}
                        />
                    )}
                </form.Subscribe>

                <div className="grid grid-cols-1 items-start gap-4 border-t border-border/60 pt-4 sm:grid-cols-2">
                    <form.AppField name="supplier_id">
                        {(field) => (
                            <Field
                                data-invalid={
                                    field.state.meta.errors.length > 0 ||
                                    undefined
                                }
                            >
                                <FieldLabel htmlFor="expense-inquiry-create-supplier">
                                    收款供应商
                                    <RequiredMark />
                                </FieldLabel>
                                <SupplierSearchCombobox
                                    id="expense-inquiry-create-supplier"
                                    value={field.state.value || undefined}
                                    disabled={pending || unresolved}
                                    onValueChange={(id) => {
                                        field.handleChange(id ?? "")
                                        field.handleBlur()
                                        form.setFieldValue("supplier_name", "")
                                    }}
                                    onItemChange={(item) =>
                                        form.setFieldValue(
                                            "supplier_name",
                                            item?.supplierName ?? "",
                                        )
                                    }
                                    placeholder="选择已建档的收款方"
                                />
                                {candidate.payee_supplier_id ? (
                                    <FieldDescription>
                                        本轮询价已确认收款供应商，须与依据一致。
                                    </FieldDescription>
                                ) : null}
                                <FieldError
                                    errors={toFieldErrors(
                                        field.state.meta.errors,
                                    )}
                                />
                            </Field>
                        )}
                    </form.AppField>

                    <form.AppField name="payment_term_code">
                        {(field) => (
                            <field.SelectField
                                id="expense-inquiry-create-payment"
                                label="付款条件"
                                required
                                allowClear={false}
                                options={SUPPLIER_PAYMENT_TERM_OPTIONS}
                                disabled={pending || unresolved}
                                placeholder="选择付款条件"
                                description={
                                    candidate.payment_term_code
                                        ? `本轮已确认：${paymentTermLabel(candidate.payment_term_code)}`
                                        : undefined
                                }
                            />
                        )}
                    </form.AppField>
                </div>

                {storageFailure ? (
                    <Alert variant="destructive">
                        <AlertDescription>
                            输入无法保存到本机，请检查浏览器存储后重试，当前内容仍保留。
                        </AlertDescription>
                    </Alert>
                ) : null}
            </div>

            <DialogFooter className="shrink-0 border-t border-grid bg-card px-6 py-3.5 sm:items-center sm:justify-between">
                <Button
                    id="expense-inquiry-create-cancel"
                    type="button"
                    variant="ghost"
                    size="sm"
                    className={detailWorkspaceStyles.quietButton}
                    disabled={pending}
                    onClick={onBack}
                >
                    <ArrowLeftIcon
                        aria-hidden="true"
                        data-icon="inline-start"
                    />
                    返回列表
                </Button>
                <div className="flex items-center justify-end gap-2">
                    <Button
                        id="expense-inquiry-create-dialog-cancel"
                        type="button"
                        variant="outline"
                        size="sm"
                        disabled={pending}
                        onClick={onCancel}
                    >
                        取消
                    </Button>
                    <form.AppForm>
                        <form.SubmitButton
                            id="expense-inquiry-create-preview"
                            size="sm"
                            showError={false}
                            label="下一步：核对采购"
                            disabled={pending || unresolved || storageFailure}
                        />
                    </form.AppForm>
                </div>
            </DialogFooter>
        </form>
    )
}

function InquiryExpenseAmountPreview({
    amount,
    taxPercent,
    quotaLimit,
}: {
    amount: string
    taxPercent: string
    quotaLimit: string
}) {
    const trimmed = amount.trim()
    const rateLabel = `${compactFixed(taxPercent)}%`
    let breakdown: { gross: string; net: string; tax: string } | null = null
    if (amountPattern.test(trimmed)) {
        try {
            if (compareDecimal(trimmed, "0", 2) > 0) {
                breakdown = splitFeeGrossByPercentRate(trimmed, taxPercent)
            }
        } catch {
            breakdown = null
        }
    }

    let quotaStatus:
        | { kind: "exceeded" }
        | { kind: "full" }
        | { kind: "remaining"; value: string }
        | null = null
    if (breakdown) {
        try {
            const cmp = compareDecimal(breakdown.gross, quotaLimit, 2)
            if (cmp > 0) {
                quotaStatus = { kind: "exceeded" }
            } else if (cmp === 0) {
                quotaStatus = { kind: "full" }
            } else {
                quotaStatus = {
                    kind: "remaining",
                    value: subtractFixed(quotaLimit, breakdown.gross, {
                        maxScale: 2,
                        outputScale: 2,
                    }),
                }
            }
        } catch {
            quotaStatus = null
        }
    }

    return (
        <div
            aria-label="询价费用采购试算"
            className="rounded-lg bg-muted/40 px-4 py-2.5"
        >
            <dl className="grid grid-cols-3 gap-3">
                <div className="min-w-0 space-y-0.5">
                    <dt className="text-xs text-muted-foreground">含税金额</dt>
                    <dd className="num font-medium text-foreground">
                        {breakdown ? (
                            <MoneyValue
                                value={breakdown.gross}
                                display="exact"
                            />
                        ) : (
                            <span className="font-normal text-muted-foreground">
                                —
                            </span>
                        )}
                    </dd>
                </div>
                <div className="min-w-0 space-y-0.5">
                    <dt className="text-xs text-muted-foreground">
                        进项税额
                        <span className="num ml-1">({rateLabel})</span>
                    </dt>
                    <dd className="num text-foreground/85">
                        {breakdown ? (
                            <MoneyValue value={breakdown.tax} display="exact" />
                        ) : (
                            <span className="text-muted-foreground">—</span>
                        )}
                    </dd>
                </div>
                <div className="min-w-0 space-y-0.5">
                    <dt className="text-xs text-muted-foreground">
                        不含税成本
                    </dt>
                    <dd className="num font-semibold text-foreground">
                        {breakdown ? (
                            <MoneyValue value={breakdown.net} display="exact" />
                        ) : (
                            <span className="font-normal text-muted-foreground">
                                —
                            </span>
                        )}
                    </dd>
                </div>
            </dl>
            {quotaStatus ? (
                <div className="mt-2 border-t border-border/60 pt-2 text-xs">
                    {quotaStatus.kind === "exceeded" ? (
                        <p className="text-destructive">
                            转换金额超出可转余额上限{" "}
                            <MoneyValue value={quotaLimit} display="exact" />
                        </p>
                    ) : quotaStatus.kind === "full" ? (
                        <p className="text-muted-foreground">
                            本次将全额转换该笔询价费用额度。
                        </p>
                    ) : (
                        <p className="text-muted-foreground">
                            本次转换后剩余可转余额：{" "}
                            <MoneyValue
                                value={quotaStatus.value}
                                display="exact"
                            />
                        </p>
                    )}
                </div>
            ) : null}
        </div>
    )
}
