"use client"

import * as React from "react"
import { useSelector } from "@tanstack/react-form"
import { z } from "zod"

import type { ResultState } from "@/components/business/feedback"
import { useAppForm } from "@/components/form"
import { useAccountProfileQuery } from "@/features/auth/queries"
import {
    useAppendEvidenceMutation,
    useRefreshTrialMutation,
    useResolveDifferenceMutation,
    useReviewDecisionMutation,
    useSettlementDetailQuery,
    useSubmitReviewMutation,
} from "@/features/supplier-settlements/hooks/queries"
import {
    blockerOf,
    newKey,
    outcomeToResult,
} from "@/features/supplier-settlements/lib/operations"
import { responsibilityOf } from "@/features/supplier-settlements/lib/settlement-responsibility"
import type { SettlementsUrlState } from "@/features/supplier-settlements/lib/url-state"
import type { DifferenceResolution } from "@/features/supplier-settlements/types"
import { getErrorMessage } from "@/lib/api/errors"
import { classifyFormalCommandError } from "@/lib/formal-command"

type CommandIdentity = {
    operationId: string
    idempotencyKey: string
}

const resolveSchema = z.object({
    resolution: z.enum([
        "SUPPLIER_ACCEPTED",
        "ERP_ACCEPTED",
        "COMPENSATED",
        "CLOSED_NO_ADJUSTMENT",
    ]),
    reasonCode: z.string().trim().min(1, "请选择原因码"),
})
const evidenceSchema = z.object({
    referenceId: z.string().trim().min(1, "请填写凭证编号或链接"),
    comment: z.string(),
})
const submitReviewSchema = z.object({
    reviewerUserId: z.string().trim().min(1, "请选择复核人"),
})
const rejectSchema = z.object({
    reasonCode: z.string().trim().min(1, "请选择驳回原因"),
    comment: z.string(),
})

/**
 * 结算中心的动作状态：详情查询、全部业务动作、动作结果与对话框开关。
 * 组件只消费返回值；所有请求仍走 TanStack Query 的 query/mutation。
 */
