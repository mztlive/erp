"use client"

import * as React from "react"

import { getErrorMessage } from "@/lib/api/errors"
import { type ResultState as SharedResultState } from "@/components/business/feedback"
import type {
    ApproveConclusion,
    CardFundsReviewDecision,
    CardFundsReviewItemView,
    ConfirmMode,
    FormalOutcome,
    WorkItemAction,
} from "@/features/card-funds-review/types"
import { APPROVE_CONCLUSION_LABEL } from "@/features/card-funds-review/types"
import type { RejectReviewValue } from "../components/reject-review-dialog"
import type { useCompleteCardFundsMutation } from "./queries"

type ResultState = SharedResultState<FormalOutcome>

/**
 * 提交复核结论（通过/从 0 起/驳回）：构造 decision、调用提交 mutation、
 * 展示结果条；autoNext 时短暂停留后前进。
 */
export function useCardFundsDecisionSubmission(args: {
    task: CardFundsReviewItemView | undefined
    autoNext: boolean
    assertAllowed: (action: WorkItemAction) => void
    buildDecisionBase: (reviewResult: "APPROVED" | "REJECTED") => {
        receivableAccountId: string
        expectedAccountSeq: number
        expectedAccountDomainVersion: string
        expectedReviewChainTailId?: string
        expectedReviewChainVersion: string
        expectedNextReviewNo: number
        expectedSalesOrderRevisionId: string
        expectedFundsFactVersion: string
        reviewType: "OPENING" | "SYNC_DELTA"
        evidenceDocumentIds: string[]
        evidenceReferences: string[]
        comment: string | undefined
        reviewResult: "APPROVED" | "REJECTED"
    }
    advanceIfNeeded: (shouldAdvance: boolean) => void
    completeMutation: ReturnType<typeof useCompleteCardFundsMutation>
    setConfirmMode: React.Dispatch<React.SetStateAction<ConfirmMode>>
    setLastResult: React.Dispatch<React.SetStateAction<ResultState>>
    setActionError: React.Dispatch<React.SetStateAction<string | null>>
}): {
    runApprove: (
        conclusion: ApproveConclusion,
        advance: boolean,
    ) => Promise<void>
    submitReject: (value: RejectReviewValue) => Promise<void>
} {
    const {
        task,
        autoNext,
        assertAllowed,
        buildDecisionBase,
        advanceIfNeeded,
        completeMutation,
        setConfirmMode,
        setLastResult,
        setActionError,
    } = args

    const runApprove = React.useCallback(
        async (conclusion: ApproveConclusion, advance: boolean) => {
            if (!task) return
            setActionError(null)
            try {
                assertAllowed(
                    conclusion === "NO_HISTORY_FROM_ZERO"
                        ? "CONFIRM_ZERO"
                        : "APPROVE",
                )
                const base = buildDecisionBase("APPROVED")
                const decision: CardFundsReviewDecision = {
                    ...base,
                    reviewResult: "APPROVED",
                    conclusion,
                }
                const response = await completeMutation.mutateAsync({
                    workItemId: task.workItem.workItemId,
                    expectedTaskVersion: task.workItem.taskVersion,
                    expectedSubjectVersion: task.workItem.subjectVersion,
                    decision,
                    idempotencyKey: `w13:${task.workItem.workItemId}:${task.workItem.taskVersion}:approve:${conclusion}`,
                })
                setConfirmMode(null)

                if (response.status !== "succeeded") {
                    if (response.status === "failed") {
                        setActionError(response.message)
                        throw new Error(response.message)
                    }
                    setLastResult({
                        status: "unknown",
                        title: "复核结果待确认",
                        description: response.message,
                        reference: response.idempotencyKey,
                        stayOnItem: true,
                    })
                    return
                }
                if (response.outcome.kind !== "APPROVED") return
                const biz = response.outcome.business
                setLastResult({
                    status: "succeeded",
                    title: `复核通过 · 复核号 ${biz.reviewNo}`,
                    description: `${APPROVE_CONCLUSION_LABEL[biz.conclusion as ApproveConclusion]} · ${advance && autoNext ? "自动下一项" : "手动继续"}`,
                    reference: biz.operationId,
                    outcome: response.outcome,
                    stayOnItem: !(advance && autoNext),
                })
                // 成功先展示固定复核号；若 autoNext 则短暂停留后前进
                if (advance && autoNext) {
                    window.setTimeout(() => advanceIfNeeded(true), 2200)
                }
            } catch (error) {
                setActionError(getErrorMessage(error, "完成失败"))
            }
        },
        [
            advanceIfNeeded,
            autoNext,
            buildDecisionBase,
            completeMutation,
            assertAllowed,
            setActionError,
            setConfirmMode,
            setLastResult,
            task,
        ],
    )

    const submitReject = React.useCallback(
        async (value: RejectReviewValue) => {
            if (!task) return
            setActionError(null)
            try {
                assertAllowed("REJECT")
                const base = buildDecisionBase("REJECTED")
                const decision: CardFundsReviewDecision = {
                    ...base,
                    reviewResult: "REJECTED",
                    conclusion: "REJECTED",
                    reasonCode: value.reasonCode,
                    comment: value.comment.trim(),
                    evidenceDocumentIds: base.evidenceDocumentIds,
                    evidenceReferences: base.evidenceReferences,
                }
                const response = await completeMutation.mutateAsync({
                    workItemId: task.workItem.workItemId,
                    expectedTaskVersion: task.workItem.taskVersion,
                    expectedSubjectVersion: task.workItem.subjectVersion,
                    decision,
                    idempotencyKey: `w13:${task.workItem.workItemId}:${task.workItem.taskVersion}:reject`,
                })
                setConfirmMode(null)
                if (response.status !== "succeeded") {
                    if (response.status === "failed") {
                        setActionError(response.message)
                    } else {
                        setLastResult({
                            status: "unknown",
                            title: "驳回结果待确认",
                            description: response.message,
                            reference: response.idempotencyKey,
                            stayOnItem: true,
                        })
                    }
                    return
                }
                if (response.outcome.kind !== "REJECTED") return
                const biz = response.outcome.business
                setLastResult({
                    status: "rejected",
                    title: `已驳回 · 复核号 ${biz.reviewNo}`,
                    description: biz.followUpConfiguration.collaborationMessage,
                    reference: biz.operationId,
                    outcome: response.outcome,
                    stayOnItem: !autoNext,
                })
                if (autoNext) {
                    window.setTimeout(() => advanceIfNeeded(true), 2200)
                }
            } catch (error) {
                setActionError(getErrorMessage(error, "驳回失败"))
            }
        },
        [
            advanceIfNeeded,
            autoNext,
            buildDecisionBase,
            completeMutation,
            assertAllowed,
            setActionError,
            setConfirmMode,
            setLastResult,
            task,
        ],
    )

    return { runApprove, submitReject }
}
