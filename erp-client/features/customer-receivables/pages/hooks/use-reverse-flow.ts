"use client"

import * as React from "react"

import type { ResultState } from "@/components/business/feedback"
import type { ReverseRequest } from "@/features/customer-receivables/components/customer-account-detail-preview"
import {
    useEnsureCustomerRefundDraftMutation,
    useReverseFactMutation,
    useSubmitCustomerRefundMutation,
} from "@/features/customer-receivables/hooks/queries"
import type { CustomerRefundRow } from "@/features/customer-receivables/types"

/**
 * 反向记录（冲正/退款/红票）确认流程。
 *
 * 冲正与红票一次提交；客户退款先创建草稿再确认冻结路线后提交审批。
 */
export function useReverseFlow(args: {
    closePreview: () => void
    openRefundPreview: (refundId: string) => void
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
    reverseMutation: ReturnType<typeof useReverseFactMutation>
    refundDraftPending: boolean
    refundSubmitPending: boolean
} {
    const { closePreview, openRefundPreview, setLastResult, setActionError } =
        args
    const reverseMutation = useReverseFactMutation()
    const ensureRefundMutation = useEnsureCustomerRefundDraftMutation()
    const submitRefundMutation = useSubmitCustomerRefundMutation()

    const [reverseConfirm, setReverseConfirm] =
        React.useState<ReverseRequest | null>(null)
    const [reverseReason, setReverseReason] = React.useState("")
    const [reverseAmount, setReverseAmount] = React.useState("")
    const [refundDraft, setRefundDraft] =
        React.useState<CustomerRefundRow | null>(null)
    const [refundSubmitOpen, setRefundSubmitOpen] = React.useState(false)
    const refundIdempotencyRef = React.useRef<string | null>(null)

    /**
     * 登记客户退款草稿并打开提交确认。
     *
     * @param reason 非空退款原因。
     */
    async function prepareRefundDraft(reason: string) {
        if (!reverseConfirm || reverseConfirm.kind !== "refund") return
        if (!refundIdempotencyRef.current) {
            refundIdempotencyRef.current = `w11-rev-${reverseConfirm.sourceFactId}-${Date.now()}`
        }
        const res = await ensureRefundMutation.mutateAsync({
            sourceFactId: reverseConfirm.sourceFactId,
            amount: reverseConfirm.amount,
            reason,
            idempotencyKey: refundIdempotencyRef.current,
        })
        if (res.status !== "succeeded") {
            setActionError(res.message)
            setReverseConfirm(null)
            return
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
        if (!refundIdempotencyRef.current) {
            refundIdempotencyRef.current = `w11-rev-${row.refundId}-${Date.now()}`
        }
        setRefundSubmitOpen(true)
    }

    /**
     * 按冻结路线提交客户退款审批。
     */
    async function confirmRefundSubmit() {
        if (!refundDraft || !refundIdempotencyRef.current) return
        const res = await submitRefundMutation.mutateAsync({
            refundId: refundDraft.refundId,
            expectedVersion: refundDraft.baselineVersion,
            idempotencyKey: refundIdempotencyRef.current,
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
            reference: refundIdempotencyRef.current,
            facts: [
                { label: "退款单号", value: res.refund.refundNo },
                { label: "当前状态", value: res.refund.statusLabel },
            ],
        })
        setRefundDraft(res.refund)
        setRefundSubmitOpen(false)
        setReverseReason("")
        setReverseAmount("")
        refundIdempotencyRef.current = null
        openRefundPreview(res.refund.refundId)
    }

    async function confirmReverse() {
        if (!reverseConfirm) return
        if (reverseConfirm.kind === "refund") {
            await prepareRefundDraft(reverseReason || "纠错")
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
        reverseMutation,
        refundDraftPending: ensureRefundMutation.isPending,
        refundSubmitPending: submitRefundMutation.isPending,
    }
}
