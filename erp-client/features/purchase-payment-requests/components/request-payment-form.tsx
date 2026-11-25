"use client"

import { useEffect, useRef, useState } from "react"
import { useQueryClient } from "@tanstack/react-query"
import { TriangleAlertIcon } from "lucide-react"
import {
    useWorkspaceTaskNavigation,
    useWorkspaceTaskPage,
} from "@/components/business/workspace-task-pane"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import {
    Tooltip,
    TooltipContent,
    TooltipProvider,
    TooltipTrigger,
} from "@/components/ui/tooltip"
import { isWorkspaceFrameActive } from "@/lib/workspace-hotkeys"
import { RequestDepositExecution } from "./request-deposit-execution"
import { z } from "zod"
import { useAppForm } from "@/components/form"
import {
    MoneyValue,
    RateValue,
    SensitiveValue,
    WorkspaceFactGrid,
    WorkspaceFactGroup,
    WorkspaceFactItem,
    WorkspaceTaskFooter,
} from "@/components/business"
import { Button } from "@/components/ui/button"
import { Field, FieldError, FieldLabel } from "@/components/ui/field"
import { fileUploadText } from "@/lib/ui-text"
import { FileUpload } from "@/components/ui/file-upload"
import { SupplierPaymentSubmitConfirmDialog } from "@/features/supplier-payables/components/supplier-payment-submit-confirm-dialog"
import { useAccountProfileQuery } from "@/features/auth/queries"
import { hasPermission } from "@/lib/permissions"
import { getErrorMessage, isApiError } from "@/lib/api"
import {
    clampZeroFixed,
    compareDecimal,
    divideFixed,
    minFixed,
    multiplyFixed,
    subtractFixed,
} from "@/lib/fixed-decimal"
import {
    allocateRequestPayment,
    paymentBlockReason,
    positivePaymentAmount,
} from "../lib/payment"
import {
    FormalCommandKeyLedger,
    isCommandOutcomeUnknown,
    type FormalCommandIdentity,
} from "@/lib/formal-command"
import { paymentRequestKeys, useRequestPaymentMutation } from "../hooks/queries"
import { revealPaymentRecipient } from "../api"
import type {
    PaymentInput,
    PaymentRecipient,
    PaymentRequest,
    PaymentTaskContext,
    RevealedPaymentRecipient,
} from "../types"

type PaymentIdempotencySlot = FormalCommandIdentity<{
    payment: Omit<PaymentInput, "idempotencyKey">
    recipient: PaymentRecipient
    completesRequest: boolean
    unpaidAmount: string
}>

const money = { maxScale: 2, outputScale: 2, allowNegative: true } as const

