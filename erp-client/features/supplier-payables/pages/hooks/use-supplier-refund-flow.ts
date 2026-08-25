"use client"

import * as React from "react"

import {
    useCommitSupplierRefundMutation,
    useSubmitSupplierRefundMutation,
} from "@/features/supplier-payables/hooks/queries"
import {
    slotForSupplierRefundIntent,
    type SupplierRefundIdempotencySlot,
} from "@/features/supplier-payables/lib/supplier-refund-approval"
import type {
    FormalSubmitResult,
    SupplierRefundRequest,
    SupplierRefundRow,
} from "@/features/supplier-payables/types"

/**
 * 供应商退款确认流程：本地确认后由一个原子命令创建单据并启动审批。
 */
export function useSupplierRefundFlow(args: {
    openRefundPreview: (refundId: string) => void
    setLastResult: React.Dispatch<
        React.SetStateAction<FormalSubmitResult | null>
    >
    setActionError: (message: string) => void
}): {
    refundRequest: SupplierRefundRequest | null
    setRefundRequest: React.Dispatch<
        React.SetStateAction<SupplierRefundRequest | null>
    >
    prepareRefundDraft: (reason: string) => Promise<void>
    confirmRefundSubmit: () => Promise<void>
    beginRefundSubmit: (row: SupplierRefundRow) => void
    refundDraft: SupplierRefundRow | null
    refundSubmitOpen: boolean
    setRefundSubmitOpen: React.Dispatch<React.SetStateAction<boolean>>
    refundDraftPending: boolean
    refundSubmitPending: boolean
} {
    const { openRefundPreview, setLastResult, setActionError } = args
    const commitRefundMutation = useCommitSupplierRefundMutation()
    const submitRefundMutation = useSubmitSupplierRefundMutation()

    const [refundRequest, setRefundRequest] =
        React.useState<SupplierRefundRequest | null>(null)
    const [refundDraft, setRefundDraft] =
        React.useState<SupplierRefundRow | null>(null)
    const [pendingCommit, setPendingCommit] = React.useState<{
        sourcePaymentId: string
        amount?: string
        reason: string
    } | null>(null)
    const [refundSubmitOpen, setRefundSubmitOpen] = React.useState(false)
    const refundSlotRef = React.useRef<SupplierRefundIdempotencySlot | null>(
        null,
    )

    /**
     * 按当前意图绑定退款幂等槽。
     *
     * @param scopeId 原付款 ID 或已有退款单 ID。
     * @param reason 退款原因。
     */
    function bindRefundSlot(
        scopeId: string,
        reason: string,
    ): SupplierRefundIdempotencySlot {
        const current = refundSlotRef.current
        const next = slotForSupplierRefundIntent(current, scopeId, reason)
        refundSlotRef.current = next
        return next
    }

    /**
     * 冻结本地退款意图并打开提交确认，不写入后端。
     *
     * @param reason 非空退款原因。
     */
    async function prepareRefundDraft(reason: string) {
        if (!refundRequest) return
        const slot = bindRefundSlot(refundRequest.sourcePaymentId, reason)
        setPendingCommit({
            sourcePaymentId: refundRequest.sourcePaymentId,
            amount: refundRequest.amount,
            reason,
        })
        refundSlotRef.current = slot
        setRefundDraft(null)
        setRefundRequest(null)
        setRefundSubmitOpen(true)
    }

    /**
     * 对已存在的退款草稿打开提交确认。
     *
     * @param row 当前退款草稿投影。
     */
    function beginRefundSubmit(row: SupplierRefundRow) {
        setPendingCommit(null)
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
     * 按冻结路线提交供应商退款审批。
     */
    async function confirmRefundSubmit() {
        const slot = refundSlotRef.current
        if (!slot || (!pendingCommit && !refundDraft)) return
        const res = pendingCommit
            ? await commitRefundMutation.mutateAsync({
                  ...pendingCommit,
                  idempotencyKey: slot.key,
              })
            : await submitRefundMutation.mutateAsync({
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
                operationId: res.idempotencyKey,
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
            description: "已按已绑定的审批流程启动审批，原付款保留。",
            reference: slot.key,
            facts: [
                { label: "退款单号", value: res.refund.refundNo },
                { label: "当前状态", value: res.refund.statusLabel },
            ],
        })
        refundSlotRef.current = null
        setPendingCommit(null)
        setRefundDraft(res.refund)
        setRefundSubmitOpen(false)
        openRefundPreview(res.refund.refundId)
    }

    return {
        refundRequest,
        setRefundRequest,
        prepareRefundDraft,
        confirmRefundSubmit,
        beginRefundSubmit,
        refundDraft,
        refundSubmitOpen,
        setRefundSubmitOpen,
        refundDraftPending: false,
        refundSubmitPending:
            commitRefundMutation.isPending || submitRefundMutation.isPending,
    }
}
