"use client"
import { useRef, useState } from "react"
import { z } from "zod"
import { useAppForm } from "@/components/form"
import { Button } from "@/components/ui/button"
import {
    Dialog,
    DialogContent,
    DialogDescription,
    DialogHeader,
    DialogTitle,
    DialogFooter,
} from "@/components/ui/dialog"
import { useConfirmDirectReturnMutation } from "../hooks/queries"
import type {
    DirectReturnRequest,
    DirectReturnPlan,
} from "../direct-return-types"
import { compactFixed, compareDecimal, parseDecimal } from "@/lib/fixed-decimal"
import { getErrorMessage } from "@/lib/api/errors"
import { commandFailureDisposition } from "@/lib/api/command-recovery"
import { toAutomationIdSegment } from "@/lib/automation-id"

export function PurchaseDirectReturnDialog({
    purchaseOrderId,
    version,
    plan,
    onClose,
}: {
    purchaseOrderId: string
    version: number
    plan: DirectReturnPlan
    onClose: () => void
}) {
    const mutation = useConfirmDirectReturnMutation()
    const [error, setError] = useState<string | null>(null)
    const [unknown, setUnknown] = useState(false)
    const submitted = useRef<DirectReturnRequest | null>(null)
    const busy = useRef(false)
    const [source] = useState(() => ({ plan, version }))
    const eligible = source.plan.lines.filter(
        (line) => compareDecimal(line.max_direct_return, "0", 6) > 0,
    )
    const form = useAppForm({
        defaultValues: {
            reason: "销售减量，客户直退供应商",
            receipt: "",
            quantities: eligible.map((line) => ({
                id: line.purchase_line_id,
                quantity: "0",
            })),
        },
        validators: {
            onChange: z.object({
                reason: z.string().trim().min(1, "请填写退货原因").max(500),
                receipt: z
                    .string()
                    .trim()
                    .min(1, "请填写供应商收货凭据或核实说明")
                    .max(500),
                quantities: z
                    .array(z.object({ id: z.string(), quantity: z.string() }))
                    .refine((lines) => {
                        try {
                            return (
                                lines.some(
                                    (line) =>
                                        compareDecimal(line.quantity, "0", 6) >
                                        0,
                                ) &&
                                lines.every((line) => {
                                    parseDecimal(line.quantity, { maxScale: 6 })
                                    const option = eligible.find(
                                        (item) =>
                                            item.purchase_line_id === line.id,
                                    )
                                    return (
                                        !!option &&
                                        compareDecimal(line.quantity, "0", 6) >=
                                            0 &&
                                        compareDecimal(
                                            line.quantity,
                                            option.max_direct_return,
                                            6,
                                        ) <= 0
                                    )
                                })
                            )
                        } catch {
                            return false
                        }
                    }, "请填写至少一项实际退回数量，不得超过可直退量；未退回的项填 0"),
            }),
        },
        onSubmit: async ({ value }) => {
            if (busy.current) return
            busy.current = true
            setError(null)
            const request = submitted.current ?? {
                expected_version: source.version,
                idempotency_key: crypto.randomUUID(),
                reason: value.reason.trim(),
                receipt_reference: value.receipt.trim(),
                lines: value.quantities
                    .filter((line) => compareDecimal(line.quantity, "0", 6) > 0)
                    .map((line) => ({
                        purchase_line_id: line.id,
                        quantity: line.quantity,
                    })),
            }
            submitted.current = request
            try {
                await mutation.mutateAsync({ purchaseOrderId, request })
                onClose()
            } catch (cause) {
                const uncertain = commandFailureDisposition(cause) === "unknown"
                setUnknown(uncertain)
                if (!uncertain) submitted.current = null
                setError(getErrorMessage(cause, "登记未完成，请核对后重试。"))
            } finally {
                busy.current = false
            }
        },
    })
    const pending = mutation.isPending
    return (
        <Dialog
            open
            onOpenChange={(open) => {
                if (!open && !pending && !unknown) onClose()
            }}
        >
            <DialogContent
                closeButtonId="purchase-direct-return-close"
                showCloseButton={!pending && !unknown}
                className="max-h-[85dvh] overflow-y-auto sm:max-w-2xl"
            >
                <DialogHeader>
                    <DialogTitle>登记客户直退供应商</DialogTitle>
                    <DialogDescription>
                        先与客户、供应商核实实物已退回并由供应商接收，再填写本次实际数量。可分批登记；未退回的商品填
                        0。
                    </DialogDescription>
                </DialogHeader>
                <form
                    className="space-y-5"
                    onSubmit={(event) => {
                        event.preventDefault()
                        void form.handleSubmit()
                    }}
                >
                    <div className="divide-y">
                        {eligible.map((line, index) => (
                            <div
                                key={line.purchase_line_id}
                                className="grid gap-3 py-4 sm:grid-cols-[1fr_10rem] sm:items-start"
                            >
                                <div>
                                    <p className="text-sm font-medium">
                                        {line.item_name}
                                    </p>
                                    <p className="mt-1 text-xs text-muted-foreground">
                                        已登记退回{" "}
                                        {compactFixed(line.returned_quantity)} ·
                                        还需退回{" "}
                                        {compactFixed(line.required_return)}{" "}
                                        {line.unit}
                                    </p>
                                </div>
                                <form.AppField
                                    name={`quantities[${index}].quantity`}
                                    children={(field) => (
                                        <field.TextField
                                            id={`purchase-direct-return-quantity-${toAutomationIdSegment(line.purchase_line_id)}`}
                                            label="本次实际退回"
                                            required
                                            inputMode="decimal"
                                            disabled={pending || unknown}
                                            description={`最多 ${compactFixed(line.max_direct_return)} ${line.unit}`}
                                        />
                                    )}
                                />
                            </div>
                        ))}
                    </div>
                    <form.AppField
                        name="receipt"
                        children={(field) => (
                            <field.TextareaField
                                id="purchase-direct-return-receipt"
                                label="供应商收货凭据或核实说明"
                                required
                                disabled={pending || unknown}
                                placeholder="例如：物流单号、供应商签收日期、确认人和接收结果"
                            />
                        )}
                    />
                    <form.AppField
                        name="reason"
                        children={(field) => (
                            <field.TextareaField
                                id="purchase-direct-return-reason"
                                label="退货原因"
                                required
                                disabled={pending || unknown}
                            />
                        )}
                    />
                    <form.Subscribe
                        selector={(state) => state.errors}
                        children={(errors) =>
                            errors.length ? (
                                <p
                                    role="alert"
                                    className="text-sm text-destructive"
                                >
                                    请核对实际退回数量、原因及收货凭据后提交。
                                </p>
                            ) : null
                        }
                    />
                    {error ? (
                        <p role="alert" className="text-sm text-destructive">
                            {error}
                        </p>
                    ) : null}
                    {unknown ? (
                        <p className="text-sm text-muted-foreground">
                            登记结果尚未确认，请保持内容不变，点击“核对登记结果”。
                        </p>
                    ) : null}
                    <DialogFooter>
                        <Button
                            id="purchase-direct-return-cancel"
                            type="button"
                            variant="outline"
                            disabled={pending || unknown}
                            onClick={onClose}
                        >
                            取消
                        </Button>
                        <form.AppForm>
                            <form.SubmitButton
                                id="purchase-direct-return-submit"
                                loading={pending}
                                label={
                                    unknown
                                        ? "核对登记结果"
                                        : "确认供应商已收货并登记"
                                }
                            />
                        </form.AppForm>
                    </DialogFooter>
                </form>
            </DialogContent>
        </Dialog>
    )
}