/** 每笔付款保持独立命令身份；响应未知时重试原金额、回单及分配。 */
function BankRequestPaymentForm({
    request,
    task,
    onTaskCompleted,
}: {
    request: PaymentRequest & { payment_recipient: PaymentRecipient }
    task: PaymentTaskContext
    onTaskCompleted?: () => void
}) {
    const payment = useRequestPaymentMutation(onTaskCompleted)
    const profile = useAccountProfileQuery()
    const [revealedState, setRevealedState] = useState<{
        key: string
        recipient: RevealedPaymentRecipient
    } | null>(null)
    const [unknown, setUnknown] = useState(false)
    const [result, setResult] = useState<string>()
    const [ledger] = useState(() => new FormalCommandKeyLedger())
    const formRef = useRef<HTMLFormElement>(null)
    const inFlight = useRef(false)
    const slotKey = JSON.stringify([request.id, task.workItemId])
    const [pending, setPending] = useState<PaymentIdempotencySlot>()
    const queryClient = useQueryClient()
    const navigation = useWorkspaceTaskNavigation()
    const fullPage = useWorkspaceTaskPage()
    const id = "ppr-pay"
    const formId = `${id}-form`
    const account = request.payment_recipient
    const recipientKey = `${account.bank_account_id}:${account.version}`
    const revealedRecipient =
        revealedState?.key === recipientKey ? revealedState.recipient : null
    const paymentTerms = [
        ...new Set(
            request.lines
                .map((line) => line.payment_term_name.trim())
                .filter(Boolean),
        ),
    ].join("、")
    const bankName =
        revealedRecipient?.bank_name?.trim() || account.bank_name?.trim() || ""
    const branchName =
        revealedRecipient?.bank_branch_name?.trim() ||
        account.bank_branch_name?.trim() ||
        ""
    const bankDisplay =
        bankName && bankName !== "未记录"
            ? branchName && branchName !== bankName
                ? `${bankName} · ${branchName}`
                : bankName
            : branchName || "未记录"
    const recipientAccountName = account.account_name?.trim() || ""
    const recipientAccountNameMissing =
        !recipientAccountName || recipientAccountName === "未记录"
    const blocked =
        !account.bank_account_id ||
        !account.version ||
        !account.account_number_masked ||
        account.account_number_masked === "未记录"
            ? "原申请缺少完整的收款账户，请核对原申请。"
            : paymentBlockReason(request)
    const rateLimited =
        Boolean(pending) &&
        payment.isError &&
        isApiError(payment.error) &&
        payment.error.status === 429
    const conflict =
        payment.isError &&
        isApiError(payment.error) &&
        payment.error.status === 409 &&
        !unknown
    const locked = unknown || payment.isPending || conflict || Boolean(blocked)
    const paidRate =
        compareDecimal(request.total_amount, "0", 4) > 0
            ? divideFixed(
                  multiplyFixed(
                      clampZeroFixed(request.paid_amount, {
                          maxScale: 4,
                          outputScale: 4,
                      }),
                      "100",
                      { leftMaxScale: 4, rightMaxScale: 0, outputScale: 4 },
                  ),
                  request.total_amount,
                  {
                      numeratorMaxScale: 4,
                      denominatorMaxScale: 4,
                      outputScale: 1,
                  },
              )
            : "0"
    const schema = z
        .object({
            amount: z.string().trim(),
            paidAt: z.string().min(1, "请填写实际付款时间"),
            reference: z.string(),
            receipt: z
                .custom<File | null>()
                .refine(
                    (file) => file instanceof File && file.size > 0,
                    "请上传银行回单",
                ),
        })
        .superRefine((value, context) => {
            try {
                allocateRequestPayment(request, value.amount)
            } catch (error) {
                context.addIssue({
                    code: "custom",
                    path: ["amount"],
                    message: getErrorMessage(error, "请核对实际付款金额"),
                })
            }
        })
    const form = useAppForm({
        defaultValues: {
            amount: blocked ? "" : request.unpaid_amount,
            paidAt: new Date(
                Date.now() - new Date().getTimezoneOffset() * 60000,
            )
                .toISOString()
                .slice(0, 16),
            reference: "",
            receipt: null as File | null,
        },
        validators: { onSubmit: schema },
        onSubmit: ({ value }) => {
            if (!value.receipt || locked) return
            const slot = ledger.acquire(slotKey, "ppr-pay", {
                payment: {
                    requestId: request.id,
                    expectedVersion: request.version,
                    supplierId: request.supplier_id,
                    workItemId: task.workItemId,
                    expectedTaskVersion: task.taskVersion,
                    amount: value.amount,
                    paidAt: value.paidAt,
                    bankReference: value.reference.trim(),
                    receipt: value.receipt,
                    ...allocateRequestPayment(request, value.amount),
                },
                recipient: account,
                unpaidAmount: request.unpaid_amount,
                completesRequest:
                    compareDecimal(value.amount, request.unpaid_amount, 2) ===
                    0,
            })
            payment.reset()
            setPending(slot)
        },
    })
    useEffect(() => {
        const element = formRef.current
        if (!element || locked || pending) return
        const submitOnShortcut = (event: KeyboardEvent) => {
            if (
                event.key !== "Enter" ||
                (!event.ctrlKey && !event.metaKey) ||
                event.isComposing ||
                event.repeat ||
                event.defaultPrevented ||
                !isWorkspaceFrameActive() ||
                document.querySelector(
                    '[data-slot="popover-content"][data-open]',
                )
            )
                return
            event.preventDefault()
            element.requestSubmit()
        }
        element.addEventListener("keydown", submitOnShortcut)
        return () => element.removeEventListener("keydown", submitOnShortcut)
    }, [locked, pending])

    useEffect(() => {
        if (!unknown || !pending) return
        if (
            request.version !== pending.payload.payment.expectedVersion ||
            task.taskVersion !== pending.payload.payment.expectedTaskVersion
        ) {
            ledger.settle(slotKey, "succeeded")
            setUnknown(false)
            setPending(undefined)
            form.setFieldValue("amount", blocked ? "" : request.unpaid_amount)
        }
    }, [
        blocked,
        form,
        ledger,
        pending,
        request.unpaid_amount,
        request.version,
        slotKey,
        task.taskVersion,
        unknown,
    ])

    async function confirmPayment() {
        if (!pending || inFlight.current) return
        inFlight.current = true
        const { payload, idempotencyKey } = pending
        try {
            const applied = await payment.mutateAsync({
                payment: { ...payload.payment, idempotencyKey },
                recipient: payload.recipient,
                completesRequest: payload.completesRequest,
            })
            ledger.settle(slotKey, "succeeded")
            setPending(undefined)
            setUnknown(false)
            setResult(`付款 ${applied.payment_no} 已登记并核销。`)
            form.reset({
                ...form.state.values,
                amount: subtractFixed(
                    payload.unpaidAmount,
                    payload.payment.amount,
                    money,
                ),
                reference: "",
                receipt: null,
            })
        } catch (error) {
            if (isCommandOutcomeUnknown(error)) {
                ledger.settle(slotKey, "unknown")
                setUnknown(true)
                return
            }
            if (isApiError(error) && error.status === 429) {
                throw error
            }
            ledger.settle(slotKey, "failed")
            setUnknown(false)
            setPending(undefined)
        } finally {
            inFlight.current = false
        }
    }

    const closeConfirmation = (open: boolean) => {
        if (inFlight.current) return
        if (!open && !unknown) {
            ledger.settle(slotKey, "failed")
            setPending(undefined)
        }
    }

    const refreshPaymentBasis = async () => {
        await queryClient.invalidateQueries({
            queryKey: paymentRequestKeys.detail(request.id, task.workItemId),
        })
        navigation?.onRefresh?.()
        payment.reset()
    }

    return (
        <section
            aria-label="当前付款任务"
            className="@container/payment-request min-w-0 space-y-4 bg-card pb-4 pt-3"
        >
            {fullPage && (
                <TooltipProvider>
                    <section
                        aria-label="付款任务摘要"
                        className="grid min-w-0 gap-x-6 gap-y-3.5 rounded-lg bg-surface-sunken px-4 py-3.5 @[480px]/payment-request:grid-cols-2 @[840px]/payment-request:grid-cols-4"
                    >
                        <div className="min-w-0">
                            <span className="text-xs text-muted-foreground">
                                本申请剩余待付
                            </span>
                            <span className="ml-1.5 text-tiny text-muted-foreground/80">
                                申请 − 已结
                            </span>
                            <Tooltip>
                                <TooltipTrigger
                                    id={`${id}-unpaid-formula`}
                                    aria-label="本申请剩余待付计算口径"
                                    render={
                                        <Button
                                            variant="ghost"
                                            size="icon-xs"
                                            type="button"
                                            className="ml-1 size-4 rounded-full border border-border/80 align-middle text-tiny text-muted-foreground hover:bg-card hover:text-foreground"
                                        />
                                    }
                                >
                                    <span aria-hidden="true">?</span>
                                </TooltipTrigger>
                                <TooltipContent>
                                    申请总额减去已结算金额与已关闭金额。
                                </TooltipContent>
                            </Tooltip>
                            <div className="mt-1 min-w-0">
                                <MoneyValue
                                    value={request.unpaid_amount}
                                    size="section"
                                    display="exact"
                                />
                            </div>
                        </div>
                        <div className="min-w-0">
                            <div className="flex flex-wrap items-center gap-1.5 text-xs text-muted-foreground">
                                <span>申请总额</span>
                                <span className="text-tiny text-muted-foreground/80">
                                    含税
                                </span>
                                <Tooltip>
                                    <TooltipTrigger
                                        id={`${id}-total-formula`}
                                        aria-label="申请总额计算口径"
                                        render={
                                            <Button
                                                variant="ghost"
                                                size="icon-xs"
                                                type="button"
                                                className="size-4 rounded-full border border-border/80 text-tiny text-muted-foreground hover:bg-card hover:text-foreground"
                                            />
                                        }
                                    >
                                        <span aria-hidden="true">?</span>
                                    </TooltipTrigger>
                                    <TooltipContent>
                                        本次付款申请审批通过的含税申请总额。
                                    </TooltipContent>
                                </Tooltip>
                            </div>
                            <div className="mt-1 min-w-0">
                                <MoneyValue
                                    value={request.total_amount}
                                    display="exact"
                                    className="num text-value-compact font-semibold"
                                />
                            </div>
                            <p className="mt-0.5 text-tiny text-muted-foreground">
                                <span className="num">
                                    {request.lines.length}
                                </span>{" "}
                                张采购单
                            </p>
                        </div>
                        <div className="min-w-0">
                            <div className="flex flex-wrap items-center gap-1.5 text-xs text-muted-foreground">
                                <span>本申请已结算</span>
                                <span className="text-tiny text-muted-foreground/80">
                                    已核销
                                </span>
                                <Tooltip>
                                    <TooltipTrigger
                                        id={`${id}-paid-formula`}
                                        aria-label="本申请已结算计算口径"
                                        render={
                                            <Button
                                                variant="ghost"
                                                size="icon-xs"
                                                type="button"
                                                className="size-4 rounded-full border border-border/80 text-tiny text-muted-foreground hover:bg-card hover:text-foreground"
                                            />
                                        }
                                    >
                                        <span aria-hidden="true">?</span>
                                    </TooltipTrigger>
                                    <TooltipContent>
                                        本申请已登记并核销的累计付款金额及完成比例。
                                    </TooltipContent>
                                </Tooltip>
                            </div>
                            <div className="mt-1 min-w-0">
                                <MoneyValue
                                    value={request.paid_amount}
                                    display="exact"
                                    className="num text-value-compact font-semibold"
                                />
                            </div>
                            <div
                                aria-hidden="true"
                                className="mt-1 h-1.5 overflow-hidden rounded-full bg-muted"
                            >
                                <div
                                    className="h-full rounded-full bg-primary"
                                    style={{
                                        width: `${minFixed(paidRate, "100", { maxScale: 1, outputScale: 1 })}%`,
                                    }}
                                />
                            </div>
                            <p className="mt-0.5 text-tiny text-muted-foreground">
                                完成{" "}
                                <RateValue
                                    value={paidRate}
                                    precision={1}
                                    className="text-muted-foreground"
                                />
                                {compareDecimal(request.closed_amount, "0", 4) >
                                    0 && (
                                    <>
                                        {" "}
                                        · 已关闭{" "}
                                        <MoneyValue
                                            value={request.closed_amount}
                                            display="exact"
                                        />
                                    </>
                                )}
                            </p>
                        </div>
                        <div className="min-w-0">
                            <div className="flex flex-wrap items-center gap-1.5 text-xs text-muted-foreground">
                                <span>希望付款日</span>
                                <span className="text-tiny text-muted-foreground/80">
                                    银行转账
                                </span>
                            </div>
                            <p className="num mt-1 text-value-compact font-semibold text-foreground">
                                {request.desired_payment_date}
                            </p>
                            <p className="mt-0.5 truncate text-tiny text-muted-foreground">
                                申请人 {request.applicant_name}
                            </p>
                        </div>
                    </section>
                </TooltipProvider>
            )}
            {blocked && !unknown && (
                <div
                    role="alert"
                    className="flex items-start gap-2 rounded-r-md border-l-2 border-l-destructive bg-destructive-soft/65 px-3 py-2 text-xs text-destructive-soft-foreground"
                >
                    <TriangleAlertIcon
                        aria-hidden="true"
                        className="mt-0.5 size-3.5 shrink-0"
                    />
                    <span>{blocked}</span>
                </div>
            )}
            <WorkspaceFactGroup title="付款与收款账户">
                <WorkspaceFactGrid columns={3} ariaLabel="付款与收款账户">
                    <WorkspaceFactItem
                        label="我方付款主体"
                        value={request.our_company_name}
                    />
                    <WorkspaceFactItem label="付款方式" value="银行转账" />
                    <WorkspaceFactItem
                        label="希望付款日"
                        value={request.desired_payment_date}
                        numeric
                        selectable
                    />
                    <WorkspaceFactItem
                        label="收款户名"
                        value={recipientAccountName || "未记录"}
                        tone={
                            recipientAccountNameMissing
                                ? "destructive"
                                : "default"
                        }
                    />
                    <WorkspaceFactItem
                        label="开户行"
                        tone={
                            bankDisplay === "未记录" ? "destructive" : "default"
                        }
                    >
                        <div className="space-y-0.5">
                            <div>{bankDisplay}</div>
                            {revealedRecipient ? (
                                <p
                                    className={
                                        revealedRecipient.bank_routing_number
                                            ? "num text-xs font-normal select-all text-muted-foreground"
                                            : "num text-xs font-medium text-warning-soft-foreground"
                                    }
                                >
                                    联行号：
                                    {revealedRecipient.bank_routing_number ||
                                        "未填写"}
                                </p>
                            ) : null}
                        </div>
                    </WorkspaceFactItem>
                    <WorkspaceFactItem label="收款账号" numeric>
                        <SensitiveValue
                            key={recipientKey}
                            id={`${id}-recipient-account`}
                            label="收款账号"
                            maskedValue={account.account_number_masked}
                            onReveal={
                                account.bank_account_id &&
                                account.version > 0 &&
                                hasPermission(
                                    profile.data?.permissions,
                                    "party_bank_account:reveal",
                                )
                                    ? async () => {
                                          const revealed =
                                              await revealPaymentRecipient(
                                                  request,
                                                  task,
                                              )
                                          setRevealedState({
                                              key: recipientKey,
                                              recipient: revealed,
                                          })
                                          return revealed.account_number
                                      }
                                    : undefined
                            }
                        />
                    </WorkspaceFactItem>
                </WorkspaceFactGrid>
            </WorkspaceFactGroup>
            <WorkspaceFactGroup
                divider
                title="申请信息"
                className="border-b border-border/60 pb-4"
            >
                <WorkspaceFactGrid columns={3} ariaLabel="申请信息">
                    <WorkspaceFactItem
                        label="供应商"
                        value={request.supplier_name}
                    />
                    <WorkspaceFactItem
                        label="申请人"
                        value={request.applicant_name}
                    />
                    <WorkspaceFactItem
                        label="付款条件"
                        value={paymentTerms || "未标注付款条件"}
                        tone={paymentTerms ? "default" : "muted"}
                    />
                    <WorkspaceFactItem
                        label="申请说明"
                        value={request.description}
                        span="full"
                    />
                </WorkspaceFactGrid>
            </WorkspaceFactGroup>
            {unknown && (
                <Alert variant="warning" role="status">
                    <AlertTitle>付款登记结果待核对</AlertTitle>
                    <AlertDescription className="flex flex-wrap items-center justify-between gap-2">
                        <span>
                            已保留本次金额、回单和收款账户。点击「核对提交结果」继续核对原付款，或先刷新最新状态。
                        </span>
                        <Button
                            id={`${id}-unknown-refresh`}
                            type="button"
                            variant="outline"
                            size="xs"
                            disabled={payment.isPending}
                            onClick={() => void refreshPaymentBasis()}
                        >
                            刷新最新数据
                        </Button>
                    </AlertDescription>
                </Alert>
            )}
            {conflict && (
                <Alert variant="warning">
                    <AlertTitle>付款依据已更新</AlertTitle>
                    <AlertDescription className="flex flex-wrap items-center gap-2">
                        {getErrorMessage(
                            payment.error,
                            "请刷新后重新核对付款依据。",
                        )}
                        <Button
                            id={`${id}-refresh`}
                            type="button"
                            variant="outline"
                            size="xs"
                            onClick={() => void refreshPaymentBasis()}
                        >
                            刷新最新数据
                        </Button>
                    </AlertDescription>
                </Alert>
            )}
            <form
                id={formId}
                ref={formRef}
                className="space-y-4"
                onSubmit={(event) => {
                    event.preventDefault()
                    if (!locked) void form.handleSubmit()
                }}
            >
                <h2 className="border-t border-border/60 pt-4 text-section-title font-semibold">
                    登记实际付款
                </h2>
                <div className="grid items-start gap-4 @[600px]/payment-request:grid-cols-3">
                    <form.AppField name="amount">
                        {(field) => (
                            <div className="space-y-1">
                                <field.TextField
                                    money
                                    id={`${id}-amount`}
                                    label="本次实际付款金额"
                                    inputMode="decimal"
                                    inputClassName="num border-transparent bg-surface-sunken hover:bg-card focus-visible:bg-card"
                                    placeholder="按银行回单填写金额"
                                    required
                                    disabled={locked}
                                />
                                <Button
                                    id={`${id}-fill-unpaid`}
                                    type="button"
                                    variant="ghost"
                                    size="xs"
                                    disabled={locked}
                                    onClick={() =>
                                        field.handleChange(
                                            request.unpaid_amount,
                                        )
                                    }
                                >
                                    填剩余待付
                                </Button>
                            </div>
                        )}
                    </form.AppField>
                    <form.AppField name="paidAt">
                        {(field) => (
                            <field.DateTimeField
                                id={`${id}-paid-at`}
                                label="实际付款时间"
                                required
                                clearable={false}
                                timeZone={
                                    Intl.DateTimeFormat().resolvedOptions()
                                        .timeZone
                                }
                                disabled={locked}
                            />
                        )}
                    </form.AppField>
                    <form.AppField name="reference">
                        {(field) => (
                            <field.TextField
                                id={`${id}-bank-reference`}
                                label="银行流水号"
                                disabled={locked}
                            />
                        )}
                    </form.AppField>
                </div>
                <form.Field name="receipt">
                    {(field) => (
                        <Field>
                            <FieldLabel htmlFor={`${id}-bank-receipt-input`}>
                                银行回单
                                <span
                                    aria-hidden="true"
                                    className="text-destructive"
                                >
                                    *
                                </span>
                            </FieldLabel>
                            <FileUpload
                                maxFileSize={5 * 1024 * 1024}
                                selectedDescription={
                                    fileUploadText.pendingSubmit
                                }
                                idPrefix={`${id}-bank-receipt`}
                                label="上传银行回单"
                                description="点击选择或拖入 JPG、PNG、WebP 图片"
                                density="compact"
                                multiple={false}
                                previewSelectedImage
                                selectedImageFile={field.state.value}
                                aria-invalid={
                                    field.state.meta.errors.length > 0
                                }
                                aria-describedby={`${id}-receipt-help ${id}-receipt-error`}
                                accept="image/jpeg,image/png,image/webp,.jpg,.jpeg,.png,.webp"
                                disabled={locked}
                                onFilesSelected={(files) => {
                                    field.handleChange(files[0] ?? null)
                                    field.handleBlur()
                                }}
                                onPreviewRemove={() => {
                                    field.handleChange(null)
                                    field.handleBlur()
                                }}
                            />
                            <p
                                id={`${id}-receipt-help`}
                                className="text-xs text-muted-foreground"
                            >
                                请核对实际打款金额与回单，提交时一并上传。
                            </p>
                            <FieldError
                                id={`${id}-receipt-error`}
                                errors={field.state.meta.errors}
                            />
                        </Field>
                    )}
                </form.Field>
                <form.Subscribe selector={(state) => state.values.amount}>
                    {(value) => {
                        const amount =
                            unknown && pending
                                ? pending.payload.payment.amount
                                : positivePaymentAmount(value)
                                  ? value
                                  : "0"
                        const unpaid =
                            unknown && pending
                                ? pending.payload.unpaidAmount
                                : request.unpaid_amount
                        const exceedsUnpaid =
                            compareDecimal(amount, unpaid, 2) > 0
                        const remaining = !exceedsUnpaid
                            ? clampZeroFixed(
                                  subtractFixed(unpaid, amount, money),
                                  money,
                              )
                            : undefined
                        const summary = (
                            <dl className="flex min-w-0 flex-wrap items-baseline gap-x-4 gap-y-1 text-xs">
                                <div className="flex items-baseline gap-1.5">
                                    <dt className="text-muted-foreground">
                                        本次登记
                                    </dt>
                                    <dd>
                                        <MoneyValue
                                            value={amount}
                                            display="exact"
                                        />
                                    </dd>
                                </div>
                                <div className="flex items-baseline gap-1.5">
                                    <dt className="text-muted-foreground">
                                        登记后待付
                                    </dt>
                                    <dd>
                                        {exceedsUnpaid ? (
                                            <span className="font-medium text-destructive">
                                                超出剩余待付
                                            </span>
                                        ) : (
                                            <MoneyValue
                                                value={remaining}
                                                display="exact"
                                            />
                                        )}
                                    </dd>
                                </div>
                            </dl>
                        )
                        const action = (
                            <Button
                                id={`${id}-submit`}
                                type={unknown ? "button" : "submit"}
                                form={unknown ? undefined : formId}
                                size="sm"
                                disabled={unknown ? payment.isPending : locked}
                                onClick={
                                    unknown
                                        ? () => void confirmPayment()
                                        : undefined
                                }
                            >
                                {unknown
                                    ? payment.isPending
                                        ? "正在核对结果…"
                                        : "核对提交结果"
                                    : payment.isPending
                                      ? "正在提交…"
                                      : "登记付款"}
                            </Button>
                        )
                        return (
                            <WorkspaceTaskFooter
                                summary={summary}
                                fallback={
                                    <div className="flex flex-wrap items-center justify-between gap-2 border-t border-border/60 pt-2">
                                        {summary}
                                        {action}
                                    </div>
                                }
                            >
                                {action}
                            </WorkspaceTaskFooter>
                        )
                    }}
                </form.Subscribe>
                <form.Subscribe selector={(state) => state.errors}>
                    {(errors) =>
                        errors.length ? (
                            <p
                                role="alert"
                                className="text-body-compact text-destructive"
                            >
                                请核对实际付款金额、付款时间及银行回单。
                            </p>
                        ) : null
                    }
                </form.Subscribe>
            </form>
            {result && (
                <p role="status" className="text-body-compact text-success">
                    {result}
                </p>
            )}
            {payment.isError && !unknown && !conflict && !rateLimited && (
                <p role="alert" className="text-body-compact text-destructive">
                    {getErrorMessage(
                        payment.error,
                        "付款登记失败，请核对后重试",
                    )}
                </p>
            )}
            {pending && !unknown && (
                <SupplierPaymentSubmitConfirmDialog
                    idPrefix={`${id}-confirm`}
                    open
                    onOpenChange={closeConfirmation}
                    pending={payment.isPending}
                    paymentAmount={pending.payload.payment.amount}
                    recipient={{
                        bankAccountId:
                            pending.payload.recipient.bank_account_id,
                        version: pending.payload.recipient.version,
                        accountName: pending.payload.recipient.account_name,
                        bankName: pending.payload.recipient.bank_name,
                        bankBranchName:
                            pending.payload.recipient.bank_branch_name ??
                            undefined,
                        accountNumberMasked:
                            pending.payload.recipient.account_number_masked,
                    }}
                    onConfirm={confirmPayment}
                />
            )}
        </section>
    )
}

export function RequestPaymentForm(props: {
    request: PaymentRequest
    task: PaymentTaskContext
    onTaskCompleted?: () => void
}) {
    if (props.request.execution_method.type === "supplier_deposit") {
        return (
            <RequestDepositExecution
                request={props.request}
                task={props.task}
            />
        )
    }
    if (!props.request.payment_recipient)
        return (
            <p role="alert" className="text-body-compact text-destructive">
                原银行付款申请缺少冻结的收款资料，请先核对原申请。
            </p>
        )
    return (
        <BankRequestPaymentForm
            {...props}
            request={{
                ...props.request,
                payment_recipient: props.request.payment_recipient,
            }}
        />
    )
}