function useSettlementCenterActions({
    statementId,
    workItemId,
    urlState,
    patchUrl,
    onTaskCompleted,
    detailEnabled = true,
}: {
    statementId: string
    workItemId?: string
    urlState: SettlementsUrlState
    patchUrl: (patch: Partial<SettlementsUrlState>) => void
    onTaskCompleted?: (workItemId: string) => void
    detailEnabled?: boolean
}) {
    const detailQuery = useSettlementDetailQuery(
        statementId,
        workItemId,
        detailEnabled,
    )
    const refreshMutation = useRefreshTrialMutation()
    const resolveMutation = useResolveDifferenceMutation()
    const evidenceMutation = useAppendEvidenceMutation()
    const submitMutation = useSubmitReviewMutation()
    const decisionMutation = useReviewDecisionMutation()
    const profileQuery = useAccountProfileQuery()

    const [result, setResult] = React.useState<ResultState>(null)
    const [resolveOpen, setResolveOpen] = React.useState(false)
    const [evidenceOpen, setEvidenceOpen] = React.useState(false)
    const [submitOpen, setSubmitOpen] = React.useState(false)
    const [confirmOpen, setConfirmOpen] = React.useState(false)
    const [rejectOpen, setRejectOpen] = React.useState(false)
    const resultRef = React.useRef<HTMLDivElement | null>(null)
    const commandIdentities = React.useRef(new Map<string, CommandIdentity>())

    const data = detailQuery.data
    const allowed = new Set(data?.allowedActions ?? [])
    const blockers = data?.actionBlockers ?? []
    const responsibilityStatus = responsibilityOf(
        data?.workItem,
        profileQuery.data?.userid,
    )
    const activeDiff = data
        ? (data.differences.find((d) => d.differenceId === urlState.diff) ??
          data.differences[0] ??
          null)
        : null
    const submitBlocker = blockerOf(blockers, "SUBMIT_REVIEW")

    const resolveForm = useAppForm({
        defaultValues: {
            resolution: "ERP_ACCEPTED" as DifferenceResolution,
            reasonCode: "ACCEPT_BILL",
        },
        validators: { onChange: resolveSchema },
        onSubmit: ({ value }) => executeResolve(value),
    })
    const evidenceForm = useAppForm({
        defaultValues: { referenceId: "", comment: "" },
        validators: { onChange: evidenceSchema },
        onSubmit: ({ value }) => executeEvidence(value),
    })
    const submitReviewForm = useAppForm({
        defaultValues: { reviewerUserId: "" },
        validators: { onChange: submitReviewSchema },
        onSubmit: ({ value }) => executeSubmitReview(value),
    })
    const rejectForm = useAppForm({
        defaultValues: { reasonCode: "", comment: "" },
        validators: { onChange: rejectSchema },
        onSubmit: ({ value }) => executeReject(value),
    })
    const resolveValues = useSelector(
        resolveForm.store,
        (state) => state.values,
    )
    const evidenceValues = useSelector(
        evidenceForm.store,
        (state) => state.values,
    )
    const reviewerUserId = useSelector(
        submitReviewForm.store,
        (state) => state.values.reviewerUserId,
    )
    const rejectValues = useSelector(rejectForm.store, (state) => state.values)

    function idempotencyIdentity(kind: string, objectId: string) {
        const key = `${kind}:${objectId}`
        const existing = commandIdentities.current.get(key)
        if (existing) return { key, ...existing }
        const identity = {
            operationId: `w27:${kind}:${crypto.randomUUID()}`,
            idempotencyKey: `w27:${kind}:${crypto.randomUUID()}`,
        }
        commandIdentities.current.set(key, identity)
        return { key, ...identity }
    }

    async function onRefresh() {
        const st = data?.statement
        if (!st) return
        if (!st.sourceSnapshotHash) {
            setResult({
                status: "blocked",
                title: "刷新试算暂不可用",
                description: "本次结算的来源依据不完整，不能刷新数据。",
            })
            return
        }
        try {
            const outcome = await refreshMutation.mutateAsync({
                statementId: st.id,
                expectedLockVersion: st.lockVersion,
                expectedSourceSnapshotHash: st.sourceSnapshotHash,
                requestId: newKey("req"),
                idempotencyKey: newKey("refresh"),
            })
            setResult(outcomeToResult(outcome))
        } catch (error) {
            setResult({
                status: "rejected",
                title: "刷新试算未完成",
                description: getErrorMessage(error, "刷新失败，请稍后重试"),
            })
        }
    }

    async function executeResolve(value: {
        resolution: DifferenceResolution
        reasonCode: string
    }) {
        if (!data || !activeDiff) return
        const identity = idempotencyIdentity(
            "resolve-difference",
            activeDiff.differenceId,
        )
        try {
            const outcome = await resolveMutation.mutateAsync({
                statementId: data.statement.id,
                differenceId: activeDiff.differenceId,
                expectedLockVersion: data.statement.lockVersion,
                expectedDifferenceVersion: activeDiff.version,
                resolution: value.resolution,
                reasonCode: value.reasonCode,
                evidenceReferenceIds: activeDiff.evidence
                    .map((item) => item.referenceIds)
                    .flat(),
                operationId: identity.operationId,
                idempotencyKey: identity.idempotencyKey,
            })
            if (outcome.status !== "unknown") {
                commandIdentities.current.delete(identity.key)
            }
            setResult(outcomeToResult(outcome))
            if (outcome.status === "succeeded") setResolveOpen(false)
        } catch (error) {
            const isUnknown = classifyFormalCommandError(error) === "unknown"
            if (!isUnknown) {
                commandIdentities.current.delete(identity.key)
            }
            setResult({
                status: isUnknown ? "unknown" : "rejected",
                title: isUnknown ? "结论登记结果待确认" : "结论登记未完成",
                description: isUnknown
                    ? "网络超时或响应异常，原请求可能已生效。请先刷新结算单核对；再次提交将复用原请求号避免重复登记。"
                    : getErrorMessage(error, "提交失败，请稍后重试"),
            })
        }
    }

    async function executeEvidence(value: {
        referenceId: string
        comment: string
    }) {
        if (!data || !activeDiff) return
        const referenceId = value.referenceId.trim()
        if (!referenceId) {
            setResult({
                status: "blocked",
                title: "请填写凭证编号或链接",
                description: "请填写工单、附件或供应商确认记录的编号或链接。",
            })
            return
        }
        try {
            const outcome = await evidenceMutation.mutateAsync({
                statementId: data.statement.id,
                differenceId: activeDiff.differenceId,
                expectedDifferenceVersion: activeDiff.version,
                evidenceReferenceIds: [referenceId],
                opinionCode: "PROCUREMENT_NOTE",
                comment: value.comment,
                requestId: newKey("req"),
                idempotencyKey: newKey("ev"),
            })
            setResult(outcomeToResult(outcome))
            if (outcome.status === "succeeded") {
                setEvidenceOpen(false)
                evidenceForm.reset()
            }
        } catch (error) {
            setResult({
                status: "rejected",
                title: "证据保存未完成",
                description: getErrorMessage(error, "保存失败，请稍后重试"),
            })
        }
    }

    async function executeSubmitReview(value: { reviewerUserId: string }) {
        if (!data) return
        const st = data.statement
        if (!st.subjectHash || !data.reviewSubmissionPolicy) {
            setResult({
                status: "blocked",
                title: "提交复核暂不可用",
                description:
                    "本次复核的数据版本或截止规则不完整，不能提交复核。",
            })
            return
        }
        const normalizedReviewerUserId = value.reviewerUserId.trim()
        if (!normalizedReviewerUserId) {
            setResult({
                status: "blocked",
                title: "请选择复核人",
                description: "提交复核前必须明确指定本次复核任务的责任人。",
            })
            return
        }
        const identity = idempotencyIdentity("submit-review", st.id)
        try {
            const outcome = await submitMutation.mutateAsync({
                statementId: st.id,
                expectedLockVersion: st.lockVersion,
                subjectHash: st.subjectHash,
                refreshCutoffPolicyId:
                    data.reviewSubmissionPolicy.refreshCutoffPolicyId,
                expectedRefreshCutoffPolicyVersion:
                    data.reviewSubmissionPolicy.version,
                reviewerUserId: normalizedReviewerUserId,
                operationId: identity.operationId,
                idempotencyKey: identity.idempotencyKey,
            })
            if (outcome.status !== "unknown") {
                commandIdentities.current.delete(identity.key)
            }
            setResult(outcomeToResult(outcome))
            if (outcome.status === "succeeded") {
                setSubmitOpen(false)
                submitReviewForm.reset()
                patchUrl({ section: "review" })
            }
        } catch (error) {
            const isUnknown = classifyFormalCommandError(error) === "unknown"
            if (!isUnknown) {
                commandIdentities.current.delete(identity.key)
            }
            setResult({
                status: isUnknown ? "unknown" : "rejected",
                title: isUnknown ? "提交复核结果待确认" : "提交复核未完成",
                description: isUnknown
                    ? "网络连接中断或响应超时，提交复核结果尚未确认。请先刷新核对当前状态；再次提交将沿用原请求避免重复提交。"
                    : getErrorMessage(error, "提交复核失败，请稍后重试"),
            })
            throw error
        }
    }

    async function onConfirm() {
        if (!data) return
        const workItem = data.workItem
        if (!workItem) {
            setResult({
                status: "blocked",
                title: "无复核任务",
                description:
                    "当前待办与该结算单不匹配，请从工作台待办列表重新进入处理。",
            })
            return
        }
        if (responsibilityStatus !== "assigned_to_me") return
        const identity = idempotencyIdentity(
            "confirm-review",
            workItem.workItemId,
        )
        try {
            const outcome = await decisionMutation.mutateAsync({
                statementId: data.statement.id,
                workItemId: workItem.workItemId,
                expectedTaskVersion: workItem.taskVersion,
                expectedSubjectVersion: workItem.subjectVersion,
                expectedLockVersion: data.statement.lockVersion,
                action: "CONFIRM",
                operationId: identity.operationId,
                idempotencyKey: identity.idempotencyKey,
            })
            if (outcome.status !== "unknown") {
                commandIdentities.current.delete(identity.key)
            }
            setResult(outcomeToResult(outcome))
            if (outcome.status === "succeeded") {
                setConfirmOpen(false)
                patchUrl({ section: "payable" })
                onTaskCompleted?.(workItem.workItemId)
            }
        } catch (error) {
            const isUnknown = classifyFormalCommandError(error) === "unknown"
            if (!isUnknown) {
                commandIdentities.current.delete(identity.key)
            }
            setResult({
                status: isUnknown ? "unknown" : "rejected",
                title: isUnknown ? "确认结算结果待确认" : "确认结算未完成",
                description: isUnknown
                    ? "确认结算响应超时，系统可能已入账并生成应付。请先刷新结算单核对；再次提交将沿用原请求避免重复入账。"
                    : getErrorMessage(error, "确认结算失败，请稍后重试"),
            })
            throw error
        }
    }

    async function executeReject(value: {
        reasonCode: string
        comment: string
    }) {
        if (!data) return
        const workItem = data.workItem
        if (!workItem || responsibilityStatus !== "assigned_to_me") return
        const identity = idempotencyIdentity(
            "reject-review",
            workItem.workItemId,
        )
        try {
            const outcome = await decisionMutation.mutateAsync({
                statementId: data.statement.id,
                workItemId: workItem.workItemId,
                expectedTaskVersion: workItem.taskVersion,
                expectedSubjectVersion: workItem.subjectVersion,
                expectedLockVersion: data.statement.lockVersion,
                action: "REJECT",
                operationId: identity.operationId,
                idempotencyKey: identity.idempotencyKey,
                reasonCode: value.reasonCode,
                comment: value.comment.trim() || undefined,
            })
            if (outcome.status !== "unknown") {
                commandIdentities.current.delete(identity.key)
            }
            setResult(outcomeToResult(outcome))
            if (
                outcome.status === "rejected" ||
                outcome.status === "succeeded"
            ) {
                setRejectOpen(false)
                rejectForm.reset()
                onTaskCompleted?.(workItem.workItemId)
            }
        } catch (error) {
            const isUnknown = classifyFormalCommandError(error) === "unknown"
            if (!isUnknown) {
                commandIdentities.current.delete(identity.key)
            }
            setResult({
                status: isUnknown ? "unknown" : "rejected",
                title: isUnknown ? "驳回复核结果待确认" : "驳回复核未完成",
                description: isUnknown
                    ? "驳回请求响应超时，结果尚未确认。请先刷新核对当前状态；再次提交将沿用原请求避免重复提交。"
                    : getErrorMessage(error, "驳回复核失败，请稍后重试"),
            })
        }
    }

    function onResolve() {
        return resolveForm.handleSubmit()
    }

    function onEvidence() {
        return evidenceForm.handleSubmit()
    }

    function onSubmitReview() {
        return submitReviewForm.handleSubmit()
    }

    function onReject() {
        return rejectForm.handleSubmit()
    }

    return {
        detailQuery,
        data,
        allowed,
        blockers,
        submitBlocker,
        activeDiff,
        responsibilityStatus,
        refreshMutation,
        resolveMutation,
        evidenceMutation,
        submitMutation,
        decisionMutation,
        result,
        clearResult: () => setResult(null),
        resultRef,
        resolveOpen,
        setResolveOpen,
        evidenceOpen,
        setEvidenceOpen,
        submitOpen,
        setSubmitOpen,
        confirmOpen,
        setConfirmOpen,
        rejectOpen,
        setRejectOpen,
        resolution: resolveValues.resolution,
        setResolution: (resolution: DifferenceResolution) =>
            resolveForm.setFieldValue("resolution", resolution),
        reasonCode: resolveValues.reasonCode,
        setReasonCode: (reasonCode: string) =>
            resolveForm.setFieldValue("reasonCode", reasonCode),
        evidenceComment: evidenceValues.comment,
        setEvidenceComment: (comment: string) =>
            evidenceForm.setFieldValue("comment", comment),
        evidenceReferenceId: evidenceValues.referenceId,
        setEvidenceReferenceId: (referenceId: string) =>
            evidenceForm.setFieldValue("referenceId", referenceId),
        rejectReason: rejectValues.reasonCode,
        setRejectReason: (reasonCode: string) =>
            rejectForm.setFieldValue("reasonCode", reasonCode),
        rejectComment: rejectValues.comment,
        setRejectComment: (comment: string) =>
            rejectForm.setFieldValue("comment", comment),
        reviewerUserId,
        setReviewerUserId: (userId: string) =>
            submitReviewForm.setFieldValue("reviewerUserId", userId),
        onRefresh,
        onResolve,
        onEvidence,
        onSubmitReview,
        onConfirm,
        onReject,
    }
}

export { useSettlementCenterActions }
