"use client"

import { Input } from "@/components/ui/input"
import { Field, FieldLabel } from "@/components/ui/field"
import type { useAllocationSession } from "@/features/customer-receivables/hooks/use-allocation-session"
import type { AllocationSessionView } from "@/features/customer-receivables/types"

/** 回款登记的平面字段区；结算主体和已有回款信息保持只读。 */
export function ReceiptSessionFields({
    form,
    session,
    existing,
    locked,
}: {
    form: ReturnType<typeof useAllocationSession>["form"]
    session: AllocationSessionView
    existing: boolean
    locked: boolean
}) {
    return (
        <section
            aria-labelledby="customer-receivables-session-fact-title"
            className="space-y-4 border-b border-grid pb-6"
        >
            <h2
                id="customer-receivables-session-fact-title"
                className="text-base font-semibold"
            >
                回款信息
            </h2>
            <div className="grid gap-4 lg:grid-cols-3">
                <Field>
                    <FieldLabel htmlFor="customer-receivables-session-counterparty">
                        结算主体
                    </FieldLabel>
                    <Input
                        id="customer-receivables-session-counterparty"
                        value={session.counterpartyPartyName}
                        readOnly
                        aria-describedby="customer-receivables-session-counterparty-note"
                    />
                </Field>
                <form.AppField
                    name="receivedAt"
                    children={(field) => (
                        <field.DateTimeField
                            id="customer-receivables-session-received-at"
                            label="实际到账时间"
                            disabled={locked}
                        />
                    )}
                />
                <form.AppField
                    name="amount"
                    children={(field) => (
                        <field.TextField
                            id="customer-receivables-session-amount"
                            label={
                                existing
                                    ? "可核销余额（含税）"
                                    : "到账金额（含税）"
                            }
                            required
                            inputMode="decimal"
                            inputClassName="num text-right"
                            placeholder="请输入到账金额"
                            disabled={locked}
                        />
                    )}
                />
                <form.AppField
                    name="bankReference"
                    children={(field) => (
                        <field.TextField
                            id="customer-receivables-session-bank-reference"
                            label="银行流水 / 回单号"
                            placeholder="填写银行流水或回单引用"
                            disabled={locked}
                        />
                    )}
                />
                <div className="flex min-w-0 flex-col justify-center gap-1 text-sm lg:col-span-2">
                    <p className="break-words text-muted-foreground">
                        经营客户：{session.customerName || "未标注"}
                    </p>
                    <p
                        id="customer-receivables-session-counterparty-note"
                        className="text-xs text-muted-foreground"
                    >
                        本次仅关联此结算主体的应收；结算主体与经营客户可能不同。
                    </p>
                </div>
            </div>
        </section>
    )
}
