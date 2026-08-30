"use client"

import * as React from "react"

import { type ResultState as SharedResultState } from "@/components/business/feedback"
import type {
    CardFundsReviewItemView,
    CardFundsReviewQueueView,
    ConfirmMode,
    FormalOutcome,
    ReviewType,
    WorkItemAction,
} from "@/features/card-funds-review/types"
import {
    useCompleteCardFundsMutation,
    useRegisterInvoiceMutation,
    useRegisterReceiptMutation,
} from "./queries"
import { useCardFundsDecisionSubmission } from "./use-card-funds-decision-submission"
import { useCardFundsReviewForms } from "./use-card-funds-review-forms"
import { useCardFundsReviewKeyboard } from "./use-card-funds-review-keyboard"
import { useCardFundsRegistration } from "./use-card-funds-registration"

type ResultState = SharedResultState<FormalOutcome>

type ReplaceUrlFn = (patch: Record<string, string | null | undefined>) => void

/**
 * 复核页核心工作流：队列导航、快捷键、提交结论和登记回款/发票。
 */
export function useCardFundsReviewWorkflow(args: {
    task: CardFundsReviewItemView | undefined
    tasks: readonly CardFundsReviewItemView[]
    context: CardFundsReviewQueueView["context"] | undefined
    currentIndex: number
    queueContextId: string
    autoNext: boolean
    replaceUrl: ReplaceUrlFn
    setSearchInput: React.Dispatch<React.SetStateAction<string>>
    onTaskCompleted?: (workItemId: string, preferredWorkItemId?: string) => void
}) {
    const {
        task,
        tasks,
        context,
        currentIndex,
        queueContextId,
        autoNext,
        replaceUrl,
        setSearchInput,
        onTaskCompleted,
    } = args

    const completeMutation = useCompleteCardFundsMutation()
    const registerReceiptMutation = useRegisterReceiptMutation()
    const registerInvoiceMutation = useRegisterInvoiceMutation()

    const forms = useCardFundsReviewForms(task)
    const {
        evidenceRef,
        setEvidenceRef,
        evidenceDocId,
        setEvidenceDocId,
        comment,
        setComment,
        evidenceOk,
        evidenceDirty,
        setEvidenceDirty,
        receiptForm,
        setReceiptForm,
        invoiceForm,
        setInvoiceForm,
        allocLines,
        setAllocLines,
        allocationMode,
        setAllocationMode,
        openAllocation,
    } = forms

    const [confirmMode, setConfirmMode] = React.useState<ConfirmMode>(null)
    const [lastResult, setLastResult] = React.useState<ResultState>(null)
    const [actionError, setActionError] = React.useState<string | null>(null)
    const [pendingNav, setPendingNav] = React.useState<number | null>(null)
    const [keyHint, setKeyHint] = React.useState<string | null>(null)

    // 切换任务时清除上一条任务的错误提示（表单草稿由 forms hook 重置）
    React.useEffect(() => {
        if (!task) return
        setActionError(null)
    }, [task])

    const goToWorkItem = React.useCallback(
        (workItemId: string | undefined | null) => {
            setLastResult(null)
            setActionError(null)
            replaceUrl({
                currentWorkItemId: workItemId ?? null,
                queueContextId,
            })
        },
        [queueContextId, replaceUrl],
    )

    const clearFilters = React.useCallback(() => {
        setSearchInput("")
        replaceUrl({
            type: null,
            status: null,
            due: null,
            q: null,
            currentWorkItemId: null,
        })
    }, [replaceUrl, setSearchInput])

    const neighborId = React.useCallback(
        (delta: number) => {
            const idx = currentIndex + delta
            return tasks[idx]?.workItem.workItemId
        },
        [currentIndex, tasks],
    )

    const assertAllowed = React.useCallback(
        (action: WorkItemAction) => {
            if (!task) throw new Error("无当前任务")
            if (!task.workItem.allowedActions.includes(action)) {
                throw new Error("当前责任或业务版本已变化，请刷新后再处理")
            }
        },
        [task],
    )

    const buildDecisionBase = React.useCallback(
        (reviewResult: "APPROVED" | "REJECTED") => {
            if (!task) throw new Error("无当前任务")
            const evidenceDocumentIds = evidenceDocId.trim()
                ? [evidenceDocId.trim()]
                : []
            const evidenceReferences = evidenceRef.trim()
                ? [evidenceRef.trim()]
                : []
            if (
                !task.reviewChain.chainVersion ||
                task.reviewChain.nextReviewNo <= 0 ||
                !task.currentSalesOrderRevisionId ||
                !task.fundsFactVersion
            ) {
                throw new Error(
                    "复核记录、销售版本或票款记录不完整，已禁止提交",
                )
            }
            if (
                evidenceDocumentIds.length === 0 &&
                evidenceReferences.length === 0
            ) {
                throw new Error("正式复核必须提供凭证编号或证据说明")
            }
            return {
                receivableAccountId: task.account.id,
                expectedAccountSeq: task.account.accountSeq,
                expectedAccountDomainVersion: task.account.domainVersion,
                expectedReviewChainTailId: task.reviewChain.tailReviewId,
                expectedReviewChainVersion: task.reviewChain.chainVersion,
                expectedNextReviewNo: task.reviewChain.nextReviewNo,
                expectedSalesOrderRevisionId: task.currentSalesOrderRevisionId,
                expectedFundsFactVersion: task.fundsFactVersion,
                reviewType: task.reviewType as ReviewType,
                evidenceDocumentIds,
                evidenceReferences,
                comment: comment.trim() || undefined,
                reviewResult,
            }
        },
        [comment, evidenceDocId, evidenceRef, task],
    )

    const advanceIfNeeded = React.useCallback(
        (shouldAdvance: boolean) => {
            if (!shouldAdvance) return
            const nextId =
                context?.nextWorkItemId ??
                neighborId(1) ??
                tasks.find(
                    (t) => t.workItem.workItemId !== task?.workItem.workItemId,
                )?.workItem.workItemId
            if (nextId) goToWorkItem(nextId)
            else replaceUrl({ currentWorkItemId: null, queueContextId })
        },
        [
            context?.nextWorkItemId,
            goToWorkItem,
            neighborId,
            queueContextId,
            replaceUrl,
            task?.workItem.workItemId,
            tasks,
        ],
    )

    const { runApprove, submitReject } = useCardFundsDecisionSubmission({
        task,
        autoNext,
        assertAllowed,
        buildDecisionBase,
        advanceIfNeeded,
        completeMutation,
        setConfirmMode,
        setLastResult,
        setActionError,
        onTaskCompleted,
    })

    const { submitReceipt, submitInvoice } = useCardFundsRegistration({
        task,
        receiptForm,
        invoiceForm,
        allocLines,
        evidenceRef,
        setAllocationMode,
        assertAllowed,
        registerReceiptMutation,
        registerInvoiceMutation,
        setLastResult,
        setActionError,
    })

    const onShortcutSubmit = React.useCallback(() => {
        if (task?.workItem.allowedActions.includes("APPROVE")) {
            if (!evidenceOk) {
                setKeyHint(
                    "请先填写凭证编号或证据说明；证据将随正式决定一并提交。",
                )
                window.setTimeout(() => setKeyHint(null), 3000)
                return
            }
            const zeroOk =
                task.reviewType === "OPENING" &&
                Number(task.account.settledTotal) === 0 &&
                Number(task.account.invoicedTotal) === 0
            setConfirmMode(
                zeroOk
                    ? { kind: "zero", advance: autoNext }
                    : {
                          kind: "approve",
                          conclusion: "RECORDED_FACTS_RECONCILED",
                          advance: autoNext,
                      },
            )
        } else if (task) {
            setKeyHint("当前责任或任务状态不允许提交，请刷新后重试。")
            window.setTimeout(() => setKeyHint(null), 3000)
        }
    }, [autoNext, evidenceOk, task])

    useCardFundsReviewKeyboard({
        task,
        evidenceOk,
        evidenceDirty,
        neighborId,
        goToWorkItem,
        onShortcutSubmit,
        setPendingNav,
    })

    const formalPending =
        completeMutation.isPending || lastResult?.status === "unknown"

    const responsibilityStatus = task
        ? task.workItem.workItemStatus === "COMPLETED"
            ? ("completed" as const)
            : task.workItem.workItemStatus === "CLOSED"
              ? ("closed" as const)
              : task.workItem.allowedActions.some((action) =>
                      ["APPROVE", "REJECT", "CONFIRM_ZERO"].includes(action),
                  )
                ? ("assigned_to_me" as const)
                : task.workItem.actionBlockers.length > 0
                  ? ("blocked" as const)
                  : task.workItem.ownerUser
                    ? ("assigned_to_other" as const)
                    : ("blocked" as const)
        : ("blocked" as const)

    const canConfirmZero = Boolean(
        task?.workItem.allowedActions.includes("CONFIRM_ZERO") &&
        task?.reviewType === "OPENING" &&
        Number(task.account.settledTotal) === 0 &&
        Number(task.account.invoicedTotal) === 0,
    )

    const allocatedSum = allocLines.reduce(
        (s, l) => s + (Number(l.amount) || 0),
        0,
    )
    const allocTarget =
        allocationMode === "receipt"
            ? Number(receiptForm.grossAmount) || 0
            : Number(invoiceForm.grossAmount) || 0

    return {
        confirmMode,
        setConfirmMode,
        lastResult,
        actionError,
        setActionError,
        pendingNav,
        setPendingNav,
        keyHint,
        evidenceOk,
        evidenceDirty,
        evidenceRef,
        setEvidenceRef,
        evidenceDocId,
        setEvidenceDocId,
        comment,
        setComment,
        setEvidenceDirty,
        receiptForm,
        setReceiptForm,
        invoiceForm,
        setInvoiceForm,
        allocLines,
        setAllocLines,
        allocationMode,
        setAllocationMode,
        allocTarget,
        allocatedSum,
        receiptPending: registerReceiptMutation.isPending,
        invoicePending: registerInvoiceMutation.isPending,
        completePending: completeMutation.isPending,
        formalPending,
        responsibilityStatus,
        canConfirmZero,
        goToWorkItem,
        clearFilters,
        neighborId,
        openAllocation,
        submitReceipt,
        submitInvoice,
        runApprove,
        submitReject,
    }
}
