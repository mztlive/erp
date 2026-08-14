"use client"

import * as React from "react"

import { getErrorMessage } from "@/lib/api/errors"
import { type ResultState as SharedResultState } from "@/components/business/feedback"
import type {
    AllocationDraftLine,
    CardFundsReviewItemView,
    FormalOutcome,
    InvoiceDraft,
    ReceiptDraft,
} from "@/features/card-funds-review/types"
import { formatMoney, moneyStrSafe, shortHash } from "../lib/presentation"
import type {
    useRegisterInvoiceMutation,
    useRegisterReceiptMutation,
} from "./queries"

type ResultState = SharedResultState<FormalOutcome>

type RegisterAction = "REGISTER_RECEIPT" | "REGISTER_INVOICE"

/**
 * 登记历史回款/发票的提交逻辑（成功停留当前项并展示结果条）。
 * 依赖表单草稿与 mutation，由 useCardFundsReviewWorkflow 组装调用。
 */
export function useCardFundsRegistration(args: {
    task: CardFundsReviewItemView | undefined
    receiptForm: ReceiptDraft
    invoiceForm: InvoiceDraft
    allocLines: AllocationDraftLine[]
    evidenceRef: string
    setAllocationMode: React.Dispatch<
        React.SetStateAction<"receipt" | "invoice" | null>
    >
    assertAllowed: (action: RegisterAction) => void
    registerReceiptMutation: ReturnType<typeof useRegisterReceiptMutation>
    registerInvoiceMutation: ReturnType<typeof useRegisterInvoiceMutation>
    setLastResult: React.Dispatch<React.SetStateAction<ResultState>>
    setActionError: React.Dispatch<React.SetStateAction<string | null>>
}): {
    submitReceipt: () => Promise<void>
    submitInvoice: () => Promise<void>
} {
    const {
        task,
        receiptForm,
        invoiceForm,
        allocLines,
        evidenceRef,
        setAllocationMode,
        assertAllowed,
        registerReceiptMutation,
        registerInvoiceMutation,
        setLastResult,
        setActionError,
    } = args

    const submitReceipt = React.useCallback(async () => {
        if (!task) return
        setActionError(null)
        try {
            assertAllowed("REGISTER_RECEIPT")
            const result = await registerReceiptMutation.mutateAsync({
                workItemId: task.workItem.workItemId,
                expectedSubjectVersion: task.workItem.subjectVersion,
                receiptNo:
                    receiptForm.receiptNo.trim() ||
                    `SK-${Date.now().toString(36).toUpperCase()}`,
                receivedAt: receiptForm.receivedAt,
                grossAmount: receiptForm.grossAmount,
                allocations: allocLines,
                evidenceReference: evidenceRef.trim() || "银行回单-本次登记",
            })
            // 登记后停留当前项，刷新金额/指纹（invalidate 后 query 更新）
            setAllocationMode(null)
            setLastResult({
                status: "succeeded",
                title: "历史回款已登记",
                description: `已形成回款与分配；净已收 ${formatMoney(result.settledTotal)}。复核完成前指标仍可能不可靠。`,
                reference: result.fundsFactVersion,
                stayOnItem: true,
            })
        } catch (error) {
            setActionError(getErrorMessage(error, "登记回款失败"))
        }
    }, [
        allocLines,
        assertAllowed,
        evidenceRef,
        receiptForm,
        registerReceiptMutation,
        setActionError,
        setAllocationMode,
        setLastResult,
        task,
    ])

    const submitInvoice = React.useCallback(async () => {
        if (!task) return
        setActionError(null)
        try {
            assertAllowed("REGISTER_INVOICE")
            const gross = invoiceForm.grossAmount
            const net =
                invoiceForm.netAmount || moneyStrSafe(Number(gross) / 1.13)
            const tax =
                invoiceForm.taxAmount ||
                moneyStrSafe(Number(gross) - Number(net))
            const result = await registerInvoiceMutation.mutateAsync({
                workItemId: task.workItem.workItemId,
                expectedSubjectVersion: task.workItem.subjectVersion,
                invoiceNo:
                    invoiceForm.invoiceNo.trim() ||
                    `FP-${Date.now().toString(36).toUpperCase()}`,
                issuedAt: invoiceForm.issuedAt,
                grossAmount: gross,
                netAmount: net,
                taxAmount: tax,
                allocations: allocLines,
                evidenceReference: evidenceRef.trim() || "发票扫描件-本次登记",
            })
            setAllocationMode(null)
            setLastResult({
                status: "succeeded",
                title: "历史发票已登记",
                description: `已形成发票与分配；版本 ${shortHash(result.subjectHash)}，净已开票 ${formatMoney(result.invoicedTotal)}。`,
                reference: result.fundsFactVersion,
                stayOnItem: true,
            })
        } catch (error) {
            setActionError(getErrorMessage(error, "登记发票失败"))
        }
    }, [
        allocLines,
        assertAllowed,
        evidenceRef,
        invoiceForm,
        registerInvoiceMutation,
        setActionError,
        setAllocationMode,
        setLastResult,
        task,
    ])

    return { submitReceipt, submitInvoice }
}
