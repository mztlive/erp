"use client"

import { FormalActionConfirmDialog } from "@/components/business"
import type { PaymentRecipient } from "@/features/supplier-payables/types"

/** 供应商付款执行确认；确认后直接形成付款事实并核销。 */
export function SupplierPaymentSubmitConfirmDialog({
    open,
    pending,
    supplierName,
    paymentAmount,
    allocatedAmount,
    recipient,
    onOpenChange,
    onConfirm,
}: {
    open: boolean
    pending: boolean
    supplierName: string
    paymentAmount: string
    allocatedAmount: string
    recipient?: PaymentRecipient
    onOpenChange: (open: boolean) => void
    onConfirm: () => void
}) {
    const bankLabel = recipient
        ? [recipient.bankName, recipient.bankBranchName]
              .filter(Boolean)
              .join(" · ") || "未填写"
        : "未加载"

    return (
        <FormalActionConfirmDialog
            open={open}
            onOpenChange={onOpenChange}
            actionLabel="登记付款并核销"
            title="确认登记付款并核销"
            confirmLabel="确认付款"
            fromStatus={{ label: "待付款", tone: "neutral" }}
            toStatus={{ label: "已过账", tone: "success" }}
            description="确认后将立即形成付款记录并核销，不再进入付款审批。请先核对收款账户、付款金额和银行回单。"
            lockedFields={[
                `供应商 ${supplierName}`,
                `收款户名 ${recipient?.accountName ?? "未加载"}`,
                `开户行 ${bankLabel}`,
                `收款账号 ${recipient?.accountNumberMasked ?? "未加载"}`,
                `付款金额 ${paymentAmount || "0"}`,
                `本次核销 ${allocatedAmount}`,
                "银行回单",
            ]}
            effects={[
                "形成已过账付款记录",
                "按本次分配核销应付",
                "更新付款任务进度",
            ]}
            irreversibleEffects={[
                "形成已过账付款记录；纠错须走付款冲正或供应商退款",
            ]}
            pending={pending}
            onConfirm={() => {
                void onConfirm()
            }}
        />
    )
}
