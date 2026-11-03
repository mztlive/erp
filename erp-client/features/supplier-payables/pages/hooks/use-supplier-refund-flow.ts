"use client"

import * as React from "react"
import { FormalCommandKeyLedger } from "@/lib/formal-command"
import { SubmissionResultUnknownError } from "@/lib/submission-result"

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

type ExistingSubmitPayload = {
    refundId: string
    expectedVersion: number
    resubmissionReason?: string
}

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
    prepareRefundDraft: (reason: string, amount?: string) => Promise<void>
    confirmRefundSubmit: (resubmissionReason?: string) => Promise<void>
    beginRefundSubmit: (row: SupplierRefundRow) => void
    refundDraft: SupplierRefundRow | null
    refundSubmitOpen: boolean
    refundSubmitUnknown: boolean
    endRefundRecovery: () => void
    refundSubmitReason?: string
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
        settlementMethod?: "CASH_REFUND" | "SUPPLIER_CREDIT"
        sourcePurchaseOrderId?: string
        settlementDecisionKey?: string
        amount?: string
        reason: string
    } | null>(null)
    const [refundSubmitOpen, setRefundSubmitOpen] = React.useState(false)
    const [refundSubmitUnknown, setRefundSubmitUnknown] = React.useState(false)
    const submitLedger = React.useRef(new FormalCommandKeyLedger())
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
        amount?: string,
    ): SupplierRefundIdempotencySlot {
        const current = refundSlotRef.current
        const next = slotForSupplierRefundIntent(
            current,
            scopeId,
            reason,
            amount,
        )
        refundSlotRef.current = next
        return next
    }

    /**
     * 从原因表单提交退款，保持同一意图的重试标识。
     *
     * @param reason 非空退款原因。
     */
    async function prepareRefundDraft(reason: string, amount?: string) {
        if (refundSubmitUnknown) {
            throw new Error("请先核对上一笔退款提交结果，再开始新的办理。")
        }
        if (!refundRequest) return
        const slot = bindRefundSlot(
            `${refundRequest.sourcePaymentId}:${refundRequest.settlementMethod ?? "CASH_REFUND"}:${refundRequest.sourcePurchaseOrderId ?? ""}:${refundRequest.settlementDecisionKey ?? ""}`,
            reason,
            amount ?? refundRequest.amount,
        )
        const intent = {
            sourcePaymentId: refundRequest.sourcePaymentId,
            settlementMethod: refundRequest.settlementMethod,
            sourcePurchaseOrderId: refundRequest.sourcePurchaseOrderId,
            settlementDecisionKey: refundRequest.settlementDecisionKey,
            amount: amount ?? refundRequest.amount,
            reason,
        }
        setPendingCommit(intent)
        refundSlotRef.current = slot
        setRefundDraft(null)
        await confirmRefundSubmit(undefined, intent)
    }

    /**
     * 对已存在的退款草稿打开提交确认。
     *
     * @param row 当前退款草稿投影。
     */
    function beginRefundSubmit(row: SupplierRefundRow) {
        if (refundSubmitUnknown) {
            if (row.refundId !== refundDraft?.refundId) {
                setActionError("请先核对上一笔退款提交结果，再提交其他单据。")
                return
            }
            setRefundSubmitOpen(true)
            return
        }
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
    async function confirmRefundSubmit(
        resubmissionReason?: string,
        intent?: NonNullable<typeof pendingCommit>,
    ) {
        const submitted = intent ?? pendingCommit
        const slot = refundSlotRef.current
        if (!slot || (!submitted && !refundDraft)) return
        const command =
            !submitted && refundDraft
                ? submitLedger.current.acquire<ExistingSubmitPayload>(
                      "submit",
                      "refund-resubmit",
                      {
                          refundId: refundDraft.refundId,
                          expectedVersion: refundDraft.baselineVersion,
                          resubmissionReason,
                      },
                  )
                : undefined
        const res = submitted
            ? await commitRefundMutation.mutateAsync({
                  ...submitted,
                  idempotencyKey: slot.key,
              })
            : await submitRefundMutation.mutateAsync({
                  ...command!.payload,
                  idempotencyKey: command!.idempotencyKey,
              })
        const remainsUnknown =
            !submitted &&
            res.status !== "succeeded" &&
            (refundSubmitUnknown || res.status === "unknown")
        if (!submitted) {
            submitLedger.current.settle(
                "submit",
                remainsUnknown ? "unknown" : res.status,
            )
            setRefundSubmitUnknown(remainsUnknown)
        }
        if (res.status !== "succeeded" && remainsUnknown) {
            setLastResult({
                status: "unknown",
                title: "退款结果待确认",
                description: res.message,
            })
            throw new SubmissionResultUnknownError(res.message)
        }
        if (res.status === "unknown") {
            setLastResult({
                status: "unknown",
                title: "退款结果待确认",
                description: res.message,
                operationId: res.idempotencyKey,
            })
            setRefundSubmitOpen(false)
            if (intent) throw new SubmissionResultUnknownError(res.message)
            return
        }
        if (res.status === "failed") {
            setActionError(res.message)
            if (submitted) setRefundSubmitOpen(false)
            throw new Error(res.message)
        }
        setLastResult({
            status: "succeeded",
            title:
                res.refund.settlementMethod === "SUPPLIER_CREDIT"
                    ? "留抵已提交审批"
                    : "退款已提交审批",
            description: "已按已绑定的审批流程启动审批，原付款保留。",
            reference: res.refund.refundNo,
            facts: [
                { label: "退款单号", value: res.refund.refundNo },
                { label: "当前状态", value: res.refund.statusLabel },
            ],
        })
        refundSlotRef.current = null
        setPendingCommit(null)
        setRefundDraft(res.refund)
        setRefundSubmitOpen(false)
        setRefundRequest(null)
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
        refundSubmitUnknown,
        endRefundRecovery: () => {
            submitLedger.current = new FormalCommandKeyLedger()
            setRefundSubmitUnknown(false)
            setRefundSubmitOpen(false)
            refundSlotRef.current = null
            setLastResult(null)
            if (refundDraft) openRefundPreview(refundDraft.refundId)
            setRefundDraft(null)
        },
        refundSubmitReason:
            submitLedger.current.peek<ExistingSubmitPayload>("submit")?.payload
                .resubmissionReason,
        setRefundSubmitOpen,
        refundDraftPending: false,
        refundSubmitPending:
            commitRefundMutation.isPending || submitRefundMutation.isPending,
    }
}
