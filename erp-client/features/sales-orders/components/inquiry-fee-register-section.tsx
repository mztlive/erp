"use client"

import * as React from "react"
import { ArrowLeftIcon, ChevronRightIcon, LockIcon } from "lucide-react"

import {
    BusinessFailureState,
    MoneyValue,
    RateValue,
    detailWorkspaceStyles,
} from "@/components/business"
import { useAppForm } from "@/components/form"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { DialogFooter } from "@/components/ui/dialog"
import { Field, FieldLabel } from "@/components/ui/field"
import { LoadingButton } from "@/components/ui/loading-button"
import { StatusBadge } from "@/components/ui/status-badge"
import { useAccountProfileQuery } from "@/features/auth/queries"
import { SupplierSearchCombobox } from "@/features/entity-selectors/components/supplier-search-combobox"
import { getErrorMessage } from "@/lib/api/errors"
import { toAutomationIdSegment } from "@/lib/automation-id"
import {
    compactFixed,
    compareDecimal,
    multiplyFixed,
} from "@/lib/fixed-decimal"
import { classifyFormalCommandError } from "@/lib/formal-command"
import {
    readInquiryFileValue,
    removeInquiryFileValue,
    writeInquiryFileValue,
} from "@/lib/inquiry-file-store"
import { hasPermission } from "@/lib/permissions"
import { cn } from "@/lib/utils"
import type {
    ExtraCostCategory,
    RegisterSalesExtraCostInput,
    SalesInquiryFeeCandidate,
} from "../api/sales-order-extra-costs"
import {
    useRegisterSalesExtraCostMutation,
    useSalesInquiryFeeCandidatesQuery,
} from "../hooks/queries"
import {
    EXTRA_COST_CATEGORY_LABEL,
    EXTRA_COST_WHOLE_ORDER,
    REGISTER_CATEGORY_OPTIONS,
    calculateExtraCostAmounts,
    formatTaxRate,
    registerDefaults,
    registerExtraCostSchema,
    todayDate,
    type RegisterExtraCostValues,
} from "../lib/extra-cost"
import { ExtraCostAmountBreakdown } from "./extra-cost-amount-breakdown"

type StoredRegistration = {
    input: RegisterSalesExtraCostInput
    draft_key: string
}

export type LineAttributionOption = Readonly<{
    value: string
    label: string
}>

function candidateReason(candidate: SalesInquiryFeeCandidate): string | null {
    return candidate.unavailable_reason ?? null
}

function sourceKey(userId: string, salesOrderId: string): string {
    return `inquiry-finance:${encodeURIComponent(userId)}:${encodeURIComponent(salesOrderId)}`
}

/** 管理财务询价费用候选、待确认命令恢复与登记提交，供弹窗与分区共享单一状态源。 */
export function useInquiryFeeRegisterState({
    salesOrderId,
    open,
    onRegistered,
}: {
    salesOrderId: string
    open: boolean
    onRegistered: (input: RegisterSalesExtraCostInput) => void
}) {
    const profile = useAccountProfileQuery()
    const allowed = hasPermission(
        profile.data?.permissions,
        "cost_entry:create",
    )
    const userId = profile.data?.userid ?? ""
    const query = useSalesInquiryFeeCandidatesQuery(
        salesOrderId,
        open && allowed,
    )
    const mutation = useRegisterSalesExtraCostMutation(salesOrderId)
    const [selected, setSelected] =
        React.useState<SalesInquiryFeeCandidate | null>(null)
    const [pending, setPending] = React.useState<StoredRegistration | null>(
        null,
    )
    const [ready, setReady] = React.useState(false)
    const [failure, setFailure] = React.useState<string | null>(null)
    const commandKey = `${sourceKey(userId, salesOrderId)}:command`

    React.useEffect(() => {
        if (!open) return
        setSelected(null)
        setFailure(null)
    }, [open, salesOrderId])

    React.useEffect(() => {
        let active = true
        setReady(false)
        setPending(null)
        if (!open || !allowed || !userId) return
        void readInquiryFileValue<StoredRegistration>(commandKey)
            .then((stored) => {
                if (!active) return
                setPending(stored ?? null)
                setReady(true)
            })
            .catch(() => {
                if (active) {
                    setFailure(
                        "无法读取上次填写的费用及凭证，请清理浏览器缓存后重试。",
                    )
                }
            })
        return () => {
            active = false
        }
    }, [allowed, commandKey, open, userId])

    async function submit(command: StoredRegistration) {
        if (!allowed || !ready || !userId) {
            throw new Error("当前费用登记资格尚未核对，请稍候重试。")
        }
        setFailure(null)
        // File 以结构化数据持久保存，原请求未保存成功时禁止发送。
        await writeInquiryFileValue(commandKey, command)
        setPending(command)
        try {
            await mutation.mutateAsync(command.input)
        } catch (error) {
            if (classifyFormalCommandError(error) === "failed") {
                await removeInquiryFileValue(commandKey)
                setPending(null)
            }
            setFailure(
                getErrorMessage(error, "费用登记失败，请保留原凭证后重试。"),
            )
            throw error
        }
        await Promise.allSettled([
            removeInquiryFileValue(commandKey),
            removeInquiryFileValue(command.draft_key),
        ])
        setPending(null)
        setSelected(null)
        onRegistered(command.input)
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
        ready,
        failure,
        clearFailure: () => setFailure(null),
        isSubmitting: mutation.isPending,
        submit,
        storagePrefix: sourceKey(userId, salesOrderId),
    }
}

