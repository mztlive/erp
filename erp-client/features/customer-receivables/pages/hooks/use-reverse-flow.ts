"use client"

import * as React from "react"

import type { ResultState } from "@/components/business/feedback"
import type { ReverseRequest } from "@/features/customer-receivables/components/customer-account-detail-preview"
import { useReverseFactMutation } from "@/features/customer-receivables/hooks/queries"

/** 反向记录（冲正/退款/红票）确认流程：收集请求 → mutate → 结果分流。 */
export function useReverseFlow(args: {
    closePreview: () => void
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
    reverseMutation: ReturnType<typeof useReverseFactMutation>
} {
    const { closePreview, setLastResult, setActionError } = args
    const reverseMutation = useReverseFactMutation()

    const [reverseConfirm, setReverseConfirm] =
        React.useState<ReverseRequest | null>(null)
    const [reverseReason, setReverseReason] = React.useState("")
    const [reverseAmount, setReverseAmount] = React.useState("")

    async function confirmReverse() {
        if (!reverseConfirm) return
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
        reverseMutation,
    }
}
