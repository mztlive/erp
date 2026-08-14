"use client"

import * as React from "react"

import type {
    AllocationDraftLine,
    CardFundsReviewItemView,
    InvoiceDraft,
    ReceiptDraft,
} from "@/features/card-funds-review/types"

/**
 * 证据/备注草稿与登记回款/发票表单状态。
 * 切换任务时整体重置为当前任务的 currentEvidence 与空登记草稿。
 */
export function useCardFundsReviewForms(
    task: CardFundsReviewItemView | undefined,
) {
    const [evidenceRef, setEvidenceRef] = React.useState("")
    const [evidenceDocId, setEvidenceDocId] = React.useState("")
    const [comment, setComment] = React.useState("")
    const [receiptForm, setReceiptForm] = React.useState<ReceiptDraft>({
        receiptNo: "",
        receivedAt: "2026-07-01",
        grossAmount: "",
    })
    const [invoiceForm, setInvoiceForm] = React.useState<InvoiceDraft>({
        invoiceNo: "",
        issuedAt: "2026-07-01",
        grossAmount: "",
        netAmount: "",
        taxAmount: "",
    })
    const [allocLines, setAllocLines] = React.useState<AllocationDraftLine[]>(
        [],
    )
    const [allocationMode, setAllocationMode] = React.useState<
        null | "receipt" | "invoice"
    >(null)
    const [evidenceDirty, setEvidenceDirty] = React.useState(false)

    React.useEffect(() => {
        if (!task) return
        setEvidenceRef(task.currentEvidence.evidenceReferences[0] ?? "")
        setEvidenceDocId(task.currentEvidence.evidenceDocumentIds[0] ?? "")
        setComment(task.currentEvidence.comment ?? "")
        setAllocationMode(null)
        setReceiptForm({
            receiptNo: "",
            receivedAt: "2026-07-01",
            grossAmount: "",
        })
        setInvoiceForm({
            invoiceNo: "",
            issuedAt: "2026-07-01",
            grossAmount: "",
            netAmount: "",
            taxAmount: "",
        })
        setAllocLines([])
        setEvidenceDirty(false)
    }, [task])

    const evidenceOk = Boolean(evidenceDocId.trim() || evidenceRef.trim())

    const openAllocation = React.useCallback(
        (mode: "receipt" | "invoice") => {
            if (!task) return
            setAllocationMode(mode)
            setAllocLines([
                {
                    lineId: "al_1",
                    targetAccountId: task.account.id,
                    targetLabel: `${task.salesOrder.orderNo} · ${task.account.customerName}`,
                    amount:
                        mode === "receipt"
                            ? receiptForm.grossAmount || "0.00"
                            : invoiceForm.grossAmount || "0.00",
                },
            ])
        },
        [invoiceForm.grossAmount, receiptForm.grossAmount, task],
    )

    return {
        evidenceRef,
        setEvidenceRef,
        evidenceDocId,
        setEvidenceDocId,
        comment,
        setComment,
        evidenceOk,
        evidenceDirty,
        setEvidenceDirty,
        receiptForm,
        setReceiptForm,
        invoiceForm,
        setInvoiceForm,
        allocLines,
        setAllocLines,
        allocationMode,
        setAllocationMode,
        openAllocation,
    }
}
