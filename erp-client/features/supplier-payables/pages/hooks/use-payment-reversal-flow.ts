"use client"

import * as React from "react"
import { FormalCommandKeyLedger } from "@/lib/formal-command"
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

type ExistingSubmitPayload = {
    reversalId: string
    expectedVersion: number
    resubmissionReason?: string
}

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
    confirmReversalSubmit: (resubmissionReason?: string) => Promise<void>
    beginReversalSubmit: (row: PaymentReversalRow) => void
    reversalDraft: PaymentReversalRow | null
    reversalSubmitOpen: boolean
    reversalSubmitUnknown: boolean
    endReversalRecovery: () => void
    reversalSubmitReason?: string
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
    const [reversalSubmitUnknown, setReversalSubmitUnknown] =
        React.useState(false)
    const submitLedger = React.useRef(new FormalCommandKeyLedger())
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
        if (reversalSubmitUnknown) {
            throw new Error("请先核对上一笔冲正提交结果，再开始新的办理。")
        }
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
        await confirmReversalSubmit(undefined, intent)
    }

    /**
     * 对已存在的冲正草稿打开提交确认。
     *
     * @param row 当前冲正草稿投影。
     */
    function beginReversalSubmit(row: PaymentReversalRow) {
        if (reversalSubmitUnknown) {
            if (row.reversalId !== reversalDraft?.reversalId) {
                setActionError("请先核对上一笔冲正提交结果，再提交其他单据。")
                return
            }
            setReversalSubmitOpen(true)
            return
        }
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
        resubmissionReason?: string,
        intent?: NonNullable<typeof pendingCommit>,
    ) {
        const submitted = intent ?? pendingCommit
        const slot = reversalSlotRef.current
        if (!slot || (!submitted && !reversalDraft)) return
        const command =
            !submitted && reversalDraft
                ? submitLedger.current.acquire<ExistingSubmitPayload>(
                      "submit",
                      "reversal-resubmit",
                      {
                          reversalId: reversalDraft.reversalId,
                          expectedVersion: reversalDraft.baselineVersion,
                          resubmissionReason,
                      },
                  )
                : undefined
        const res = submitted
            ? await commitReversalMutation.mutateAsync({
                  ...submitted,
                  idempotencyKey: slot.key,
              })
            : await submitReversalMutation.mutateAsync({
                  ...command!.payload,
                  idempotencyKey: command!.idempotencyKey,
              })
        const remainsUnknown =
            !submitted &&
            res.status !== "succeeded" &&
            (reversalSubmitUnknown || res.status === "unknown")
        if (!submitted) {
            submitLedger.current.settle(
                "submit",
                remainsUnknown ? "unknown" : res.status,
            )
            setReversalSubmitUnknown(remainsUnknown)
        }
        if (res.status !== "succeeded" && remainsUnknown) {
            setLastResult({
                status: "unknown",
                title: "冲正结果待确认",
                description: res.message,
            })
            throw new SubmissionResultUnknownError(res.message)
        }
        if (res.status === "unknown") {
            setLastResult({
                status: "unknown",
                title: "冲正结果待确认",
                description: res.message,
                operationId: res.idempotencyKey,
            })
            setReversalSubmitOpen(false)
            if (intent) throw new SubmissionResultUnknownError(res.message)
            return
        }
        if (res.status === "failed") {
            setActionError(res.message)
            if (submitted) setReversalSubmitOpen(false)
            throw new Error(res.message)
        }
        setLastResult({
            status: "succeeded",
            title: "冲正已提交审批",
            description: "已按已绑定的审批流程启动审批，原付款保留。",
            reference: res.reversal.reversalNo,
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
        reversalSubmitUnknown,
        endReversalRecovery: () => {
            submitLedger.current = new FormalCommandKeyLedger()
            setReversalSubmitUnknown(false)
            setReversalSubmitOpen(false)
            reversalSlotRef.current = null
            setLastResult(null)
            if (reversalDraft) openReversalPreview(reversalDraft.reversalId)
            setReversalDraft(null)
        },
        reversalSubmitReason:
            submitLedger.current.peek<ExistingSubmitPayload>("submit")?.payload
                .resubmissionReason,
        setReversalSubmitOpen,
        reversalDraftPending: false,
        reversalSubmitPending:
            commitReversalMutation.isPending ||
            submitReversalMutation.isPending,
    }
}
