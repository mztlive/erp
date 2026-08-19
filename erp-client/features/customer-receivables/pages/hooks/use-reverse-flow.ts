"use client"

import * as React from "react"

import type { ResultState } from "@/components/business/feedback"
import type { ReverseRequest } from "@/features/customer-receivables/components/customer-account-detail-preview"
import {
    forgetRefundDraft,
    forgetReversalDraft,
} from "@/features/customer-receivables/api/reverse-fact"
import {
    useEnsureCustomerRefundDraftMutation,
    useEnsureReceiptReversalDraftMutation,
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
} from "@/features/customer-receivables/types"

/**
 * 反向记录（冲正/退款/红票）确认流程。
 *
 * 红票一次提交；客户退款与回款冲正先创建草稿再确认冻结路线后提交审批。
 */
export function useReverseFlow(args: {
    closePreview: () => void
    openRefundPreview: (refundId: string) => void
    openReversalPreview: (reversalId: string) => void
    setLastResult: React.Dispatch<React.SetStateAction<ResultState>>
    setActionError: React.Dispatch<React.SetStateAction<string | null>>
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
    } = args
    const reverseMutation = useReverseFactMutation()
    const ensureRefundMutation = useEnsureCustomerRefundDraftMutation()
    const submitRefundMutation = useSubmitCustomerRefundMutation()
    const ensureReversalMutation = useEnsureReceiptReversalDraftMutation()
    const submitReversalMutation = useSubmitReceiptReversalMutation()

    const [reverseConfirm, setReverseConfirm] =
        React.useState<ReverseRequest | null>(null)
    const [reverseReason, setReverseReason] = React.useState("")
    const [reverseAmount, setReverseAmount] = React.useState("")
    const [refundDraft, setRefundDraft] =
        React.useState<CustomerRefundRow | null>(null)
    const [refundSubmitOpen, setRefundSubmitOpen] = React.useState(false)
    const refundSlotRef = React.useRef<CustomerRefundIdempotencySlot | null>(
        null,
    )
    const [reversalDraft, setReversalDraft] =
        React.useState<ReceiptReversalRow | null>(null)
    const [reversalSubmitOpen, setReversalSubmitOpen] = React.useState(false)
    const reversalSlotRef = React.useRef<ReceiptReversalIdempotencySlot | null>(
        null,
    )

    /**
     * 按当前意图绑定退款幂等槽。换单或改原因时轮换并丢弃旧草稿缓存。
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
        if (current && current.key !== next.key) {
            forgetRefundDraft(current.key)
        }
        refundSlotRef.current = next
        return next
    }

    /**
     * 登记客户退款草稿并打开提交确认。
     *
     * @param reason 非空退款原因。
     */
    async function prepareRefundDraft(reason: string) {
        if (!reverseConfirm || reverseConfirm.kind !== "refund") return
        const slot = bindRefundSlot(reverseConfirm.sourceFactId, reason)
        const res = await ensureRefundMutation.mutateAsync({
            sourceFactId: reverseConfirm.sourceFactId,
            amount: reverseConfirm.amount,
            reason,
            idempotencyKey: slot.key,
        })
        if (res.status !== "succeeded") {
            setActionError(res.message)
            setReverseConfirm(null)
            return
        }
        refundSlotRef.current = {
            ...slot,
            refundId: res.refund.refundId,
        }
        setRefundDraft(res.refund)
        setReverseReason(reason)
        setReverseConfirm(null)
        openRefundPreview(res.refund.refundId)
        setRefundSubmitOpen(true)
    }

    /**
     * 对已存在的退款草稿打开提交确认。
     *
     * @param row 当前退款草稿投影。
     */
    function beginRefundSubmit(row: CustomerRefundRow) {
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
     * 按当前意图绑定冲正幂等槽。换单或改原因时轮换并丢弃旧草稿缓存。
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
        if (current && current.key !== next.key) {
            forgetReversalDraft(current.key)
        }
        reversalSlotRef.current = next
        return next
    }

    /**
     * 登记回款冲正草稿并打开提交确认。
     *
     * @param reason 非空冲正原因。
     */
    async function prepareReversalDraft(reason: string) {
        if (!reverseConfirm || reverseConfirm.kind !== "receipt_reverse") return
        const slot = bindReversalSlot(reverseConfirm.sourceFactId, reason)
        const res = await ensureReversalMutation.mutateAsync({
            sourceFactId: reverseConfirm.sourceFactId,
            amount: reverseConfirm.amount,
            reason,
            idempotencyKey: slot.key,
        })
        if (res.status !== "succeeded") {
            setActionError(res.message)
            setReverseConfirm(null)
            return
        }
        reversalSlotRef.current = {
            ...slot,
            reversalId: res.reversal.reversalId,
        }
        setReversalDraft(res.reversal)
        setReverseReason(reason)
        setReverseConfirm(null)
        openReversalPreview(res.reversal.reversalId)
        setReversalSubmitOpen(true)
    }

    /**
     * 对已存在的冲正草稿打开提交确认。
     *
     * @param row 当前冲正草稿投影。
     */
    function beginReversalSubmit(row: ReceiptReversalRow) {
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
        if (!reversalDraft || !slot) return
        const res = await submitReversalMutation.mutateAsync({
            reversalId: reversalDraft.reversalId,
            expectedVersion: reversalDraft.baselineVersion,
            idempotencyKey: slot.key,
        })
        if (res.status !== "succeeded") {
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
        forgetReversalDraft(slot.key)
        reversalSlotRef.current = null
        setReversalDraft(res.reversal)
        setReversalSubmitOpen(false)
        setReverseReason("")
        setReverseAmount("")
        openReversalPreview(res.reversal.reversalId)
    }

    /**
     * 按冻结路线提交客户退款审批。
     */
    async function confirmRefundSubmit() {
        const slot = refundSlotRef.current
        if (!refundDraft || !slot) return
        const res = await submitRefundMutation.mutateAsync({
            refundId: refundDraft.refundId,
            expectedVersion: refundDraft.baselineVersion,
            idempotencyKey: slot.key,
        })
        if (res.status !== "succeeded") {
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
        forgetRefundDraft(slot.key)
        refundSlotRef.current = null
        setRefundDraft(res.refund)
        setRefundSubmitOpen(false)
        setReverseReason("")
        setReverseAmount("")
        openRefundPreview(res.refund.refundId)
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
        const key = `w11-rev-${reverseConfirm.sourceFactId}-${Date.now()}`
        const res = await reverseMutation.mutateAsync({
            kind: reverseConfirm.kind,
            sourceFactId: reverseConfirm.sourceFactId,
            amount:
                reverseConfirm.kind === "red_invoice"
                    ? reverseAmount
                    : undefined,
            reason: reverseReason || "纠错",
            idempotencyKey: key,
        })
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
        refundDraftPending: ensureRefundMutation.isPending,
        refundSubmitPending: submitRefundMutation.isPending,
        reversalDraftPending: ensureReversalMutation.isPending,
        reversalSubmitPending: submitReversalMutation.isPending,
    }
}
