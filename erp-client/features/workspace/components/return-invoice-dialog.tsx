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
import { compareDecimal } from "@/lib/fixed-decimal"
import { useReverseInvoiceMutation } from "@/features/supplier-payables/hooks/queries"
import type {
    PurchaseInvoiceRow,
    FormalSubmitResult,
    ReverseInvoiceInput,
} from "@/features/supplier-payables/types"
export function ReturnInvoiceDialog({
    invoice,
    payableAccountId,
    amount,
    onClose,
    onResult,
}: {
    invoice: PurchaseInvoiceRow
    payableAccountId: string
    amount: string
    onClose: () => void
    onResult: (result: FormalSubmitResult) => void
}) {
    const mutation = useReverseInvoiceMutation()
    const submitted = useRef<ReverseInvoiceInput | null>(null)
    const [unknown, setUnknown] = useState(false)
    const [error, setError] = useState("")
    const form = useAppForm({
        defaultValues: {
            number: "",
            amount,
            reason: "采购退货，冲减本采购进项票额",
        },
        validators: {
            onChange: z.object({
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
                }, "金额须大于零且不超过待冲减票额"),
                reason: z.string().trim().min(1, "请填写原因"),
            }),
        },
        onSubmit: async ({ value }) => {
            const input = submitted.current ?? {
                invoiceId: invoice.invoiceId,
                payableAccountId,
                redInvoiceNo: value.number.trim(),
                amount: value.amount,
                reason: value.reason.trim(),
                idempotencyKey: crypto.randomUUID(),
            }
            submitted.current = input
            const result = await mutation.mutateAsync(input)
            onResult(result)
            if (result.status === "succeeded") {
                onClose()
                return
            }
            const uncertain = result.status === "unknown"
            setUnknown(uncertain)
            if (!uncertain) submitted.current = null
            setError(result.description)
        },
    })
    return (
        <Dialog
            open
            onOpenChange={(open) => {
                if (!open && !unknown && !mutation.isPending) onClose()
            }}
        >
            <DialogContent closeButtonId="return-invoice-close">
                <DialogHeader>
                    <DialogTitle>登记进项红票</DialogTitle>
                    <DialogDescription>
                        原蓝票 {invoice.invoiceNo}{" "}
                        保留，本次仅冲减当前采购。请按实际红票填写。
                    </DialogDescription>
                </DialogHeader>
                <form
                    className="space-y-4"
                    onSubmit={(event) => {
                        event.preventDefault()
                        void form.handleSubmit()
                    }}
                >
                    <form.AppField
                        name="number"
                        children={(field) => (
                            <field.TextField
                                id="return-invoice-number"
                                label="红票号码"
                                required
                                disabled={unknown || mutation.isPending}
                            />
                        )}
                    />
                    <form.AppField
                        name="amount"
                        children={(field) => (
                            <field.TextField
                                money
                                id="return-invoice-amount"
                                label="本次红冲含税金额"
                                inputMode="decimal"
                                required
                                disabled={unknown || mutation.isPending}
                            />
                        )}
                    />
                    <form.AppField
                        name="reason"
                        children={(field) => (
                            <field.TextareaField
                                id="return-invoice-reason"
                                label="原因"
                                required
                                disabled={unknown || mutation.isPending}
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
                            id="return-invoice-cancel"
                            type="button"
                            variant="outline"
                            disabled={unknown || mutation.isPending}
                            onClick={onClose}
                        >
                            取消
                        </Button>
                        <form.AppForm>
                            <form.SubmitButton
                                id="return-invoice-submit"
                                label={unknown ? "核对登记结果" : "登记红票"}
                                loading={mutation.isPending}
                            />
                        </form.AppForm>
                    </DialogFooter>
                </form>
            </DialogContent>
        </Dialog>
    )
}
