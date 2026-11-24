"use client"

import { useEffect, useRef, useState } from "react"
import { useStore } from "@tanstack/react-form"
import { useMutation, useQueryClient } from "@tanstack/react-query"

import { MoneyValue, WorkspaceTaskFooter } from "@/components/business"
import { useAppForm } from "@/components/form"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Button } from "@/components/ui/button"
import { Field, FieldError, FieldLabel } from "@/components/ui/field"
import { Input } from "@/components/ui/input"
import { LoadingButton } from "@/components/ui/loading-button"
import { toast } from "@/components/ui/toast"
import { InvoiceFilesField } from "@/features/customer-receivables/components/invoice-files-field"
import { invoiceFilesError } from "@/features/customer-receivables/lib/invoice-files"
import type { InvoiceRequest } from "@/features/invoice-requests/api"
import { commandFailureDisposition } from "@/lib/api/command-recovery"
import { getErrorMessage } from "@/lib/api/errors"
import {
    compareDecimal,
    normalizeFixed,
    subtractFixed,
} from "@/lib/fixed-decimal"
import {
    FormalCommandKeyLedger,
    type FormalCommandIdentity,
} from "@/lib/formal-command"
import { queryKeyRoots } from "@/lib/query-key-roots"
import { cn } from "@/lib/utils"
import { useWorkspaceDirtyState } from "@/lib/workspace-form-guard"
import { isWorkspaceFrameActive } from "@/lib/workspace-hotkeys"
import { completeInvoice } from "../api/invoice-evidence"
import type { WorkspaceWorkItem } from "../types"

const REGISTER_INVOICE = "register-invoice"
const currencyScale = { maxScale: 2, outputScale: 2 }
const wrapMoney =
    "[&_[data-slot=money-value-number]]:whitespace-normal [&_[data-slot=money-value-number]]:wrap-anywhere"
const affectedQueries = [
    queryKeyRoots.invoiceRequests,
    queryKeyRoots.customerReceivables,
    queryKeyRoots.salesOrders,
    queryKeyRoots.workItems,
    queryKeyRoots.workspaceHome,
]
const filesIdPrefix = "customer-receivables-session-invoice-files"

type InvoiceSubmission = {
    command: Omit<Parameters<typeof completeInvoice>[0], "idempotency_key">
    amount: string
    files: readonly File[]
}

function declaredAmount(value: string): string | null {
    try {
        return normalizeFixed(
            value.trim().replace(/^0+(?=\d)/, ""),
            currencyScale,
        )
    } catch {
        return null
    }
}

function amountValidation(value: string, remaining: string | null) {
    if (!value.trim()) return "请输入本次实际开票金额"
    const amount = declaredAmount(value)
    if (amount === null || compareDecimal(amount, "0", 2) <= 0) {
        return "请输入大于 0 元且最多保留两位小数的金额"
    }
    if (remaining === null || compareDecimal(amount, remaining, 2) !== 0) {
        return "本次登记金额须等于待开票余额"
    }
}

function filesValidation(files: readonly File[]) {
    return files.length ? invoiceFilesError(files) : "请上传至少一个发票附件"
}

