"use client"

import type { useAllocationSession } from "@/features/customer-receivables/hooks/use-allocation-session"

/** 回款登记的平面字段区；结算主体和已有回款信息保持只读。 */
export function ReceiptSessionFields({
    form,
    existing,
    locked,
}: {
    form: ReturnType<typeof useAllocationSession>["form"]
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
                到账信息
            </h2>
            <div className="grid gap-x-6 gap-y-5 @2xl:grid-cols-2">
                <form.AppField
                    name="amount"
                    children={(field) => (
                        <field.TextField
                            id="customer-receivables-session-amount"
                            money
                            label={
                                existing
                                    ? "原单到账金额（含税）"
                                    : "到账金额（含税）"
                            }
                            required
                            inputMode="decimal"
                            inputClassName="num"
                            placeholder="请输入到账金额"
                            disabled={locked}
                        />
                    )}
                />
                <form.AppField
                    name="receivedAt"
                    children={(field) => (
                        <field.DateTimeField
                            id="customer-receivables-session-received-at"
                            label="实际到账时间"
                            required
                            timeZone="Asia/Shanghai"
                            showTimeZone={false}
                            description="北京时间（UTC+8）"
                            disabled={locked}
                        />
                    )}
                />
                <form.AppField
                    name="bankReference"
                    children={(field) => (
                        <field.TextField
                            id="customer-receivables-session-bank-reference"
                            label="银行流水 / 回单号（选填）"
                            placeholder="填写银行流水或回单引用"
                            disabled={locked}
                        />
                    )}
                />
            </div>
        </section>
    )
}
