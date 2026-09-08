"use client"

import * as React from "react"
import { SubmissionResultUnknownError } from "@/lib/submission-result"

import {
    useCommitPaymentReversalMutation,
    useSubmitPaymentReversalMutation,
} from "@/features/supplier-payables/hooks/queries"
import {
    slotForPaymentReversalIntent,
    type PaymentReversalIdempotencySlot,
} from "@/features/supplier-payables/lib/payment-reversal-approval"
import type {
    FormalSubmitResult,
    PaymentReversalRequest,
    PaymentReversalRow,
} from "@/features/supplier-payables/types"

/**
 * 付款冲正确认流程：本地确认后由一个原子命令创建单据并启动审批。
 */
export function usePaymentReversalFlow(args: {
    openReversalPreview: (reversalId: string) => void
    setLastResult: React.Dispatch<
        React.SetStateAction<FormalSubmitResult | null>
    >
    setActionError: (message: string) => void
}): {
    reversalRequest: PaymentReversalRequest | null
    setReversalRequest: React.Dispatch<
        React.SetStateAction<PaymentReversalRequest | null>
    >
    prepareReversalDraft: (
        reason: string,
        request?: PaymentReversalRequest | null,
    ) => Promise<void>
    confirmReversalSubmit: () => Promise<void>
    beginReversalSubmit: (row: PaymentReversalRow) => void
    reversalDraft: PaymentReversalRow | null
    reversalSubmitOpen: boolean
    setReversalSubmitOpen: React.Dispatch<React.SetStateAction<boolean>>
    reversalDraftPending: boolean
    reversalSubmitPending: boolean
} {
    const { openReversalPreview, setLastResult, setActionError } = args
    const commitReversalMutation = useCommitPaymentReversalMutation()
    const submitReversalMutation = useSubmitPaymentReversalMutation()

    const [reversalRequest, setReversalRequest] =
        React.useState<PaymentReversalRequest | null>(null)
    const [reversalDraft, setReversalDraft] =
        React.useState<PaymentReversalRow | null>(null)
    const [pendingCommit, setPendingCommit] = React.useState<{
        sourcePaymentId: string
        amount?: string
        reason: string
    } | null>(null)
    const [reversalSubmitOpen, setReversalSubmitOpen] = React.useState(false)
    const reversalSlotRef = React.useRef<PaymentReversalIdempotencySlot | null>(
        null,
    )

    /**
     * 按当前意图绑定冲正幂等槽。
     *
     * @param scopeId 原付款 ID 或已有冲正单 ID。
     * @param reason 冲正原因。
     */
    function bindReversalSlot(
        scopeId: string,
        reason: string,
    ): PaymentReversalIdempotencySlot {
        const current = reversalSlotRef.current
        const next = slotForPaymentReversalIntent(current, scopeId, reason)
        reversalSlotRef.current = next
        return next
    }

    /**
     * 从原因表单提交付款冲正，保持同一意图的重试标识。
     *
     * @param reason 非空冲正原因。
     */
    async function prepareReversalDraft(
        reason: string,
        request?: PaymentReversalRequest | null,
    ) {
        const current = request ?? reversalRequest
        if (!current) return
        const slot = bindReversalSlot(current.sourcePaymentId, reason)
        const intent = {
            sourcePaymentId: current.sourcePaymentId,
            amount: current.amount,
            reason,
        }
        setPendingCommit(intent)
        reversalSlotRef.current = slot
        setReversalDraft(null)
        await confirmReversalSubmit(intent)
    }

    /**
     * 对已存在的冲正草稿打开提交确认。
     *
     * @param row 当前冲正草稿投影。
     */
    function beginReversalSubmit(row: PaymentReversalRow) {
        setPendingCommit(null)
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
     * 按冻结路线提交付款冲正审批。
     */
    async function confirmReversalSubmit(
        intent?: NonNullable<typeof pendingCommit>,
    ) {
        const submitted = intent ?? pendingCommit
        const slot = reversalSlotRef.current
        if (!slot || (!submitted && !reversalDraft)) return
        const res = submitted
            ? await commitReversalMutation.mutateAsync({
                  ...submitted,
                  idempotencyKey: slot.key,
              })
            : await submitReversalMutation.mutateAsync({
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
                operationId: res.idempotencyKey,
            })
            setReversalSubmitOpen(false)
            if (intent) throw new SubmissionResultUnknownError(res.message)
            return
        }
        if (res.status === "failed") {
            setActionError(res.message)
            setReversalSubmitOpen(false)
            if (intent) throw new Error(res.message)
            return
        }
        setLastResult({
            status: "succeeded",
            title: "冲正已提交审批",
            description: "已按已绑定的审批流程启动审批，原付款保留。",
            reference: slot.key,
            facts: [
                { label: "冲正单号", value: res.reversal.reversalNo },
                { label: "当前状态", value: res.reversal.statusLabel },
            ],
        })
        reversalSlotRef.current = null
        setPendingCommit(null)
        setReversalDraft(res.reversal)
        setReversalSubmitOpen(false)
        setReversalRequest(null)
        openReversalPreview(res.reversal.reversalId)
    }

    return {
        reversalRequest,
        setReversalRequest,
        prepareReversalDraft,
        confirmReversalSubmit,
        beginReversalSubmit,
        reversalDraft,
        reversalSubmitOpen,
        setReversalSubmitOpen,
        reversalDraftPending: false,
        reversalSubmitPending:
            commitReversalMutation.isPending ||
            submitReversalMutation.isPending,
    }
}
