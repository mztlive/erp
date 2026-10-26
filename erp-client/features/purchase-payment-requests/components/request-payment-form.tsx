"use client"

import { useRef, useState } from "react"
import { z } from "zod"
import { useAppForm } from "@/components/form"
import {
    MoneyValue,
    SensitiveValue,
    WorkspaceTaskFooter,
} from "@/components/business"
import { Button } from "@/components/ui/button"
import { Field, FieldError, FieldLabel } from "@/components/ui/field"
import { fileUploadText } from "@/lib/ui-text"
import { FileUpload } from "@/components/ui/file-upload"
import { SupplierPaymentSubmitConfirmDialog } from "@/features/supplier-payables/components/supplier-payment-submit-confirm-dialog"
import { useAccountProfileQuery } from "@/features/auth/queries"
import { hasPermission } from "@/lib/permissions"
import { getErrorMessage } from "@/lib/api"
import { compareDecimal } from "@/lib/fixed-decimal"
import {
    allocateRequestPayment,
    paymentBlockReason,
    positivePaymentAmount,
} from "../lib/payment"
import { classifyFormalCommandError } from "@/lib/formal-command"
import {
    useAccountRevealMutation,
    useRequestPaymentMutation,
} from "../hooks/queries"
import type {
    PaymentInput,
    PaymentRecipient,
    PaymentRequest,
    PaymentTaskContext,
} from "../types"

