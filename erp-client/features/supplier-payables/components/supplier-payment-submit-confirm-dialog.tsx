"use client"

import { TriangleAlertIcon } from "lucide-react"

import { FormalActionConfirmDialog, MoneyValue } from "@/components/business"
import type { PaymentRecipient } from "@/features/supplier-payables/types"

type PaymentResult = {
    id: string
    documentNo: string
    termLabel: string
    amount: string
    outcome: string
}

/** 供应商付款执行确认；确认后直接形成付款事实并核销。 */
export function SupplierPaymentSubmitConfirmDialog({
    open,
    pending,
    paymentAmount,
    recipient,
    paymentResults = [],
    onOpenChange,
    onConfirm,
    id,
    idPrefix,
}: {
    open: boolean
    pending: boolean
    paymentAmount: string
    recipient?: PaymentRecipient
    paymentResults?: readonly PaymentResult[]
    onOpenChange: (open: boolean) => void
    onConfirm: () => void | Promise<void>
    id?: string
    idPrefix?: string
}) {
    const bankLabel = recipient
        ? [recipient.bankName, recipient.bankBranchName]
              .filter(Boolean)
              .join(" · ") || "未填写"
        : "未加载"
    const recipientFields = [
        ["收款户名", recipient?.accountName ?? "未加载"],
        ["开户行", bankLabel],
        ["收款账号", recipient?.accountNumberMasked ?? "未加载"],
    ]

    return (
        <FormalActionConfirmDialog
            actionVariant="default"
            layout="compact"
            contentClassName="max-h-[calc(100dvh-2rem)] overflow-y-auto data-[size=default]:max-w-[calc(100%-2rem)] data-[size=default]:sm:max-w-xl"
            id={id}
            idPrefix={idPrefix ?? "supplier-payables-payment-submit-confirm"}
            open={open}
            onOpenChange={onOpenChange}
            actionLabel="付款"
            title="确认登记付款"
            confirmLabel="确认登记付款"
            fromStatus={{ label: "待付款", tone: "neutral" }}
            toStatus={{ label: "已过账", tone: "success" }}
            description="请核对实际银行付款与回单，确认后登记并核销。"
            formContent={
                <div className="min-w-0 space-y-5">
                    <section
                        aria-label="本次付款"
                        className="rounded-lg bg-muted px-5 py-4"
                    >
                        <p className="mb-1 text-xs text-muted-foreground">
                            本次实际付款金额
                        </p>
                        <MoneyValue value={paymentAmount || "0"} size="hero" />
                    </section>

                    <section aria-label="收款信息" className="space-y-3">
                        <h3 className="text-sm font-medium">收款信息</h3>
                        <dl className="space-y-3 text-sm">
                            {recipientFields.map(([label, value]) => (
                                <div
                                    key={label}
                                    className="grid min-w-0 grid-cols-[4rem_minmax(0,1fr)] items-baseline gap-4"
                                >
                                    <dt className="text-muted-foreground">
                                        {label}
                                    </dt>
                                    <dd className="min-w-0 break-words font-medium">
                                        {value}
                                    </dd>
                                </div>
                            ))}
                        </dl>
                    </section>

                    {paymentResults.length > 0 ? (
                        <section
                            aria-label="登记后结果"
                            className="space-y-3 border-t border-border pt-4"
                        >
                            <div className="flex items-baseline justify-between gap-3">
                                <h3 className="text-sm font-medium">
                                    登记后结果
                                </h3>
                                <span className="text-xs text-muted-foreground">
                                    共 {paymentResults.length} 笔
                                </span>
                            </div>
                            <div className="max-h-48 space-y-4 overflow-y-auto">
                                {paymentResults.map((result) => (
                                    <div
                                        key={result.id}
                                        className="space-y-2 text-sm"
                                    >
                                        <p className="num break-all text-xs text-muted-foreground">
                                            采购单 {result.documentNo}
                                        </p>
                                        <div className="flex flex-wrap items-baseline justify-between gap-x-4 gap-y-1">
                                            <span className="font-medium">
                                                {result.termLabel}
                                            </span>
                                            <span className="flex items-baseline gap-2">
                                                <span className="text-xs text-muted-foreground">
                                                    本次核销
                                                </span>
                                                <MoneyValue
                                                    value={result.amount}
                                                />
                                            </span>
                                        </div>
                                        <p className="text-sm leading-relaxed text-muted-foreground">
                                            {result.outcome}
                                        </p>
                                    </div>
                                ))}
                            </div>
                        </section>
                    ) : null}

                    <p className="flex items-start gap-2 border-t border-border pt-4 text-xs leading-relaxed text-muted-foreground">
                        <TriangleAlertIcon
                            aria-hidden="true"
                            className="mt-0.5 size-3.5 shrink-0 text-warning"
                        />
                        登记后不可直接撤回，纠错须走付款冲正或供应商退款。
                    </p>
                </div>
            }
            pending={pending}
            onConfirm={onConfirm}
        />
    )
}