export type InquiryFeeRegisterState = ReturnType<
    typeof useInquiryFeeRegisterState
>

/** 在「登记附加费用」弹窗内承载询价来源费用：候选选择 → 信息填写 → 核对确认（无嵌套弹窗）。 */
export function InquiryFeeRegisterSection({
    state,
    salesOrderNo,
    lineOptions,
    onCancel,
    onSwitchManual,
}: {
    state: InquiryFeeRegisterState
    salesOrderNo: string
    lineOptions: readonly LineAttributionOption[]
    onCancel: () => void
    onSwitchManual: () => void
}) {
    if (!state.allowed) return null

    const recoveryAlert = state.pending ? (
        <Alert variant="warning">
            <AlertTitle>上次费用登记结果待确认</AlertTitle>
            <AlertDescription>
                已保留原金额、费用来源和凭证，请先核对原结果。
            </AlertDescription>
            <div className="mt-2">
                <Button
                    id="finance-inquiry-fee-recover"
                    type="button"
                    size="sm"
                    variant="outline"
                    disabled={state.isSubmitting || !state.ready}
                    onClick={() => {
                        if (!state.pending) return
                        void state.submit(state.pending).catch(() => undefined)
                    }}
                >
                    使用原请求及凭证核对
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
            <InquiryFeeRegistration
                key={`${state.selected.inquiry_id}:${state.selected.allocation_id}`}
                candidate={state.selected}
                salesOrderNo={salesOrderNo}
                lineOptions={lineOptions}
                storagePrefix={state.storagePrefix}
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
            aria-label="财务询价费用来源"
        >
            <div className="min-h-0 flex-1 space-y-4 overflow-y-auto px-6 py-5">
                {recoveryAlert}
                {failureAlert}
                {state.query.isError ? (
                    <BusinessFailureState
                        id="finance-inquiry-fee-retry"
                        title="询价费用来源加载失败"
                        error={state.query.error}
                        onRetry={() => void state.query.refetch()}
                    />
                ) : state.query.isPending ? (
                    <div
                        className="space-y-2.5"
                        role="status"
                        aria-label="正在核对可登记费用"
                    >
                        <div className="h-20 animate-pulse rounded-lg bg-muted/60" />
                        <div className="h-20 animate-pulse rounded-lg bg-muted/45" />
                    </div>
                ) : state.candidates.length > 0 ? (
                    <>
                        <p className="text-xs text-muted-foreground">
                            选择本单已确认分摊的询价费用；登记后将自动关联原询价来源并沿用已确认税率。
                        </p>
                        <ul className="space-y-2.5">
                            {state.candidates.map((candidate) => {
                                const reason = candidateReason(candidate)
                                const disabled =
                                    Boolean(reason) ||
                                    Boolean(state.pending) ||
                                    state.isSubmitting ||
                                    !state.ready ||
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
                                                            : "可登记"
                                                    }
                                                />
                                            </div>
                                            <p className="num text-xs text-muted-foreground">
                                                {candidate.inquiry_no} · 第{" "}
                                                {candidate.feedback_round} 轮 ·
                                                税率{" "}
                                                {formatTaxRate(
                                                    candidate.tax_rate,
                                                )}
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
                                                    可登记额度
                                                </p>
                                                <MoneyValue
                                                    value={
                                                        candidate.available_amount
                                                    }
                                                    display="exact"
                                                    className="font-semibold text-foreground"
                                                />
                                            </div>
                                            <Button
                                                id={`finance-inquiry-fee-select-${toAutomationIdSegment(`${candidate.inquiry_id}-${candidate.allocation_id}`)}`}
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
                                                选择登记
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
                            本销售单暂无可登记的询价费用
                        </p>
                        <p className="mt-1 text-xs text-muted-foreground">
                            如有询价费用需先由销售确认归属；其他临时费用可直接登记。
                        </p>
                        <Button
                            id="finance-inquiry-fee-switch-manual"
                            type="button"
                            variant="outline"
                            size="sm"
                            className="mt-3"
                            onClick={onSwitchManual}
                        >
                            转为直接登记
                        </Button>
                    </div>
                )}
            </div>
            <DialogFooter className="shrink-0 border-t border-grid bg-card px-6 py-3.5 sm:items-center sm:justify-between">
                <p className="text-xs text-muted-foreground">
                    非询价产生的费用可切换至「直接登记」。
                </p>
                <Button
                    id="finance-inquiry-fee-close"
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

function InquiryFeeRegistration({
    candidate,
    salesOrderNo,
    lineOptions,
    storagePrefix,
    pending,
    unresolved,
    recoveryAlert,
    failureAlert,
    onBack,
    onCancel,
    onConfirm,
}: {
    candidate: SalesInquiryFeeCandidate
    salesOrderNo: string
    lineOptions: readonly LineAttributionOption[]
    storagePrefix: string
    pending: boolean
    unresolved: boolean
    recoveryAlert: React.ReactNode
    failureAlert: React.ReactNode
    onBack: () => void
    onCancel: () => void
    onConfirm: (command: StoredRegistration) => Promise<void>
}) {
    const draftKey = `${storagePrefix}:${encodeURIComponent(candidate.allocation_id)}:draft`
    const [initial, setInitial] =
        React.useState<RegisterExtraCostValues | null>(null)
    const [restoreFailed, setRestoreFailed] = React.useState(false)

    React.useEffect(() => {
        let active = true
        void readInquiryFileValue<RegisterExtraCostValues>(draftKey)
            .then((stored) => {
                if (!active) return
                const defaults: RegisterExtraCostValues = {
                    ...registerDefaults(),
                    category: "other" as ExtraCostCategory,
                    description: candidate.name,
                    grossAmount: candidate.available_amount,
                    taxRatePercent: multiplyFixed(candidate.tax_rate, "100", {
                        leftMaxScale: 6,
                        rightMaxScale: 0,
                        outputScale: 2,
                    }),
                }
                setInitial(
                    stored
                        ? {
                              ...defaults,
                              ...stored,
                              salesOrderLineId:
                                  stored.salesOrderLineId ||
                                  EXTRA_COST_WHOLE_ORDER,
                              taxRatePercent: defaults.taxRatePercent,
                          }
                        : defaults,
                )
            })
            .catch(() => {
                if (active) setRestoreFailed(true)
            })
        return () => {
            active = false
        }
        // 同一费用来源的背景刷新不替换已输入资料及凭证。
        // eslint-disable-next-line react-hooks/exhaustive-deps
    }, [draftKey])

    if (restoreFailed) {
        return (
            <div className="flex min-h-0 flex-1 flex-col">
                <div className="min-h-0 flex-1 space-y-4 overflow-y-auto px-6 py-5">
                    {recoveryAlert}
                    <Alert variant="destructive">
                        <AlertDescription>
                            无法恢复该笔费用的暂存输入，请清理浏览器缓存后重新选择。
                        </AlertDescription>
                    </Alert>
                </div>
                <DialogFooter className="shrink-0 border-t border-grid bg-card px-6 py-3.5 sm:items-center sm:justify-between">
                    <Button
                        id="finance-inquiry-register-cancel"
                        type="button"
                        variant="ghost"
                        size="sm"
                        className={detailWorkspaceStyles.quietButton}
                        onClick={onBack}
                    >
                        <ArrowLeftIcon
                            aria-hidden="true"
                            data-icon="inline-start"
                        />
                        返回列表
                    </Button>
                    <Button
                        type="button"
                        variant="outline"
                        size="sm"
                        onClick={onCancel}
                    >
                        取消
                    </Button>
                </DialogFooter>
            </div>
        )
    }

    if (!initial) {
        return (
            <div className="px-6 py-5">
                <div
                    className="h-44 animate-pulse rounded-lg bg-muted/50"
                    role="status"
                    aria-label="加载询价费用表单"
                />
            </div>
        )
    }

    return (
        <InquiryFeeRegistrationForm
            candidate={candidate}
            salesOrderNo={salesOrderNo}
            lineOptions={lineOptions}
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

function InquiryFeeRegistrationForm({
    candidate,
    salesOrderNo,
    lineOptions,
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
    candidate: SalesInquiryFeeCandidate
    salesOrderNo: string
    lineOptions: readonly LineAttributionOption[]
    initial: RegisterExtraCostValues
    draftKey: string
    pending: boolean
    unresolved: boolean
    recoveryAlert: React.ReactNode
    failureAlert: React.ReactNode
    onBack: () => void
    onCancel: () => void
    onConfirm: (command: StoredRegistration) => Promise<void>
}) {
    const [preview, setPreview] = React.useState<StoredRegistration | null>(
        null,
    )
    const [storageFailure, setStorageFailure] = React.useState(false)
    const saveQueue = React.useRef(Promise.resolve())
    const lockedTaxRateDisplay = compactFixed(initial.taxRatePercent)

    const form = useAppForm({
        defaultValues: initial,
        listeners: {
            onChange: ({ formApi }) => {
                const snapshot = structuredClone(formApi.state.values)
                saveQueue.current = saveQueue.current
                    .then(() => writeInquiryFileValue(draftKey, snapshot))
                    .then(() => setStorageFailure(false))
                    .catch(() => setStorageFailure(true))
            },
        },
        validators: {
            onChange: registerExtraCostSchema,
            onSubmit: ({ value }) => {
                const reason = candidateReason(candidate)
                if (reason) {
                    return { fields: { grossAmount: reason } }
                }
                try {
                    if (
                        compareDecimal(
                            value.grossAmount,
                            candidate.available_amount,
                            2,
                        ) > 0
                    ) {
                        return {
                            fields: {
                                grossAmount: "登记金额超过本笔费用的剩余额度",
                            },
                        }
                    }
                } catch {
                    return { fields: { grossAmount: "请输入有效的金额" } }
                }
                return undefined
            },
        },
        onSubmit: async ({ value }) => {
            await saveQueue.current
            if (pending || unresolved || storageFailure || !value.evidence) {
                return
            }
            setPreview({
                draft_key: draftKey,
                input: {
                    category: value.category as ExtraCostCategory,
                    description: value.description.trim(),
                    grossAmount: value.grossAmount.trim(),
                    inputTaxRate: candidate.tax_rate,
                    occurredOn: value.occurredOn,
                    supplierId: value.supplierId || undefined,
                    salesOrderLineId:
                        value.salesOrderLineId === EXTRA_COST_WHOLE_ORDER
                            ? undefined
                            : value.salesOrderLineId || undefined,
                    evidence: value.evidence,
                    idempotencyKey: `inquiry-finance:${crypto.randomUUID()}`,
                    inquirySource: {
                        inquiry_id: candidate.inquiry_id,
                        feedback_round: candidate.feedback_round,
                        fee_id: candidate.fee_id,
                        allocation_id: candidate.allocation_id,
                    },
                },
            })
        },
    })

    if (preview) {
        const amounts = calculateExtraCostAmounts(
            preview.input.grossAmount,
            initial.taxRatePercent,
        )
        const lineLabel = preview.input.salesOrderLineId
            ? (lineOptions.find(
                  (opt) => opt.value === preview.input.salesOrderLineId,
              )?.label ?? "指定明细")
            : "整张销售单"

        return (
            <div className="flex min-h-0 flex-1 flex-col">
                <div className="min-h-0 flex-1 space-y-4 overflow-y-auto px-6 py-5">
                    {recoveryAlert}
                    {failureAlert}
                    <Alert variant="info">
                        <AlertTitle>核对询价费用登记</AlertTitle>
                        <AlertDescription>
                            确认无误后将立即计入本销售单实际经营盈亏，不生成费用采购或应付。
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
                                归属对象
                            </dt>
                            <dd className="text-body-compact text-foreground">
                                <span className="num">{salesOrderNo}</span>
                                <span className="mx-1.5 text-muted-foreground">
                                    ·
                                </span>
                                <span>{lineLabel}</span>
                            </dd>
                        </div>
                        <div className="space-y-0.5">
                            <dt className="text-xs text-muted-foreground">
                                费用类别
                            </dt>
                            <dd className="text-body-compact text-foreground">
                                {
                                    EXTRA_COST_CATEGORY_LABEL[
                                        preview.input.category
                                    ]
                                }
                            </dd>
                        </div>
                        <div className="space-y-0.5">
                            <dt className="text-xs text-muted-foreground">
                                费用说明
                            </dt>
                            <dd className="break-words text-body-compact text-foreground">
                                {preview.input.description}
                            </dd>
                        </div>
                        <div className="space-y-0.5">
                            <dt className="text-xs text-muted-foreground">
                                含税金额
                            </dt>
                            <dd>
                                <MoneyValue
                                    value={preview.input.grossAmount}
                                    display="exact"
                                    className="font-semibold text-foreground"
                                />
                            </dd>
                        </div>
                        <div className="space-y-0.5">
                            <dt className="text-xs text-muted-foreground">
                                进项税率 / 不含税成本
                            </dt>
                            <dd className="num text-body-compact text-foreground">
                                {formatTaxRate(preview.input.inputTaxRate)}
                                {amounts ? (
                                    <>
                                        <span className="mx-1.5 text-muted-foreground">
                                            ·
                                        </span>
                                        <MoneyValue
                                            value={amounts.net}
                                            display="exact"
                                        />
                                    </>
                                ) : null}
                            </dd>
                        </div>
                        <div className="space-y-0.5">
                            <dt className="text-xs text-muted-foreground">
                                发生日期
                            </dt>
                            <dd className="num text-body-compact text-foreground">
                                {preview.input.occurredOn}
                            </dd>
                        </div>
                        <div className="space-y-0.5">
                            <dt className="text-xs text-muted-foreground">
                                费用凭证
                            </dt>
                            <dd className="break-all text-body-compact text-foreground">
                                {preview.input.evidence.name}
                            </dd>
                        </div>
                    </dl>

                    <div className="space-y-1 rounded-lg bg-muted/35 px-4 py-2.5 text-xs text-muted-foreground">
                        <p>
                            ·
                            登记后将立即关联原询价费用并扣减可登记额度，不生成费用采购或应付。
                        </p>
                        <p>
                            · 后续若发生财务费用冲减，原询价额度不会自动恢复。
                        </p>
                    </div>
                </div>

                <DialogFooter className="shrink-0 border-t border-grid bg-card px-6 py-3.5 sm:items-center sm:justify-between">
                    <Button
                        id="finance-inquiry-register-confirm-cancel"
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
                            id="finance-inquiry-register-confirm-close"
                            type="button"
                            variant="outline"
                            size="sm"
                            disabled={pending}
                            onClick={onCancel}
                        >
                            取消
                        </Button>
                        <LoadingButton
                            id="finance-inquiry-register-confirm-confirm"
                            type="button"
                            size="sm"
                            loading={pending}
                            disabled={pending || unresolved}
                            onClick={() => {
                                void onConfirm(preview)
                                    .then(() => setPreview(null))
                                    .catch(() => undefined)
                            }}
                        >
                            确认登记费用
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
                            可登记额度：
                            <MoneyValue
                                value={candidate.available_amount}
                                display="exact"
                                className="font-medium text-foreground"
                            />
                            <span className="mx-1.5">·</span>
                            确认税率：
                            <RateValue
                                value={initial.taxRatePercent}
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
                        id="finance-inquiry-register-change-source"
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

                <div className="grid grid-cols-1 gap-x-4 gap-y-3.5 sm:grid-cols-12">
                    <form.AppField
                        name="category"
                        children={(field) => (
                            <field.SelectField
                                id="finance-inquiry-register-category"
                                label="费用类别"
                                required
                                allowClear={false}
                                disabled={pending || unresolved}
                                options={REGISTER_CATEGORY_OPTIONS}
                                className="sm:col-span-4"
                            />
                        )}
                    />
                    <form.AppField name="description">
                        {(field) => (
                            <field.TextField
                                id="finance-inquiry-register-description"
                                label="费用说明"
                                disabled={pending || unresolved}
                                required
                                className="sm:col-span-8"
                            />
                        )}
                    </form.AppField>
                </div>

                <div className="grid grid-cols-1 items-start gap-x-4 gap-y-3.5 sm:grid-cols-12">
                    <form.AppField name="grossAmount">
                        {(field) => (
                            <field.TextField
                                id="finance-inquiry-register-amount"
                                money
                                label="含税金额（元）"
                                inputMode="decimal"
                                inputClassName="num"
                                disabled={pending || unresolved}
                                required
                                className="sm:col-span-5"
                            />
                        )}
                    </form.AppField>
                    <Field className="sm:col-span-3">
                        <FieldLabel>进项税率</FieldLabel>
                        <div
                            title="沿用询价确认税率，不可修改"
                            aria-label={`进项税率 ${lockedTaxRateDisplay}%，沿用询价确认税率`}
                            className="flex h-control items-center justify-between rounded-md border border-border/80 bg-muted/45 px-3 text-body-compact"
                        >
                            <span className="num font-medium text-foreground">
                                {lockedTaxRateDisplay}%
                            </span>
                            <LockIcon
                                aria-hidden="true"
                                className="size-3.5 shrink-0 text-muted-foreground"
                            />
                        </div>
                    </Field>
                    <form.AppField name="occurredOn">
                        {(field) => (
                            <field.DateField
                                id="finance-inquiry-register-date"
                                label="发生日期"
                                required
                                clearable={false}
                                disabled={pending || unresolved}
                                disabledDates={{
                                    after: new Date(`${todayDate()}T00:00:00`),
                                }}
                                className="sm:col-span-4"
                            />
                        )}
                    </form.AppField>
                </div>

                <form.Subscribe selector={(state) => state.values.grossAmount}>
                    {(grossAmount) => (
                        <ExtraCostAmountBreakdown
                            grossAmount={grossAmount}
                            taxRatePercent={initial.taxRatePercent}
                            quotaLimit={candidate.available_amount}
                        />
                    )}
                </form.Subscribe>

                <form.AppField name="evidence">
                    {(field) => (
                        <field.EvidenceUploadField
                            id="finance-inquiry-register-evidence"
                            label="费用凭证"
                            required
                            density="compact"
                            uploadLabel="上传 ERP 外付款或费用发生凭证"
                            description="支持 PDF、JPG、PNG 或 WebP，不超过 5 MB。"
                            disabled={pending || unresolved}
                        />
                    )}
                </form.AppField>

                <div className="space-y-3 border-t border-border/60 pt-4">
                    <div className="flex flex-wrap items-baseline gap-2">
                        <span className="text-xs font-medium text-foreground/80">
                            归属与收款方
                        </span>
                        <span className="text-xs text-muted-foreground">
                            选填，默认归属整张销售单
                        </span>
                    </div>
                    <div className="grid grid-cols-1 gap-4 sm:grid-cols-2">
                        <form.AppField name="salesOrderLineId">
                            {(field) => (
                                <field.SelectField
                                    id="finance-inquiry-register-line"
                                    label="归属明细"
                                    allowClear={false}
                                    disabled={pending || unresolved}
                                    options={lineOptions}
                                />
                            )}
                        </form.AppField>
                        <form.AppField name="supplierId">
                            {(field) => (
                                <Field>
                                    <FieldLabel htmlFor="finance-inquiry-register-supplier">
                                        收款供应商
                                    </FieldLabel>
                                    <SupplierSearchCombobox
                                        id="finance-inquiry-register-supplier"
                                        value={field.state.value || undefined}
                                        disabled={pending || unresolved}
                                        onValueChange={(id) =>
                                            field.handleChange(id ?? "")
                                        }
                                        placeholder="选填，收款方已建档时选择"
                                    />
                                </Field>
                            )}
                        </form.AppField>
                    </div>
                </div>

                {storageFailure ? (
                    <Alert variant="destructive">
                        <AlertDescription>
                            费用输入或凭证未能暂存，请清理浏览器缓存或关闭无痕模式后重试。当前内容仍保留在页面中，尚未登记。
                        </AlertDescription>
                    </Alert>
                ) : null}
            </div>

            <DialogFooter className="shrink-0 border-t border-grid bg-card px-6 py-3.5 sm:items-center sm:justify-between">
                <Button
                    id="finance-inquiry-register-cancel"
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
                        id="finance-inquiry-register-dialog-cancel"
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
                            id="finance-inquiry-register-preview"
                            size="sm"
                            showError={false}
                            label="下一步：核对登记"
                            disabled={pending || unresolved || storageFailure}
                        />
                    </form.AppForm>
                </div>
            </DialogFooter>
        </form>
    )
}
