"use client"

import { FormalActionConfirmDialog } from "@/components/business"
import type { PaymentRecipient } from "@/features/supplier-payables/types"

/** 供应商付款执行确认；确认后直接形成付款事实并核销。 */
export function SupplierPaymentSubmitConfirmDialog({
    open,
    pending,
    paymentAmount,
    recipient,
    onOpenChange,
    onConfirm,
    id,
    idPrefix,
}: {
    open: boolean
    pending: boolean
    paymentAmount: string
    recipient?: PaymentRecipient
    onOpenChange: (open: boolean) => void
    onConfirm: () => void
    id?: string
    idPrefix?: string
}) {
    const bankLabel = recipient
        ? [recipient.bankName, recipient.bankBranchName]
              .filter(Boolean)
              .join(" · ") || "未填写"
        : "未加载"

    return (
        <FormalActionConfirmDialog
            id={id}
            idPrefix={idPrefix ?? "supplier-payables-payment-submit-confirm"}
            open={open}
            onOpenChange={onOpenChange}
            actionLabel="付款"
            title="确认付款"
            confirmLabel="确认付款"
            fromStatus={{ label: "待付款", tone: "neutral" }}
            toStatus={{ label: "已过账", tone: "success" }}
            description="确认后立即过账并核销。"
            lockedFields={[
                `收款户名 ${recipient?.accountName ?? "未加载"}`,
                `开户行 ${bankLabel}`,
                `收款账号 ${recipient?.accountNumberMasked ?? "未加载"}`,
                `付款金额 ${paymentAmount || "0"}`,
            ]}
            irreversibleEffects={["纠错须走付款冲正或供应商退款"]}
            pending={pending}
            onConfirm={() => {
                void onConfirm()
            }}
        />
    )
}
