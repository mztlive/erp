"use client"

import { useRef, useState } from "react"
import { z } from "zod"
import { ReasonTextareaField } from "@/components/business/reason-textarea-field"
import { MoneyValue } from "@/components/business/values"
import { toFieldErrors, useAppForm } from "@/components/form"
import { Button } from "@/components/ui/button"
import {
    Dialog,
    DialogBody,
    DialogContent,
    DialogDescription,
    DialogFooter,
    DialogHeader,
    DialogTitle,
} from "@/components/ui/dialog"
import { useReverseInvoiceMutation } from "@/features/supplier-payables/hooks/queries"
import type {
    FormalSubmitResult,
    PurchaseInvoiceRow,
    ReverseInvoiceInput,
} from "@/features/supplier-payables/types"
import { getErrorMessage } from "@/lib/api"
import { compareDecimal, normalizeFixed } from "@/lib/fixed-decimal"
import {
    FormalCommandKeyLedger,
    isCommandOutcomeUnknown,
    type FormalCommandIdentity,
} from "@/lib/formal-command"
import { bpmReasonSchema } from "@/lib/validation/reason-length"

type ReverseInvoicePayload = Omit<ReverseInvoiceInput, "idempotencyKey">

export function ReturnInvoiceDialog({
    idPrefix,
    invoice,
    payableAccountId,
    amount,
    refreshing,
    onRefresh,
    onClose,
    onResult,
    onSucceeded,
}: {
    idPrefix: string
    invoice: PurchaseInvoiceRow
    payableAccountId: string
    amount: string
    refreshing: boolean
    onRefresh: () => void
    onClose: () => void
    onResult: (result: FormalSubmitResult) => void
    onSucceeded: () => Promise<void>
}) {
    const mutation = useReverseInvoiceMutation()
    const [ledger] = useState(() => new FormalCommandKeyLedger())
    const commandSlot = `${invoice.invoiceId}:${payableAccountId}`
    const submitting = useRef(false)
    const [feedback, setFeedback] = useState<FormalSubmitResult | null>(null)
    const unknown = feedback?.status === "unknown"
    const locked = unknown || mutation.isPending
    const canFill = compareDecimal(amount, "0", 2) > 0
    const schema = z.object({
        number: z.string().trim().min(1, "请填写实际红票号码"),
        amount: z.string().refine((value) => {
            try {
                return (
                    compareDecimal(value, "0", 2) > 0 &&
                    compareDecimal(value, amount, 2) <= 0
                )
            } catch {
                return false
            }
        }, "金额须大于零且不超过可冲上限，最多两位小数"),
        reason: bpmReasonSchema({ required: true, label: "红冲原因" }),
    })

    async function submit(
        identity: FormalCommandIdentity<ReverseInvoicePayload>,
    ) {
        if (submitting.current) return
        submitting.current = true
        try {
            let result: FormalSubmitResult
            try {
                result = await mutation.mutateAsync({
                    ...identity.payload,
                    idempotencyKey: identity.idempotencyKey,
                })
            } catch (cause) {
                const isUnknown = isCommandOutcomeUnknown(cause)
                result = {
                    status: isUnknown ? "unknown" : "failed",
                    title: isUnknown ? "红票登记结果待确认" : "红票登记未完成",
                    description: getErrorMessage(
                        cause,
                        isUnknown
                            ? "未收到登记结果，请核对原提交。"
                            : "提交失败，请核对后重试。",
                    ),
                }
            }

            if (
                result.status === "unknown" ||
                result.errorCode === "OUTCOME_UNKNOWN"
            ) {
                result = {
                    ...result,
                    status: "unknown",
                    title: "红票登记结果待确认",
                }
            }
            ledger.settle(
                commandSlot,
                result.status === "unknown"
                    ? "unknown"
                    : result.status === "succeeded"
                      ? "succeeded"
                      : "failed",
            )
            setFeedback(result)
            onResult(result)

            if (result.status === "succeeded") {
                onClose()
                await onSucceeded()
            }
        } finally {
            submitting.current = false
        }
    }

    const form = useAppForm({
        defaultValues: {
            number: "",
            amount,
            reason: "采购退货，冲减本采购进项票额",
        },
        validators: { onChange: schema, onSubmit: schema },
        onSubmit: async ({ value }) => {
            if (refreshing || locked || ledger.peek(commandSlot)) return
            let identity: FormalCommandIdentity<ReverseInvoicePayload>
            try {
                identity = ledger.acquire<ReverseInvoicePayload>(
                    commandSlot,
                    "purchase-return-red-invoice",
                    {
                        invoiceId: invoice.invoiceId,
                        payableAccountId,
                        redInvoiceNo: value.number.trim(),
                        amount: normalizeFixed(value.amount, {
                            maxScale: 2,
                            outputScale: 2,
                        }),
                        reason: value.reason.trim(),
                    },
                )
            } catch (cause) {
                const result: FormalSubmitResult = {
                    status: "failed",
                    title: "红票登记未完成",
                    description: getErrorMessage(
                        cause,
                        "当前浏览器环境受限，请刷新后重试。",
                    ),
                }
                setFeedback(result)
                onResult(result)
                return
            }
            await submit(identity)
        },
    })

    function close() {
        if (!locked && !submitting.current) onClose()
    }

    return (
        <Dialog
            open
            onOpenChange={(open) => {
                if (!open) close()
            }}
        >
            <DialogContent
                size="md"
                scroll="body"
                className="min-w-0 bg-card"
                closeButtonId={`${idPrefix}-close`}
                showCloseButton={!locked}
            >
                <DialogHeader>
                    <DialogTitle>登记进项红票</DialogTitle>
                    <DialogDescription className="wrap-anywhere">
                        原蓝票{" "}
                        <span className="num select-all">
                            {invoice.invoiceNo}
                        </span>
                        。请按实际收到的红票填写本采购的冲减金额。
                    </DialogDescription>
                </DialogHeader>
                <form
                    className="flex min-h-0 min-w-0 flex-1 flex-col overflow-hidden"
                    onSubmit={(event) => {
                        event.preventDefault()
                        void form.handleSubmit()
                    }}
                >
                    <DialogBody className="min-w-0 space-y-4 overflow-x-hidden">
                        {feedback && (
                            <div
                                role={unknown ? "status" : "alert"}
                                className={
                                    unknown
                                        ? "space-y-1 rounded-r-md border-l-2 border-l-warning bg-warning-soft px-3 py-2 text-xs text-warning-soft-foreground"
                                        : "space-y-1 rounded-r-md border-l-2 border-l-destructive bg-destructive-soft px-3 py-2 text-xs text-destructive-soft-foreground"
                                }
                            >
                                <p className="font-medium">{feedback.title}</p>
                                <p className="wrap-anywhere">
                                    {feedback.description}
                                </p>
                                {unknown ? (
                                    <p>
                                        请核对原提交，保持红票号码、金额和原因。
                                    </p>
                                ) : (
                                    <Button
                                        id={`${idPrefix}-refresh`}
                                        type="button"
                                        variant="ghost"
                                        size="xs"
                                        disabled={refreshing}
                                        onClick={onRefresh}
                                    >
                                        刷新最新数据
                                    </Button>
                                )}
                            </div>
                        )}
                        <form.AppField name="number">
                            {(field) => (
                                <field.TextField
                                    id={`${idPrefix}-number`}
                                    label="红票号码"
                                    required
                                    disabled={locked}
                                    inputClassName="num border-0 bg-surface-sunken text-body-compact"
                                />
                            )}
                        </form.AppField>
                        <div className="min-w-0 space-y-1.5">
                            <form.AppField name="amount">
                                {(field) => (
                                    <>
                                        <field.TextField
                                            money
                                            id={`${idPrefix}-amount`}
                                            label="本次红冲含税金额"
                                            inputMode="decimal"
                                            required
                                            disabled={locked}
                                            aria-describedby={`${idPrefix}-limit`}
                                            inputClassName="num border-0 bg-surface-sunken text-body-compact"
                                        />
                                        <div className="flex flex-wrap items-center justify-between gap-2">
                                            <p
                                                id={`${idPrefix}-limit`}
                                                className="text-xs text-muted-foreground"
                                            >
                                                可冲上限{" "}
                                                <MoneyValue value={amount} />
                                            </p>
                                            <Button
                                                id={`${idPrefix}-fill-max`}
                                                type="button"
                                                variant="ghost"
                                                size="xs"
                                                disabled={
                                                    locked ||
                                                    refreshing ||
                                                    !canFill
                                                }
                                                onClick={() =>
                                                    field.handleChange(amount)
                                                }
                                            >
                                                填最大可冲额
                                            </Button>
                                        </div>
                                    </>
                                )}
                            </form.AppField>
                        </div>
                        <form.AppField name="reason">
                            {(field) => (
                                <ReasonTextareaField
                                    id={`${idPrefix}-reason`}
                                    name={field.name}
                                    mode="utf8-bytes"
                                    label="红冲原因"
                                    required
                                    value={field.state.value}
                                    error={
                                        field.state.meta.isTouched
                                            ? toFieldErrors(
                                                  field.state.meta.errors,
                                              )[0]?.message
                                            : undefined
                                    }
                                    disabled={locked}
                                    onChange={field.handleChange}
                                    onBlur={field.handleBlur}
                                    textareaClassName="min-w-0 border-0 bg-surface-sunken"
                                />
                            )}
                        </form.AppField>
                    </DialogBody>
                    <DialogFooter>
                        <Button
                            id={`${idPrefix}-cancel`}
                            type="button"
                            variant="outline"
                            disabled={locked}
                            onClick={close}
                        >
                            取消
                        </Button>
                        {unknown ? (
                            <Button
                                id={`${idPrefix}-recover`}
                                type="button"
                                variant="default"
                                disabled={mutation.isPending}
                                onClick={() => {
                                    const identity =
                                        ledger.peek<ReverseInvoicePayload>(
                                            commandSlot,
                                        )
                                    if (identity) void submit(identity)
                                }}
                            >
                                {mutation.isPending
                                    ? "正在核对…"
                                    : "核对提交结果"}
                            </Button>
                        ) : (
                            <form.AppForm>
                                <form.SubmitButton
                                    id={`${idPrefix}-submit`}
                                    label="登记红票"
                                    loading={mutation.isPending}
                                    disabled={refreshing || !canFill}
                                />
                            </form.AppForm>
                        )}
                    </DialogFooter>
                </form>
            </DialogContent>
        </Dialog>
    )
}
