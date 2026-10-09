"use client"

import { MoneyValue } from "@/components/business"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import type { AllocationSessionView } from "@/features/supplier-payables/types"
import {
    paymentBasisNote,
    paymentOutcome,
} from "@/features/supplier-payables/lib/payment-guidance"
import { toAutomationIdSegment } from "@/lib/automation-id"

/** 付款前直接核对条款、累计付款与本次建议，合并付款逐单分配。 */
export function PaymentBasis({
    pool,
    selected,
    amounts,
    disabled,
    onAmountChange,
}: {
    pool: AllocationSessionView["pool"]
    selected: ReadonlySet<string>
    amounts: Readonly<Record<string, string>>
    disabled: boolean
    onAmountChange: (id: string, value: string) => void
}) {
    const targets = pool.filter((item) => selected.has(item.payableAccountId))
    return (
        <section
            aria-label="付款依据"
            className="space-y-4 border-b border-border px-4 py-4"
        >
            <h2 className="text-sm font-semibold">付款依据</h2>
            {targets.map((item) => {
                const guidance = item.paymentGuidance
                const amount = amounts[item.payableAccountId] ?? ""
                const inputId = `supplier-payment-basis-${toAutomationIdSegment(item.payableAccountId)}-amount`
                const values = [
                    ["采购总额", guidance?.purchaseTotal],
                    ["累计已付", guidance?.paidTotal],
                    ["整单剩余应付", item.openTotal],
                    ...(guidance?.prepayGate
                        ? [["履约前需付足", guidance.requiredPrepayment]]
                        : []),
                    ["本次建议付款", guidance?.suggestedAmount],
                ]
                return (
                    <div key={item.payableAccountId} className="space-y-3">
                        <div className="flex flex-wrap items-baseline gap-x-4 gap-y-1 text-sm">
                            <span className="font-medium">
                                {guidance?.termLabel ?? "付款条件待核对"}
                            </span>
                            {targets.length > 1 ? (
                                <span className="num break-all text-muted-foreground">
                                    采购单 {item.sourceDocumentNo}
                                </span>
                            ) : null}
                        </div>
                        <dl className="grid grid-cols-2 gap-x-4 gap-y-3 sm:grid-cols-3">
                            {values.map(([label, value]) => (
                                <div key={label} className="min-w-0 space-y-1">
                                    <dt className="text-xs text-muted-foreground">
                                        {label}
                                    </dt>
                                    <dd className="text-sm font-medium">
                                        {value != null ? (
                                            <MoneyValue value={value} />
                                        ) : (
                                            "待核对"
                                        )}
                                    </dd>
                                </div>
                            ))}
                        </dl>
                        <p className="text-xs text-muted-foreground">
                            {paymentBasisNote(item)}
                        </p>
                        {targets.length > 1 ? (
                            <div className="space-y-2">
                                <Label htmlFor={inputId}>
                                    本单实际付款金额
                                </Label>
                                <Input
                                    id={inputId}
                                    inputMode="decimal"
                                    value={amount}
                                    disabled={disabled}
                                    onChange={(event) =>
                                        onAmountChange(
                                            item.payableAccountId,
                                            event.target.value,
                                        )
                                    }
                                />
                            </div>
                        ) : null}
                        {amount ? (
                            <p className="text-sm" aria-live="polite">
                                {paymentOutcome(item, amount)}
                            </p>
                        ) : null}
                    </div>
                )
            })}
        </section>
    )
}
