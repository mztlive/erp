"use client"
import { useRef, useState } from "react"
import Link from "next/link"
import { z } from "zod"
import { useAppForm } from "@/components/form"
import { MoneyValue } from "@/components/business/values"
import { Button } from "@/components/ui/button"
import {
    Dialog,
    DialogContent,
    DialogHeader,
    DialogTitle,
    DialogDescription,
    DialogFooter,
} from "@/components/ui/dialog"
import { compareDecimal, minFixed, subtractFixed } from "@/lib/fixed-decimal"
import { getErrorMessage } from "@/lib/api"
import { commandFailureDisposition } from "@/lib/api/command-recovery"
import { toAutomationIdSegment } from "@/lib/automation-id"
import {
    useSupplierCreditsQuery,
    useSupplierCreditMutation,
} from "../hooks/credit-queries"
import type { SupplierCredit, CreditUseInput } from "../credit-api"
import type { PaymentRequest, PaymentTaskContext } from "../types"

export function SupplierCreditUsePanel({
    request,
    task,
    onTaskCompleted,
}: {
    request: PaymentRequest
    task: PaymentTaskContext
    onTaskCompleted?: () => void
}) {
    const query = useSupplierCreditsQuery(request.id)
    const [selected, setSelected] = useState<SupplierCredit | null>(null)
    if (query.isPending)
        return (
            <p className="text-sm text-muted-foreground">正在核对可用留抵…</p>
        )
    if (query.isError)
        return (
            <div className="text-sm">
                <p role="alert">
                    {getErrorMessage(query.error, "留抵余额读取失败")}
                </p>
                <Button
                    id="ppr-credit-retry"
                    variant="outline"
                    onClick={() => void query.refetch()}
                >
                    重新读取留抵
                </Button>
            </div>
        )
    if (!query.data.length) return null
    return (
        <section
            className="space-y-3 rounded-lg border p-4"
            aria-label="可用供应商留抵"
        >
            <h3 className="text-sm font-medium">可用留抵</h3>
            {query.data.map((credit) => (
                <div
                    key={credit.id}
                    className="flex flex-wrap items-center justify-between gap-3 text-sm"
                >
                    <div>
                        <MoneyValue
                            value={subtractFixed(
                                credit.amount,
                                credit.used_amount,
                                { maxScale: 2, outputScale: 2 },
                            )}
                        />
                        <Link
                            id={`ppr-credit-source-${toAutomationIdSegment(credit.id)}`}
                            href={`/procurement/orders/${encodeURIComponent(credit.source_purchase_order_id)}`}
                            className="ml-3 text-muted-foreground underline underline-offset-4"
                        >
                            查看来源采购
                        </Link>
                    </div>
                    <Button
                        id={`ppr-credit-use-${toAutomationIdSegment(credit.id)}`}
                        variant="outline"
                        onClick={() => setSelected(credit)}
                    >
                        抵扣本次应付
                    </Button>
                </div>
            ))}
            {selected && (
                <CreditUseDialog
                    request={request}
                    task={task}
                    credit={selected}
                    onClose={() => setSelected(null)}
                    onTaskCompleted={onTaskCompleted}
                />
            )}
        </section>
    )
}
function CreditUseDialog({
    request,
    task,
    credit,
    onClose,
    onTaskCompleted,
}: {
    request: PaymentRequest
    task: PaymentTaskContext
    credit: SupplierCredit
    onClose: () => void
    onTaskCompleted?: () => void
}) {
    const mutation = useSupplierCreditMutation()
    const [error, setError] = useState("")
    const [unknown, setUnknown] = useState(false)
    const submitted = useRef<CreditUseInput | null>(null)
    const available = subtractFixed(credit.amount, credit.used_amount, {
        maxScale: 2,
        outputScale: 2,
    })
    const options = request.lines.flatMap((line) =>
        line.payable_entries
            .filter(
                (entry) =>
                    compareDecimal(entry.open_total, "0", 2) > 0 &&
                    compareDecimal(line.unpaid_amount, "0", 2) > 0,
            )
            .map((entry) => ({
                value: entry.id,
                label: `${line.purchase_order_no} · 可核销 ${entry.open_total} 元`,
                account: line.payable_account_id,
                max: minFixed(line.unpaid_amount, entry.open_total, {
                    maxScale: 2,
                    outputScale: 2,
                }),
            })),
    )
    const form = useAppForm({
        defaultValues: { entry: options[0]?.value ?? "", amount: "" },
        validators: {
            onChange: z
                .object({
                    entry: z.string().min(1, "请选择采购"),
                    amount: z.string(),
                })
                .refine((value) => {
                    try {
                        const option = options.find(
                            (row) => row.value === value.entry,
                        )
                        return (
                            !!option &&
                            compareDecimal(value.amount, "0", 2) > 0 &&
                            compareDecimal(value.amount, available, 2) <= 0 &&
                            compareDecimal(value.amount, option.max, 2) <= 0
                        )
                    } catch {
                        return false
                    }
                }, "抵扣金额不得超过留抵余额或本采购批准未付金额"),
        },
        onSubmit: async ({ value }) => {
            const option = options.find((row) => row.value === value.entry)
            if (!option) return
            const input = submitted.current ?? {
                requestId: request.id,
                expected_version: request.version,
                work_item_id: task.workItemId,
                task_version: task.taskVersion,
                credit_id: credit.id,
                payable_account_id: option.account,
                payable_entry_id: value.entry,
                amount: value.amount,
                idempotency_key: crypto.randomUUID(),
            }
            submitted.current = input
            try {
                await mutation.mutateAsync(input)
                if (
                    compareDecimal(input.amount, request.unpaid_amount, 2) === 0
                )
                    onTaskCompleted?.()
                onClose()
            } catch (err) {
                const uncertain = commandFailureDisposition(err) === "unknown"
                setUnknown(uncertain)
                if (!uncertain) submitted.current = null
                setError(getErrorMessage(err, "抵扣失败，请核对后重试"))
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
            <DialogContent closeButtonId="ppr-credit-close">
                <DialogHeader>
                    <DialogTitle>使用供应商留抵</DialogTitle>
                    <DialogDescription>
                        本次抵扣会减少批准未付金额，不生成新的银行付款。可用余额{" "}
                        {available} 元。
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
                        name="entry"
                        children={(field) => (
                            <field.SelectField
                                id="ppr-credit-entry"
                                label="抵扣采购"
                                required
                                allowClear={false}
                                options={options}
                                disabled={mutation.isPending || unknown}
                            />
                        )}
                    />
                    <form.AppField
                        name="amount"
                        children={(field) => (
                            <field.TextField
                                id="ppr-credit-amount"
                                label="本次抵扣金额"
                                inputMode="decimal"
                                required
                                disabled={mutation.isPending || unknown}
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
                            id="ppr-credit-cancel"
                            type="button"
                            variant="outline"
                            disabled={mutation.isPending || unknown}
                            onClick={onClose}
                        >
                            取消
                        </Button>
                        <form.AppForm>
                            <form.SubmitButton
                                id="ppr-credit-submit"
                                label={unknown ? "核对抵扣结果" : "确认抵扣"}
                                loading={mutation.isPending}
                            />
                        </form.AppForm>
                    </DialogFooter>
                </form>
            </DialogContent>
        </Dialog>
    )
}
