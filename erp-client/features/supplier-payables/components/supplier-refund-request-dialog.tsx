"use client"

import { FinancialRequestDialog } from "@/components/business/financial-request-dialog"
import type { DocumentApprovalView } from "@/features/approval-workflow/types"
import { SupplierRefundApprovalArea } from "@/features/supplier-payables/components/supplier-refund-approval-area"

/** Collect the actual partial or full refund amount and submit one stable intent. */
export function SupplierRefundRequestDialog({
    approval,
    settlementMethod,
    ...props
}: {
    settlementMethod?: "CASH_REFUND" | "SUPPLIER_CREDIT"
    open: boolean
    pending: boolean
    sourceLabel?: string
    amount?: string
    approval?: DocumentApprovalView
    onOpenChange: (open: boolean) => void
    onSubmit: (reason: string, amount?: string) => void | Promise<void>
}) {
    return (
        <FinancialRequestDialog
            {...props}
            id="supplier-payables-refund-request"
            title={
                settlementMethod === "SUPPLIER_CREDIT"
                    ? "供应商留抵申请"
                    : "供应商退款"
            }
            amountLabel={
                settlementMethod === "SUPPLIER_CREDIT"
                    ? "本次转留抵金额"
                    : undefined
            }
            amountDescription={
                settlementMethod === "SUPPLIER_CREDIT"
                    ? "填写供应商确认保留供后续采购抵扣的金额；审批后余额才可使用。"
                    : undefined
            }
            amountError={
                settlementMethod === "SUPPLIER_CREDIT"
                    ? "金额须大于零且不超过本次可转留抵金额，最多两位小数"
                    : undefined
            }
            editableAmount
            description={
                settlementMethod === "SUPPLIER_CREDIT"
                    ? "核对采购与供应商确认的留抵依据。审批通过后转为该供应商可用余额，后续仅用于同一付款主体的采购。"
                    : "按本次实际到账金额提交退款审批，支持部分退款。审批通过后更新已付金额，原付款记录保留。"
            }
            submitLabel={
                settlementMethod === "SUPPLIER_CREDIT"
                    ? "提交留抵审批"
                    : "提交退款审批"
            }
            approvalContent={
                approval ? (
                    <SupplierRefundApprovalArea
                        phase="draft"
                        approval={approval}
                    />
                ) : undefined
            }
        />
    )
}