/** 工作台单栏、双栏切换时保留表单和已提交命令。 */
export function useWorkspaceInvoiceRegistration({
    item,
    request,
    amount,
    canRegister,
    canVerify,
    onTaskCompleted,
}: {
    item: WorkspaceWorkItem
    request?: InvoiceRequest
    amount: string | null
    canRegister: boolean
    canVerify: boolean
    onTaskCompleted?: (id: string) => void
}) {
    const queryClient = useQueryClient()
    const [ledger] = useState(() => new FormalCommandKeyLedger())
    const submitting = useRef(false)
    const complete = useMutation({
        mutationFn: ({
            payload,
            idempotencyKey,
        }: FormalCommandIdentity<InvoiceSubmission>) =>
            completeInvoice(
                { ...payload.command, idempotency_key: idempotencyKey },
                payload.files,
            ),
        onError: (error) =>
            ledger.settle(
                REGISTER_INVOICE,
                commandFailureDisposition(error) === "unknown"
                    ? "unknown"
                    : "failed",
            ),
        onSuccess: () => {
            ledger.settle(REGISTER_INVOICE, "succeeded")
            toast.add({
                title: "发票已登记，本次开票任务已完成",
                type: "success",
            })
            void Promise.all(
                affectedQueries.map((queryKey) =>
                    queryClient.invalidateQueries({ queryKey }),
                ),
            )
            onTaskCompleted?.(item.workItemId)
        },
    })
    const disposition = complete.isError
        ? commandFailureDisposition(complete.error)
        : undefined
    const uncertain = disposition === "unknown"
    const conflicted = disposition === "conflict"
    const refresh = useMutation({
        mutationFn: async () => {
            await Promise.all(
                [
                    queryKeyRoots.invoiceRequests,
                    queryKeyRoots.customerReceivables,
                    queryKeyRoots.workItems,
                ].map((queryKey) =>
                    queryClient.invalidateQueries(
                        { queryKey },
                        { throwOnError: true },
                    ),
                ),
            )
        },
        onSuccess: () => {
            complete.reset()
            toast.add({
                title: "开票资料已刷新，请重新核对后提交",
                type: "success",
            })
        },
        onError: (error) => {
            toast.add({
                title: getErrorMessage(error, "刷新开票资料失败，请重试"),
                type: "error",
            })
        },
    })

    const form = useAppForm({
        defaultValues: {
            amountOverride: null as string | null,
            files: [] as File[],
        },
        onSubmit: async ({ value }) => {
            if (submitting.current || complete.isSuccess || conflicted) return
            submitting.current = true
            try {
                const previous =
                    ledger.peek<InvoiceSubmission>(REGISTER_INVOICE)
                if (previous) {
                    if (canVerify) await complete.mutateAsync(previous)
                    return
                }
                if (
                    !canRegister ||
                    !request ||
                    amount === null ||
                    amountValidation(value.amountOverride ?? amount, amount) ||
                    filesValidation(value.files)
                )
                    return

                let command
                try {
                    command = ledger.acquire<InvoiceSubmission>(
                        REGISTER_INVOICE,
                        "invoice-evidence",
                        {
                            command: {
                                work_item_id: item.workItemId,
                                expected_task_version: item.taskVersion,
                                expected_request_version: request.version,
                            },
                            amount,
                            files: value.files,
                        },
                    )
                } catch (error) {
                    toast.add({
                        title: getErrorMessage(error, "暂时无法准备发票登记"),
                        type: "error",
                    })
                    return
                }
                await complete.mutateAsync(command)
            } catch {
                // 变更状态保留错误，结果未知时账本保留原命令。
            } finally {
                submitting.current = false
            }
        },
    })

    return {
        form,
        complete,
        refresh,
        canRegister,
        canVerify,
        conflicted,
        uncertain,
        verifying: canVerify && (complete.isPending || uncertain),
        amount:
            ledger.peek<InvoiceSubmission>(REGISTER_INVOICE)?.payload.amount ??
            amount,
    }
}

