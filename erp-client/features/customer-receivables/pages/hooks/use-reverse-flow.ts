"use client"

import * as React from "react"

import type { ResultState } from "@/components/business/feedback"
import type { ReverseRequest } from "@/features/customer-receivables/components/customer-account-detail-preview"
import {
    useReverseFactMutation,
    useSubmitCustomerRefundMutation,
    useSubmitReceiptReversalMutation,
} from "@/features/customer-receivables/hooks/queries"
import {
    slotForCustomerRefundIntent,
    type CustomerRefundIdempotencySlot,
} from "@/features/customer-receivables/lib/customer-refund-approval"
import {
    slotForReceiptReversalIntent,
    type ReceiptReversalIdempotencySlot,
} from "@/features/customer-receivables/lib/receipt-reversal-approval"
import type {
    CustomerRefundRow,
    ReceiptReversalRow,
    ReverseFactInput,
} from "@/features/customer-receivables/types"
import { FormalCommandKeyLedger } from "@/lib/formal-command"

/**
 * 反向记录（冲正/退款/红票）确认流程。
 *
 * 红票一次提交；客户退款与回款冲正在本地确认后一次创建并启动审批。
 */
export function useReverseFlow(args: {
    closePreview: () => void
    openRefundPreview: (refundId: string) => void
    openReversalPreview: (reversalId: string) => void
    setLastResult: React.Dispatch<React.SetStateAction<ResultState>>
    setActionError: React.Dispatch<React.SetStateAction<string | null>>
    onChanged?: () => void
}): {
    reverseConfirm: ReverseRequest | null
    setReverseConfirm: React.Dispatch<
        React.SetStateAction<ReverseRequest | null>
    >
    reverseReason: string
    setReverseReason: React.Dispatch<React.SetStateAction<string>>
    reverseAmount: string
    setReverseAmount: React.Dispatch<React.SetStateAction<string>>
    confirmReverse: () => Promise<void>
    prepareRefundDraft: (reason: string) => Promise<void>
    confirmRefundSubmit: () => Promise<void>
    beginRefundSubmit: (row: CustomerRefundRow) => void
    refundDraft: CustomerRefundRow | null
    refundSubmitOpen: boolean
    setRefundSubmitOpen: React.Dispatch<React.SetStateAction<boolean>>
    prepareReversalDraft: (reason: string) => Promise<void>
    confirmReversalSubmit: () => Promise<void>
    beginReversalSubmit: (row: ReceiptReversalRow) => void
    reversalDraft: ReceiptReversalRow | null
    reversalSubmitOpen: boolean
    setReversalSubmitOpen: React.Dispatch<React.SetStateAction<boolean>>
    reverseMutation: ReturnType<typeof useReverseFactMutation>
    refundDraftPending: boolean
    refundSubmitPending: boolean
    reversalDraftPending: boolean
    reversalSubmitPending: boolean
} {
    const {
        closePreview,
        openRefundPreview,
        openReversalPreview,
        setLastResult,
        setActionError,
        onChanged,
    } = args
    const reverseMutation = useReverseFactMutation()
    const submitRefundMutation = useSubmitCustomerRefundMutation()
    const submitReversalMutation = useSubmitReceiptReversalMutation()
    const redInvoiceLedgerRef = React.useRef(new FormalCommandKeyLedger())

    const [reverseConfirm, setReverseConfirm] =
        React.useState<ReverseRequest | null>(null)
    const [reverseReason, setReverseReason] = React.useState("")
    const [reverseAmount, setReverseAmount] = React.useState("")
    const [refundDraft, setRefundDraft] =
        React.useState<CustomerRefundRow | null>(null)
    const [pendingRefund, setPendingRefund] = React.useState<{
        sourceFactId: string
        amount?: string
        reason: string
    } | null>(null)
    const [refundSubmitOpen, setRefundSubmitOpen] = React.useState(false)
    const refundSlotRef = React.useRef<CustomerRefundIdempotencySlot | null>(
        null,
    )
    const [reversalDraft, setReversalDraft] =
        React.useState<ReceiptReversalRow | null>(null)
    const [pendingReversal, setPendingReversal] = React.useState<{
        sourceFactId: string
        amount?: string
        reason: string
    } | null>(null)
    const [reversalSubmitOpen, setReversalSubmitOpen] = React.useState(false)
    const reversalSlotRef = React.useRef<ReceiptReversalIdempotencySlot | null>(
        null,
    )

    /**
     * 按当前意图绑定退款幂等槽。
     *
     * @param scopeId 原回款 ID 或已有退款单 ID。
     * @param reason 退款原因。
     */
    function bindRefundSlot(
        scopeId: string,
        reason: string,
    ): CustomerRefundIdempotencySlot {
        const current = refundSlotRef.current
        const next = slotForCustomerRefundIntent(current, scopeId, reason)
        refundSlotRef.current = next
        return next
    }

    /**
     * 冻结本地退款意图并打开提交确认，不写入后端。
     *
     * @param reason 非空退款原因。
     */
    async function prepareRefundDraft(reason: string) {
        if (!reverseConfirm || reverseConfirm.kind !== "refund") return
        const slot = bindRefundSlot(reverseConfirm.sourceFactId, reason)
        setPendingRefund({
            sourceFactId: reverseConfirm.sourceFactId,
            amount: reverseConfirm.amount,
            reason,
        })
        refundSlotRef.current = slot
        setRefundDraft(null)
        setReverseReason(reason)
        setReverseConfirm(null)
        setRefundSubmitOpen(true)
    }

    /**
     * 对已存在的退款草稿打开提交确认。
     *
     * @param row 当前退款草稿投影。
     */
    function beginRefundSubmit(row: CustomerRefundRow) {
        setPendingRefund(null)
        setRefundDraft(row)
        const current = refundSlotRef.current
        if (current?.refundId === row.refundId) {
            setRefundSubmitOpen(true)
            return
        }
        const slot = bindRefundSlot(row.refundId, row.reasonText)
        refundSlotRef.current = {
            ...slot,
            refundId: row.refundId,
        }
        setRefundSubmitOpen(true)
    }

    /**
     * 按当前意图绑定冲正幂等槽。
     *
     * @param scopeId 原回款 ID 或已有冲正单 ID。
     * @param reason 冲正原因。
     */
    function bindReversalSlot(
        scopeId: string,
        reason: string,
    ): ReceiptReversalIdempotencySlot {
        const current = reversalSlotRef.current
        const next = slotForReceiptReversalIntent(current, scopeId, reason)
        reversalSlotRef.current = next
        return next
    }

    /**
     * 冻结本地回款冲正意图并打开提交确认，不写入后端。
     *
     * @param reason 非空冲正原因。
     */
    async function prepareReversalDraft(reason: string) {
        if (!reverseConfirm || reverseConfirm.kind !== "receipt_reverse") return
        const slot = bindReversalSlot(reverseConfirm.sourceFactId, reason)
        setPendingReversal({
            sourceFactId: reverseConfirm.sourceFactId,
            amount: reverseConfirm.amount,
            reason,
        })
        reversalSlotRef.current = slot
        setReversalDraft(null)
        setReverseReason(reason)
        setReverseConfirm(null)
        setReversalSubmitOpen(true)
    }

    /**
     * 对已存在的冲正草稿打开提交确认。
     *
     * @param row 当前冲正草稿投影。
     */
    function beginReversalSubmit(row: ReceiptReversalRow) {
        setPendingReversal(null)
        setReversalDraft(row)
        const current = reversalSlotRef.current
        if (current?.reversalId === row.reversalId) {
            setReversalSubmitOpen(true)
            return
        }
        const slot = bindReversalSlot(row.reversalId, row.reasonText)
        reversalSlotRef.current = {
            ...slot,
            reversalId: row.reversalId,
        }
        setReversalSubmitOpen(true)
    }

    /**
     * 按冻结路线提交回款冲正审批。
     */
    async function confirmReversalSubmit() {
        const slot = reversalSlotRef.current
        if (!slot || (!pendingReversal && !reversalDraft)) return
        if (pendingReversal) {
            const committed = await reverseMutation.mutateAsync({
                kind: "receipt_reverse",
                ...pendingReversal,
                idempotencyKey: slot.key,
            })
            if (committed.status === "unknown") {
                setLastResult({
                    status: "unknown",
                    title: "冲正结果待确认",
                    description: committed.message,
                    reference: committed.idempotencyKey,
                })
                setReversalSubmitOpen(false)
                return
            }
            if (committed.status === "failed") {
                setActionError(committed.message)
                setReversalSubmitOpen(false)
                return
            }
            setLastResult({
                status: "succeeded",
                title: "冲正已提交审批",
                description: committed.message,
                reference: slot.key,
                facts: [{ label: "冲正单号", value: committed.reverseFactNo }],
            })
            reversalSlotRef.current = null
            setPendingReversal(null)
            setReversalDraft(null)
            setReversalSubmitOpen(false)
            setReverseReason("")
            setReverseAmount("")
            openReversalPreview(committed.reverseFactId)
            onChanged?.()
            return
        }
        const res = await submitReversalMutation.mutateAsync({
            reversalId: reversalDraft!.reversalId,
            expectedVersion: reversalDraft!.baselineVersion,
            idempotencyKey: slot.key,
        })
        if (res.status === "unknown") {
            setLastResult({
                status: "unknown",
                title: "冲正结果待确认",
                description: res.message,
                reference: res.idempotencyKey,
            })
            setReversalSubmitOpen(false)
            return
        }
        if (res.status === "failed") {
            setActionError(res.message)
            setReversalSubmitOpen(false)
            return
        }
        setLastResult({
            status: "succeeded",
            title: "冲正已提交审批",
            description: "已按已绑定的审批流程启动审批，原回款保留。",
            reference: slot.key,
            facts: [
                { label: "冲正单号", value: res.reversal.reversalNo },
                { label: "当前状态", value: res.reversal.statusLabel },
            ],
        })
        reversalSlotRef.current = null
        setReversalDraft(res.reversal)
        setReversalSubmitOpen(false)
        setReverseReason("")
        setReverseAmount("")
        openReversalPreview(res.reversal.reversalId)
        onChanged?.()
    }

    /**
     * 按冻结路线提交客户退款审批。
     */
    async function confirmRefundSubmit() {
        const slot = refundSlotRef.current
        if (!slot || (!pendingRefund && !refundDraft)) return
        if (pendingRefund) {
            const committed = await reverseMutation.mutateAsync({
                kind: "refund",
                ...pendingRefund,
                idempotencyKey: slot.key,
            })
            if (committed.status === "unknown") {
                setLastResult({
                    status: "unknown",
                    title: "退款结果待确认",
                    description: committed.message,
                    reference: committed.idempotencyKey,
                })
                setRefundSubmitOpen(false)
                return
            }
            if (committed.status === "failed") {
                setActionError(committed.message)
                setRefundSubmitOpen(false)
                return
            }
            setLastResult({
                status: "succeeded",
                title: "退款已提交审批",
                description: committed.message,
                reference: slot.key,
                facts: [{ label: "退款单号", value: committed.reverseFactNo }],
            })
            refundSlotRef.current = null
            setPendingRefund(null)
            setRefundDraft(null)
            setRefundSubmitOpen(false)
            setReverseReason("")
            setReverseAmount("")
            openRefundPreview(committed.reverseFactId)
            onChanged?.()
            return
        }
        const res = await submitRefundMutation.mutateAsync({
            refundId: refundDraft!.refundId,
            expectedVersion: refundDraft!.baselineVersion,
            idempotencyKey: slot.key,
        })
        if (res.status === "unknown") {
            setLastResult({
                status: "unknown",
                title: "退款结果待确认",
                description: res.message,
                reference: res.idempotencyKey,
            })
            setRefundSubmitOpen(false)
            return
        }
        if (res.status === "failed") {
            setActionError(res.message)
            setRefundSubmitOpen(false)
            return
        }
        setLastResult({
            status: "succeeded",
            title: "退款已提交审批",
            description: "已按已绑定的审批流程启动审批，原回款保留。",
            reference: slot.key,
            facts: [
                { label: "退款单号", value: res.refund.refundNo },
                { label: "当前状态", value: res.refund.statusLabel },
            ],
        })
        refundSlotRef.current = null
        setRefundDraft(res.refund)
        setRefundSubmitOpen(false)
        setReverseReason("")
        setReverseAmount("")
        openRefundPreview(res.refund.refundId)
        onChanged?.()
    }

    async function confirmReverse() {
        if (!reverseConfirm) return
        if (reverseConfirm.kind === "refund") {
            await prepareRefundDraft(reverseReason || "纠错")
            return
        }
        if (reverseConfirm.kind === "receipt_reverse") {
            await prepareReversalDraft(reverseReason || "纠错")
            return
        }
        const commandSlot = `red-invoice:${reverseConfirm.sourceFactId}`
        const payload: Omit<ReverseFactInput, "idempotencyKey"> = {
            kind: "red_invoice",
            sourceFactId: reverseConfirm.sourceFactId,
            amount: reverseAmount,
            reason: reverseReason || "纠错",
        }
        const command = redInvoiceLedgerRef.current.acquire(
            commandSlot,
            "w11-red-invoice",
            payload,
        )
        const res = await reverseMutation.mutateAsync({
            ...command.payload,
            idempotencyKey: command.idempotencyKey,
        })
        redInvoiceLedgerRef.current.settle(commandSlot, res.status)
        if (res.status === "succeeded") {
            setLastResult({
                status: "succeeded",
                title: "反向记录已追加",
                description: res.message,
                reference: res.operationId,
                facts: [
                    { label: "反向单号", value: res.reverseFactNo },
                    { label: "原记录", value: reverseConfirm.label },
                ],
            })
            setReverseConfirm(null)
            setReverseReason("")
            setReverseAmount("")
            closePreview()
            onChanged?.()
            return
        }
        if (res.status === "unknown") {
            setLastResult({
                status: "unknown",
                title: "纠错结果不确定",
                description: res.message,
                reference: res.idempotencyKey,
            })
            setReverseConfirm(null)
            return
        }
        setActionError(res.message)
        setReverseConfirm(null)
    }

    return {
        reverseConfirm,
        setReverseConfirm,
        reverseReason,
        setReverseReason,
        reverseAmount,
        setReverseAmount,
        confirmReverse,
        prepareRefundDraft,
        confirmRefundSubmit,
        beginRefundSubmit,
        refundDraft,
        refundSubmitOpen,
        setRefundSubmitOpen,
        prepareReversalDraft,
        confirmReversalSubmit,
        beginReversalSubmit,
        reversalDraft,
        reversalSubmitOpen,
        setReversalSubmitOpen,
        reverseMutation,
        refundDraftPending: false,
        refundSubmitPending:
            reverseMutation.isPending || submitRefundMutation.isPending,
        reversalDraftPending: false,
        reversalSubmitPending:
            reverseMutation.isPending || submitReversalMutation.isPending,
    }
}
