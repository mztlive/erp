"use client"

import * as React from "react"

import { forgetSupplierRefundDraft } from "@/features/supplier-payables/api/refunds"
import {
    useEnsureSupplierRefundDraftMutation,
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
 * 供应商退款确认流程：先创建草稿再确认冻结路线后提交审批。
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
    const ensureRefundMutation = useEnsureSupplierRefundDraftMutation()
    const submitRefundMutation = useSubmitSupplierRefundMutation()

    const [refundRequest, setRefundRequest] =
        React.useState<SupplierRefundRequest | null>(null)
    const [refundDraft, setRefundDraft] =
        React.useState<SupplierRefundRow | null>(null)
    const [refundSubmitOpen, setRefundSubmitOpen] = React.useState(false)
    const refundSlotRef = React.useRef<SupplierRefundIdempotencySlot | null>(
        null,
    )

    /**
     * 按当前意图绑定退款幂等槽。换单或改原因时轮换并丢弃旧草稿缓存。
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
        if (current && current.key !== next.key) {
            forgetSupplierRefundDraft(current.key)
        }
        refundSlotRef.current = next
        return next
    }

    /**
     * 登记供应商退款草稿并打开提交确认。
     *
     * @param reason 非空退款原因。
     */
    async function prepareRefundDraft(reason: string) {
        if (!refundRequest) return
        const slot = bindRefundSlot(refundRequest.sourcePaymentId, reason)
        const res = await ensureRefundMutation.mutateAsync({
            sourcePaymentId: refundRequest.sourcePaymentId,
            supplierId: refundRequest.supplierId,
            amount: refundRequest.amount,
            reason,
            idempotencyKey: slot.key,
        })
        if (res.status !== "succeeded") {
            setActionError(res.message)
            setRefundRequest(null)
            return
        }
        refundSlotRef.current = {
            ...slot,
            refundId: res.refund.refundId,
        }
        setRefundDraft(res.refund)
        setRefundRequest(null)
        openRefundPreview(res.refund.refundId)
        setRefundSubmitOpen(true)
    }

    /**
     * 对已存在的退款草稿打开提交确认。
     *
     * @param row 当前退款草稿投影。
     */
    function beginRefundSubmit(row: SupplierRefundRow) {
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
            description: "已按已绑定的审批流程启动审批，原付款保留。",
            reference: slot.key,
            facts: [
                { label: "退款单号", value: res.refund.refundNo },
                { label: "当前状态", value: res.refund.statusLabel },
            ],
        })
        forgetSupplierRefundDraft(slot.key)
        refundSlotRef.current = null
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
        refundDraftPending: ensureRefundMutation.isPending,
        refundSubmitPending: submitRefundMutation.isPending,
    }
}