export function WorkspaceInvoiceUpload({
    idPrefix,
    registration,
}: {
    idPrefix: string
    registration: ReturnType<typeof useWorkspaceInvoiceRegistration>
}) {
    const {
        form,
        complete,
        refresh,
        amount,
        uncertain,
        conflicted,
        canRegister,
        canVerify,
    } = registration
    const amountId = `${idPrefix}-registered-amount`
    const amountErrorId = `${amountId}-error`
    const [highlightedTarget, setHighlightedTarget] = useState<
        "amount" | "files" | null
    >(null)
    const highlightTimer = useRef<ReturnType<typeof setTimeout>>(null)
    useEffect(
        () => () => {
            if (highlightTimer.current) clearTimeout(highlightTimer.current)
        },
        [],
    )

    const { values, isSubmitting } = useStore(form.store)
    const { amountOverride, files } = values
    const value = amountOverride ?? amount ?? ""
    const normalized = declaredAmount(value)
    const amountError = amountValidation(value, amount)
    const filesError = filesValidation(files)
    const issueCount = [amountError, filesError].filter(Boolean).length
    const remaining =
        normalized !== null && amount !== null
            ? subtractFixed(amount, normalized, currencyScale)
            : null
    const loading = complete.isPending || isSubmitting
    const locked =
        loading || complete.isSuccess || uncertain || conflicted || !canRegister
    useWorkspaceDirtyState({
        dirty: !complete.isSuccess && (files.length > 0 || uncertain),
        busy: loading,
    })
    const canSubmit =
        !loading &&
        !complete.isSuccess &&
        (uncertain ? canVerify : canRegister && !conflicted && issueCount === 0)
    const submit = () => {
        if (canSubmit) void form.handleSubmit()
    }
    const triggerHighlight = (target: "amount" | "files") => {
        if (highlightTimer.current) clearTimeout(highlightTimer.current)
        setHighlightedTarget(target)
        highlightTimer.current = setTimeout(
            () => setHighlightedTarget(null),
            2200,
        )
    }
    const activeFooter = (
        <>
            {!locked && issueCount > 0 ? (
                <Button
                    id={`${idPrefix}-locate-issue`}
                    type="button"
                    variant="ghost"
                    size="sm"
                    onClick={(event) => {
                        const target: "amount" | "files" = amountError
                            ? "amount"
                            : "files"
                        form.setFieldMeta(
                            amountError ? "amountOverride" : "files",
                            (meta) => ({ ...meta, isTouched: true }),
                        )
                        triggerHighlight(target)
                        const first =
                            event.currentTarget.ownerDocument.getElementById(
                                amountError
                                    ? amountId
                                    : `${filesIdPrefix}-select`,
                            )
                        const behavior = window.matchMedia(
                            "(prefers-reduced-motion: reduce)",
                        ).matches
                            ? "auto"
                            : "smooth"
                        first?.scrollIntoView({
                            block: "center",
                            behavior,
                        })
                        first?.focus({ preventScroll: true })
                    }}
                >
                    <span className="num">{issueCount}</span> 项待完善 · 定位
                </Button>
            ) : null}
            <LoadingButton
                id="workspace-invoice-confirm"
                type="button"
                size="sm"
                variant="default"
                loading={loading}
                disabled={!canSubmit}
                onClick={submit}
            >
                {uncertain ? "核对提交结果" : "确认登记发票"}
            </LoadingButton>
        </>
    )
    const summary = (
        <dl className="flex min-w-0 flex-wrap items-center gap-x-4 gap-y-1 text-xs">
            <div className="flex min-w-0 flex-wrap items-baseline gap-x-1.5">
                <dt className="text-muted-foreground">本次开票金额</dt>
                <dd>
                    <MoneyValue
                        value={normalized}
                        display="exact"
                        className={`font-semibold ${wrapMoney}`}
                    />
                </dd>
            </div>
            <div className="flex min-w-0 flex-wrap items-baseline gap-x-1.5">
                <dt className="text-muted-foreground">登记后剩余待开票</dt>
                <dd>
                    <MoneyValue
                        value={remaining}
                        display="exact"
                        className={wrapMoney}
                    />
                </dd>
            </div>
            <div className="flex gap-x-1.5">
                <dt className="text-muted-foreground">附件数</dt>
                <dd className="num">{files.length}</dd>
            </div>
        </dl>
    )

    return (
        <section
            aria-label="销项发票登记"
            className="min-w-0 space-y-3 border-t border-border/60 bg-card pt-3"
        >
            <h3 className="text-body-compact font-semibold">实际发票信息</h3>
            {complete.isError ? (
                <Alert
                    variant={
                        uncertain || conflicted ? "warning" : "destructive"
                    }
                    className="border-l-2"
                >
                    <AlertTitle>
                        {uncertain
                            ? "登记结果待确认"
                            : conflicted
                              ? "开票资料已变化"
                              : "发票登记失败"}
                    </AlertTitle>
                    <AlertDescription>
                        {uncertain
                            ? "请保留当前附件，点击「核对提交结果」查询本次登记。"
                            : getErrorMessage(complete.error, "请重试")}
                        {conflicted ? (
                            <LoadingButton
                                id={`${idPrefix}-refresh-conflict`}
                                type="button"
                                size="xs"
                                variant="outline"
                                className="ml-2"
                                loading={refresh.isPending}
                                onClick={() => refresh.mutate()}
                            >
                                刷新最新数据
                            </LoadingButton>
                        ) : null}
                    </AlertDescription>
                </Alert>
            ) : null}
            {complete.isSuccess ? (
                <Alert variant="success" className="border-l-2">
                    <AlertTitle>本次开票已完成</AlertTitle>
                    <AlertDescription>
                        附件已保存，可在销售单详情下载。
                    </AlertDescription>
                </Alert>
            ) : null}
            <form.AppField
                name="amountOverride"
                validators={{
                    onSubmit: ({ value }) =>
                        uncertain
                            ? undefined
                            : amountValidation(value ?? amount ?? "", amount),
                }}
            >
                {(field) => {
                    const error = field.state.meta.isTouched
                        ? amountError
                        : undefined
                    return (
                        <Field
                            data-invalid={Boolean(error) || undefined}
                            className={cn(
                                "min-w-0 gap-1.5 rounded-md transition-colors",
                                highlightedTarget === "amount" &&
                                    "bg-info-soft/25 px-2 py-1.5 ring-1 ring-info-border motion-safe:animate-pulse",
                            )}
                        >
                            <FieldLabel htmlFor={amountId}>
                                本次登记 / 待开票余额
                            </FieldLabel>
                            <div className="flex min-w-0 flex-wrap items-center gap-x-2 gap-y-1">
                                <Input
                                    id={amountId}
                                    type="text"
                                    inputMode="decimal"
                                    autoComplete="off"
                                    aria-label="本次登记金额"
                                    aria-invalid={Boolean(error) || undefined}
                                    aria-describedby={
                                        error ? amountErrorId : undefined
                                    }
                                    className="num h-control-sm w-32 max-w-full border-transparent bg-surface-sunken text-body-compact shadow-none hover:bg-card focus-visible:bg-card"
                                    value={value}
                                    disabled={locked}
                                    onChange={(event) => {
                                        if (complete.isError) complete.reset()
                                        field.handleChange(event.target.value)
                                    }}
                                    onBlur={field.handleBlur}
                                    onKeyDown={(event) => {
                                        if (
                                            event.defaultPrevented ||
                                            event.nativeEvent.isComposing ||
                                            !isWorkspaceFrameActive() ||
                                            event.key !== "Enter" ||
                                            (!event.ctrlKey && !event.metaKey)
                                        )
                                            return
                                        event.preventDefault()
                                        submit()
                                    }}
                                />
                                <span
                                    aria-hidden="true"
                                    className="text-muted-foreground"
                                >
                                    /
                                </span>
                                <MoneyValue
                                    value={amount}
                                    display="exact"
                                    className={`text-body-compact ${wrapMoney}`}
                                />
                                {amountError && amount !== null && !locked ? (
                                    <Button
                                        id={`${idPrefix}-fill-remaining`}
                                        type="button"
                                        variant="outline"
                                        size="xs"
                                        onClick={() => {
                                            if (complete.isError)
                                                complete.reset()
                                            field.handleChange(amount)
                                        }}
                                    >
                                        填剩余金额
                                    </Button>
                                ) : null}
                            </div>
                            {error ? (
                                <FieldError
                                    id={amountErrorId}
                                    errors={[{ message: error }]}
                                />
                            ) : null}
                        </Field>
                    )
                }}
            </form.AppField>
            <form.AppField
                name="files"
                validators={{
                    onChange: ({ value }) => filesValidation(value),
                }}
            >
                {(field) => (
                    <div
                        className={cn(
                            "rounded-md transition-colors",
                            highlightedTarget === "files" &&
                                "bg-info-soft/25 p-2 ring-1 ring-info-border motion-safe:animate-pulse",
                        )}
                    >
                        <InvoiceFilesField
                            idPrefix={filesIdPrefix}
                            files={field.state.value}
                            disabled={locked}
                            label="发票附件"
                            density="compact"
                            required
                            errors={
                                filesError && field.state.meta.isTouched
                                    ? [{ message: filesError }]
                                    : []
                            }
                            description="发票合计应与本次登记金额一致。"
                            onChange={(nextFiles) => {
                                if (complete.isError) complete.reset()
                                if (
                                    nextFiles.length &&
                                    amountOverride === null
                                ) {
                                    form.setFieldValue("amountOverride", amount)
                                }
                                field.handleChange(nextFiles)
                            }}
                        />
                    </div>
                )}
            </form.AppField>
            <WorkspaceTaskFooter
                summary={summary}
                fallback={
                    <div className="flex w-full min-w-0 flex-wrap items-center justify-between gap-x-3 gap-y-2">
                        {summary}
                        <div className="ml-auto flex min-w-0 flex-wrap items-center justify-end gap-2">
                            {activeFooter}
                        </div>
                    </div>
                }
            >
                {activeFooter}
            </WorkspaceTaskFooter>
        </section>
    )
}
