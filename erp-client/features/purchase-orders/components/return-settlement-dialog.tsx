"use client"
import { useRef, useState } from "react"
import { z } from "zod"
import { useAppForm } from "@/components/form"
import { Button } from "@/components/ui/button"
import {
    Dialog,
    DialogContent,
    DialogHeader,
    DialogTitle,
    DialogDescription,
    DialogFooter,
} from "@/components/ui/dialog"
import { MoneyValue } from "@/components/business/values"
import {
    compareDecimal,
    clampZeroFixed,
    subtractFixed,
} from "@/lib/fixed-decimal"
import { getErrorMessage } from "@/lib/api"
import { commandFailureDisposition } from "@/lib/api/command-recovery"
import { useSyncReturnFinanceMutation } from "../hooks/queries"
import type {
    DirectReturnPlan,
    ReturnSettlementDecision,
} from "../direct-return-types"

export function ReturnSettlementDialog({
    purchaseOrderId,
    plan,
    excess,
    onClose,
}: {
    purchaseOrderId: string
    plan: DirectReturnPlan
    excess: string
    onClose: () => void
}) {
    const mutation = useSyncReturnFinanceMutation()
    const [error, setError] = useState("")
    const [unknown, setUnknown] = useState(false)
    const submitted = useRef<ReturnSettlementDecision | null>(null)
    const total = clampZeroFixed(excess, { maxScale: 2, outputScale: 2 })
    const form = useAppForm({
        defaultValues: { method: "refund", refund: total, confirmation: "" },
        validators: {
            onChange: z
                .object({
                    method: z.string(),
                    refund: z.string(),
                    confirmation: z
                        .string()
                        .trim()
                        .min(1, "请填写供应商确认依据")
                        .max(500),
                })
                .refine((value) => {
                    if (value.method !== "split") return true
                    try {
                        return (
                            compareDecimal(value.refund, "0", 2) > 0 &&
                            compareDecimal(value.refund, total, 2) < 0
                        )
                    } catch {
                        return false
                    }
                }, "部分退款金额须大于零且小于待处理总额"),
        },
        onSubmit: async ({ value }) => {
            const refund =
                value.method === "credit"
                    ? "0.00"
                    : value.method === "split"
                      ? value.refund
                      : total
            const decision = submitted.current ?? {
                idempotency_key: crypto.randomUUID(),
                sales_revision_id: plan.sales_revision_id,
                target_gross: plan.target_gross,
                refund_amount: refund,
                credit_amount: subtractFixed(total, refund, {
                    maxScale: 2,
                    outputScale: 2,
                }),
                supplier_confirmation: value.confirmation.trim(),
            }
            submitted.current = decision
            setError("")
            try {
                await mutation.mutateAsync({ purchaseOrderId, decision })
                onClose()
            } catch (err) {
                const uncertain = commandFailureDisposition(err) === "unknown"
                setUnknown(uncertain)
                if (!uncertain) submitted.current = null
                setError(getErrorMessage(err, "交接失败，请核对后重试"))
            }
        },
    })
    return (
        <Dialog
            open
            onOpenChange={(open) => {
                if (!open && !mutation.isPending && !unknown) onClose()
            }}
        >
            <DialogContent closeButtonId="return-settlement-close">
                <DialogHeader>
                    <DialogTitle>确认退货款处理方式</DialogTitle>
                    <DialogDescription>
                        填写与供应商确认的结果，提交后交给财务办理。此操作不登记资金到账。
                    </DialogDescription>
                </DialogHeader>
                <p className="text-sm">
                    本次待处理款项 <MoneyValue value={total} />
                    {compareDecimal(total, "0", 2) === 0 &&
                        "，仅需交接进项红票处理。"}
                </p>
                <form
                    onSubmit={(event) => {
                        event.preventDefault()
                        void form.handleSubmit()
                    }}
                    className="space-y-4"
                >
                    {compareDecimal(total, "0", 2) > 0 && (
                        <form.AppField
                            name="method"
                            children={(field) => (
                                <field.SelectField
                                    id="return-settlement-method"
                                    label="处理方式"
                                    required
                                    allowClear={false}
                                    disabled={mutation.isPending || unknown}
                                    options={[
                                        {
                                            value: "refund",
                                            label: "供应商退款",
                                        },
                                        {
                                            value: "credit",
                                            label: "留抵下次采购",
                                        },
                                        {
                                            value: "split",
                                            label: "部分退款、部分留抵",
                                        },
                                    ]}
                                />
                            )}
                        />
                    )}
                    <form.Subscribe
                        selector={(state) => state.values.method}
                        children={(method) =>
                            method === "split" ? (
                                <form.AppField
                                    name="refund"
                                    children={(field) => (
                                        <field.TextField
                                            id="return-settlement-refund"
                                            label="供应商应退现金"
                                            inputMode="decimal"
                                            disabled={
                                                mutation.isPending || unknown
                                            }
                                            description="剩余金额转为留抵"
                                        />
                                    )}
                                />
                            ) : null
                        }
                    />
                    <form.AppField
                        name="confirmation"
                        children={(field) => (
                            <field.TextareaField
                                id="return-settlement-confirmation"
                                label="供应商确认依据"
                                required
                                disabled={mutation.isPending || unknown}
                                placeholder="填写确认人、确认日期，以及退款或留抵的约定"
                            />
                        )}
                    />
                    {error && (
                        <p role="alert" className="text-sm text-destructive">
                            {error}
                        </p>
                    )}
                    <DialogFooter>
                        <Button
                            id="return-settlement-cancel"
                            variant="outline"
                            type="button"
                            disabled={mutation.isPending || unknown}
                            onClick={onClose}
                        >
                            取消
                        </Button>
                        <form.AppForm>
                            <form.SubmitButton
                                id="return-settlement-submit"
                                loading={mutation.isPending}
                                label={
                                    unknown ? "核对交接结果" : "提交并交给财务"
                                }
                            />
                        </form.AppForm>
                    </DialogFooter>
                </form>
            </DialogContent>
        </Dialog>
    )
}