/** 每笔付款保持独立命令身份；响应未知时重试原金额、回单及分配。 */
export function RequestPaymentForm({
    request,
    task,
    onTaskCompleted,
}: {
    request: PaymentRequest
    task: PaymentTaskContext
    onTaskCompleted?: () => void
}) {
    const payment = useRequestPaymentMutation()
    const reveal = useAccountRevealMutation()
    const profile = useAccountProfileQuery()
    const [confirmOpen, setConfirmOpen] = useState(false)
    const [unknown, setUnknown] = useState(false)
    const [result, setResult] = useState<string>()
    const pending = useRef<PaymentInput | null>(null)
    const pendingRecipient = useRef<PaymentRecipient | null>(null)
    const pendingCompletesRequest = useRef(false)
    const account = request.payment_recipient
    const blocked = paymentBlockReason(request)
    const locked = unknown || payment.isPending || Boolean(blocked)
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
            pendingCompletesRequest.current =
                compareDecimal(value.amount, request.unpaid_amount, 2) === 0
            pending.current = {
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
                idempotencyKey: `ppr-pay:${crypto.randomUUID()}`,
            }
            pendingRecipient.current = { ...account }
            setConfirmOpen(true)
        },
    })
    async function confirmPayment() {
        if (!pending.current || !pendingRecipient.current) return
        try {
            const completesRequest = pendingCompletesRequest.current
            const applied = await payment.mutateAsync({
                payment: pending.current,
                recipient: pendingRecipient.current,
            })
            pending.current = null
            pendingRecipient.current = null
            setUnknown(false)
            setResult(`付款 ${applied.payment_no} 已登记并核销。`)
            form.reset()
            setConfirmOpen(false)
            if (completesRequest) onTaskCompleted?.()
        } catch (error) {
            const outcomeUnknown =
                classifyFormalCommandError(error) === "unknown"
            setUnknown(outcomeUnknown)
            throw error
        }
    }
    return (
        <section aria-label="当前付款任务" className="space-y-4">
            {blocked && !unknown && (
                <p
                    role="alert"
                    className="rounded-md bg-destructive/10 p-3 text-sm text-destructive"
                >
                    {blocked}
                </p>
            )}
            <h3 className="text-sm font-semibold">付款与收款信息</h3>
            <dl className="grid gap-x-8 gap-y-4 text-sm lg:grid-cols-2">
                <div>
                    <dt className="text-xs text-muted-foreground">
                        我方付款主体
                    </dt>
                    <dd className="mt-1 break-words font-medium">
                        {request.our_company_name}
                    </dd>
                </div>
                <div>
                    <dt className="text-xs text-muted-foreground">收款户名</dt>
                    <dd className="mt-1 break-words font-medium">
                        {account.account_name}
                    </dd>
                </div>
                <div>
                    <dt className="text-xs text-muted-foreground">开户行</dt>
                    <dd className="mt-1 break-words">{account.bank_name}</dd>
                </div>
                <div>
                    <dt className="text-xs text-muted-foreground">收款账号</dt>
                    <dd className="mt-1">
                        <SensitiveValue
                            id="ppr-pay-recipient-account"
                            label="收款账号"
                            maskedValue={account.account_number_masked}
                            onReveal={
                                hasPermission(
                                    profile.data?.permissions,
                                    "party_bank_account:reveal",
                                )
                                    ? () =>
                                          reveal.mutateAsync({ request, task })
                                    : undefined
                            }
                        />
                    </dd>
                </div>
            </dl>
            <form
                id="ppr-pay-form"
                className="space-y-4"
                onSubmit={(event) => {
                    event.preventDefault()
                    void form.handleSubmit()
                }}
            >
                <div className="space-y-4 border-t pt-5">
                    <h3 className="text-sm font-semibold">登记实际付款</h3>
                    <form.AppField name="amount">
                        {(field) => (
                            <field.TextField
                                money
                                id="ppr-pay-amount"
                                label="本次实际付款金额"
                                inputMode="decimal"
                                placeholder="按银行回单填写金额"
                                required
                                disabled={locked}
                            />
                        )}
                    </form.AppField>
                </div>
                <div className="grid gap-4 sm:grid-cols-2">
                    <form.AppField name="paidAt">
                        {(field) => (
                            <field.DateTimeField
                                id="ppr-pay-paid-at"
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
                                id="ppr-pay-bank-reference"
                                label="银行流水号"
                                disabled={locked}
                            />
                        )}
                    </form.AppField>
                </div>
                <form.Field name="receipt">
                    {(field) => (
                        <Field>
                            <FieldLabel htmlFor="ppr-pay-bank-receipt-input">
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
                                idPrefix="ppr-pay-bank-receipt"
                                label="上传银行回单"
                                description="点击选择或拖入 JPG、PNG、WebP 图片"
                                density="compact"
                                multiple={false}
                                previewSelectedImage
                                selectedImageFile={field.state.value}
                                aria-invalid={
                                    field.state.meta.errors.length > 0
                                }
                                aria-describedby="ppr-pay-receipt-help ppr-pay-receipt-error"
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
                                id="ppr-pay-receipt-help"
                                className="text-xs text-muted-foreground"
                            >
                                请核对实际打款金额与回单，提交时一并上传。
                            </p>
                            <FieldError
                                id="ppr-pay-receipt-error"
                                errors={field.state.meta.errors}
                            />
                        </Field>
                    )}
                </form.Field>
                <form.Subscribe selector={(state) => state.values.amount}>
                    {(amount) => (
                        <WorkspaceTaskFooter>
                            <div className="mr-auto text-sm">
                                本次登记{" "}
                                <MoneyValue
                                    value={
                                        positivePaymentAmount(amount)
                                            ? amount
                                            : "0"
                                    }
                                />
                            </div>
                            <Button
                                id="ppr-pay-submit"
                                type="submit"
                                form="ppr-pay-form"
                                disabled={locked}
                            >
                                登记付款
                            </Button>
                        </WorkspaceTaskFooter>
                    )}
                </form.Subscribe>
                <form.Subscribe selector={(state) => state.errors}>
                    {(errors) =>
                        errors.length ? (
                            <p
                                role="alert"
                                className="text-sm text-destructive"
                            >
                                请核对实际付款金额、付款时间及银行回单。
                            </p>
                        ) : null
                    }
                </form.Subscribe>
            </form>
            {result && (
                <p role="status" className="text-sm text-success">
                    {result}
                </p>
            )}
            {payment.isError && (
                <p role="alert" className="text-sm text-destructive">
                    {getErrorMessage(
                        payment.error,
                        "付款登记失败，请核对后重试",
                    )}
                </p>
            )}
            {unknown && (
                <div role="status" className="space-y-3 rounded-md border p-4">
                    <p className="text-sm">
                        付款结果暂无法确认。本次金额和回单已保留，请查询原次付款结果后继续。
                    </p>
                    <Button
                        id="ppr-pay-recover"
                        type="button"
                        variant="outline"
                        disabled={payment.isPending}
                        onClick={() =>
                            void confirmPayment().catch(() => undefined)
                        }
                    >
                        查询本次付款结果
                    </Button>
                </div>
            )}
            <SupplierPaymentSubmitConfirmDialog
                idPrefix="ppr-pay-confirm"
                open={confirmOpen}
                onOpenChange={setConfirmOpen}
                pending={payment.isPending}
                paymentAmount={pending.current?.amount ?? "0"}
                recipient={{
                    bankAccountId: (pendingRecipient.current ?? account)
                        .bank_account_id,
                    version: (pendingRecipient.current ?? account).version,
                    accountName: (pendingRecipient.current ?? account)
                        .account_name,
                    bankName: (pendingRecipient.current ?? account).bank_name,
                    accountNumberMasked: (pendingRecipient.current ?? account)
                        .account_number_masked,
                }}
                onConfirm={confirmPayment}
            />
        </section>
    )
}
