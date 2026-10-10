"use client"

import { useRef, useState } from "react"
import { z } from "zod"
import { useAppForm } from "@/components/form"
import { MoneyValue, SensitiveValue } from "@/components/business"
import { Button } from "@/components/ui/button"
import { Field, FieldLabel } from "@/components/ui/field"
import { Input } from "@/components/ui/input"
import { SupplierPaymentSubmitConfirmDialog } from "@/features/supplier-payables/components/supplier-payment-submit-confirm-dialog"
import { useAccountProfileQuery } from "@/features/auth/queries"
import { hasPermission } from "@/lib/permissions"
import { getErrorMessage } from "@/lib/api"
import { toAutomationIdSegment } from "@/lib/automation-id"
import {
    compareDecimal,
    minFixed,
    subtractFixed,
    sumFixed,
} from "@/lib/fixed-decimal"
import { classifyFormalCommandError } from "@/lib/formal-command"
import {
    useAccountRevealMutation,
    useRequestPaymentMutation,
} from "../hooks/queries"
import type { PaymentInput, PaymentRequest, PaymentTaskContext } from "../types"

function validPositive(value: string) {
    try {
        return compareDecimal(value, "0", 2) > 0
    } catch {
        return false
    }
}
function total(values: Record<string, string>) {
    return sumFixed(Object.values(values).filter(validPositive), {
        maxScale: 2,
        outputScale: 2,
    })
}
function allocationsFor(
    request: PaymentRequest,
    values: Record<string, string>,
): PaymentInput["allocations"] {
    return request.lines.flatMap((line) => {
        let remaining = values[line.payable_account_id] || "0"
        if (!validPositive(remaining)) return []
        const allocations: PaymentInput["allocations"] = []
        for (const entry of line.payable_entries) {
            const amount = minFixed(remaining, entry.open_total, {
                maxScale: 2,
                outputScale: 2,
            })
            if (!validPositive(amount)) continue
            allocations.push({
                payable_entry_id: entry.id,
                allocated_amount: amount,
            })
            remaining = subtractFixed(remaining, amount, {
                maxScale: 2,
                outputScale: 2,
            })
        }
        if (compareDecimal(remaining, "0", 2) > 0)
            throw new Error("采购当前可付款金额不足，请刷新后重新核对。")
        return allocations
    })
}

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
    const pendingCompletesRequest = useRef(false)
    const account = request.payment_recipient
    const schema = z
        .object({
            amounts: z.record(z.string(), z.string()),
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
            let count = 0
            for (const line of request.lines) {
                const amount = value.amounts[line.payable_account_id]?.trim()
                if (!amount) continue
                count += 1
                if (!validPositive(amount))
                    context.addIssue({
                        code: "custom",
                        path: ["amounts", line.payable_account_id],
                        message: "请填写大于零且最多两位小数的金额",
                    })
                else if (compareDecimal(amount, line.unpaid_amount, 2) > 0)
                    context.addIssue({
                        code: "custom",
                        path: ["amounts", line.payable_account_id],
                        message: "不能超过本行未付批准金额",
                    })
                else if (
                    line.source_changed ||
                    compareDecimal(
                        amount,
                        sumFixed(
                            line.payable_entries
                                .map((entry) => entry.open_total)
                                .filter(validPositive),
                            { maxScale: 2, outputScale: 2 },
                        ),
                        2,
                    ) > 0
                )
                    context.addIssue({
                        code: "custom",
                        path: ["amounts", line.payable_account_id],
                        message: "当前采购来源或可付款金额已变化，请刷新后核对",
                    })
            }
            if (count === 0)
                context.addIssue({
                    code: "custom",
                    path: ["amounts"],
                    message: "请填写至少一张采购单的实际付款金额",
                })
        })
    const form = useAppForm({
        defaultValues: {
            amounts: Object.fromEntries(
                request.lines.map((line) => [line.payable_account_id, ""]),
            ) as Record<string, string>,
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
            if (!value.receipt || unknown) return
            pendingCompletesRequest.current =
                compareDecimal(
                    total(value.amounts),
                    request.unpaid_amount,
                    2,
                ) === 0
            pending.current = {
                requestId: request.id,
                expectedVersion: request.version,
                supplierId: request.supplier_id,
                workItemId: task.workItemId,
                expectedTaskVersion: task.taskVersion,
                amount: total(value.amounts),
                paidAt: value.paidAt,
                bankReference: value.reference.trim(),
                receipt: value.receipt,
                lines: request.lines
                    .filter((line) =>
                        validPositive(
                            value.amounts[line.payable_account_id] ?? "",
                        ),
                    )
                    .map((line) => ({
                        payable_account_id: line.payable_account_id,
                        amount: value.amounts[line.payable_account_id],
                    })),
                allocations: allocationsFor(request, value.amounts),
                idempotencyKey: `ppr-pay:${crypto.randomUUID()}`,
            }
            setConfirmOpen(true)
        },
    })
    async function confirmPayment() {
        if (!pending.current) return
        try {
            const completesRequest = pendingCompletesRequest.current
            const applied = await payment.mutateAsync({
                payment: pending.current,
                recipient: account,
            })
            pending.current = null
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
        <section aria-label="当前付款任务" className="space-y-5 border-t pt-5">
            <div>
                <h3 className="text-lg font-semibold">
                    向{request.supplier_name}付款
                </h3>
                <p className="mt-1 text-xs text-muted-foreground">
                    仅在本申请逐单批准额度内登记实际付款。分次付款后保留剩余待付金额。
                </p>
            </div>
            <dl className="grid gap-4 text-sm sm:grid-cols-3">
                <div>
                    <dt className="text-xs text-muted-foreground">收款户名</dt>
                    <dd className="mt-1 font-medium">{account.account_name}</dd>
                </div>
                <div>
                    <dt className="text-xs text-muted-foreground">开户行</dt>
                    <dd className="mt-1">{account.bank_name}</dd>
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
                className="space-y-5"
                onSubmit={(event) => {
                    event.preventDefault()
                    void form.handleSubmit()
                }}
            >
                {request.lines.map((line) => (
                    <div
                        key={line.payable_account_id}
                        className="grid min-w-0 gap-3 border-b pb-4 sm:grid-cols-[minmax(0,1fr)_12rem]"
                    >
                        <div>
                            <p className="num font-medium">
                                {line.purchase_order_no}
                            </p>
                            <p className="mt-1 text-xs text-muted-foreground">
                                本申请未付批准金额{" "}
                                <MoneyValue value={line.unpaid_amount} /> ·
                                当前采购剩余应付{" "}
                                <MoneyValue
                                    value={
                                        line.current_open_amount ??
                                        line.open_amount
                                    }
                                />
                            </p>
                        </div>
                        <form.AppField
                            name={`amounts.${line.payable_account_id}`}
                        >
                            {(field) => (
                                <field.TextField
                                    id={`ppr-pay-line-${toAutomationIdSegment(line.payable_account_id)}`}
                                    label="本次实际付款"
                                    inputMode="decimal"
                                    placeholder="填写付款金额"
                                    disabled={
                                        unknown ||
                                        payment.isPending ||
                                        compareDecimal(
                                            line.unpaid_amount,
                                            "0",
                                            2,
                                        ) <= 0
                                    }
                                />
                            )}
                        </form.AppField>
                    </div>
                ))}
                <div className="grid gap-4 sm:grid-cols-2">
                    <form.AppField name="paidAt">
                        {(field) => (
                            <field.TextField
                                id="ppr-pay-paid-at"
                                type="datetime-local"
                                label="实际付款时间"
                                required
                                disabled={unknown || payment.isPending}
                            />
                        )}
                    </form.AppField>
                    <form.AppField name="reference">
                        {(field) => (
                            <field.TextField
                                id="ppr-pay-bank-reference"
                                label="银行流水号"
                                disabled={unknown || payment.isPending}
                            />
                        )}
                    </form.AppField>
                </div>
                <form.Field name="receipt">
                    {(field) => (
                        <Field>
                            <FieldLabel htmlFor="ppr-pay-bank-receipt">
                                银行回单
                            </FieldLabel>
                            <Input
                                id="ppr-pay-bank-receipt"
                                type="file"
                                accept="image/jpeg,image/png,image/webp"
                                disabled={unknown || payment.isPending}
                                onChange={(event) =>
                                    field.handleChange(
                                        event.target.files?.[0] ?? null,
                                    )
                                }
                            />
                            <p className="text-xs text-muted-foreground">
                                请核对实际打款金额与回单，支持 JPG、PNG 或 WebP
                                图片。
                            </p>
                        </Field>
                    )}
                </form.Field>
                <form.Subscribe selector={(state) => state.values.amounts}>
                    {(amounts) => (
                        <div className="flex flex-wrap items-center justify-between gap-4">
                            <p className="text-sm">
                                本次付款合计{" "}
                                <MoneyValue value={total(amounts)} />
                            </p>
                            <Button
                                id="ppr-pay-submit"
                                type="submit"
                                disabled={unknown || payment.isPending}
                            >
                                登记付款
                            </Button>
                        </div>
                    )}
                </form.Subscribe>
                <form.Subscribe selector={(state) => state.errors}>
                    {(errors) =>
                        errors.length ? (
                            <p
                                role="alert"
                                className="text-sm text-destructive"
                            >
                                请核对逐单付款金额、付款时间及银行回单。
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
                    bankAccountId: account.bank_account_id,
                    version: account.version,
                    accountName: account.account_name,
                    bankName: account.bank_name,
                    accountNumberMasked: account.account_number_masked,
                }}
                onConfirm={confirmPayment}
            />
        </section>
    )
}
