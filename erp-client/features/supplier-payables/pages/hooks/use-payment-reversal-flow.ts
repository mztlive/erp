"use client"

import * as React from "react"

import { forgetPaymentReversalDraft } from "@/features/supplier-payables/api/reversals"
import {
    useEnsurePaymentReversalDraftMutation,
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
 * 付款冲正确认流程：先创建草稿再确认冻结路线后提交审批。
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
    const ensureReversalMutation = useEnsurePaymentReversalDraftMutation()
    const submitReversalMutation = useSubmitPaymentReversalMutation()

    const [reversalRequest, setReversalRequest] =
        React.useState<PaymentReversalRequest | null>(null)
    const [reversalDraft, setReversalDraft] =
        React.useState<PaymentReversalRow | null>(null)
    const [reversalSubmitOpen, setReversalSubmitOpen] = React.useState(false)
    const reversalSlotRef =
        React.useRef<PaymentReversalIdempotencySlot | null>(null)

    /**
     * 按当前意图绑定冲正幂等槽。换单或改原因时轮换并丢弃旧草稿缓存。
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
        if (current && current.key !== next.key) {
            forgetPaymentReversalDraft(current.key)
        }
        reversalSlotRef.current = next
        return next
    }

    /**
     * 登记付款冲正草稿并打开提交确认。
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
        const res = await ensureReversalMutation.mutateAsync({
            sourcePaymentId: current.sourcePaymentId,
            amount: current.amount,
            reason,
            idempotencyKey: slot.key,
        })
        if (res.status !== "succeeded") {
            setActionError(res.message)
            setReversalRequest(null)
            return
        }
        reversalSlotRef.current = {
            ...slot,
            reversalId: res.reversal.reversalId,
        }
        setReversalDraft(res.reversal)
        setReversalRequest(null)
        openReversalPreview(res.reversal.reversalId)
        setReversalSubmitOpen(true)
    }

    /**
     * 对已存在的冲正草稿打开提交确认。
     *
     * @param row 当前冲正草稿投影。
     */
    function beginReversalSubmit(row: PaymentReversalRow) {
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
            description: "已按已绑定的审批流程启动审批，原付款保留。",
            reference: slot.key,
            facts: [
                { label: "冲正单号", value: res.reversal.reversalNo },
                { label: "当前状态", value: res.reversal.statusLabel },
            ],
        })
        forgetPaymentReversalDraft(slot.key)
        reversalSlotRef.current = null
        setReversalDraft(res.reversal)
        setReversalSubmitOpen(false)
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
        reversalDraftPending: ensureReversalMutation.isPending,
        reversalSubmitPending: submitReversalMutation.isPending,
    }
}
