"use client"

import * as React from "react"

import type { ResultState } from "@/components/business/feedback"
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
import { useWorkItemResponsibilityMutation } from "@/features/work-items"
import { getErrorMessage } from "@/lib/api/errors"

type CommandIdentity = {
    operationId: string
    idempotencyKey: string
}

/**
 * 结算中心的动作状态：详情查询、全部业务动作、动作结果与对话框开关。
 * 组件只消费返回值；所有请求仍走 TanStack Query 的 query/mutation。
 */
function useSettlementCenterActions({
    statementId,
    workItemId,
    urlState,
    patchUrl,
}: {
    statementId: string
    workItemId?: string
    urlState: SettlementsUrlState
    patchUrl: (patch: Partial<SettlementsUrlState>) => void
}) {
    const detailQuery = useSettlementDetailQuery(statementId, workItemId)
    const refreshMutation = useRefreshTrialMutation()
    const resolveMutation = useResolveDifferenceMutation()
    const evidenceMutation = useAppendEvidenceMutation()
    const submitMutation = useSubmitReviewMutation()
    const decisionMutation = useReviewDecisionMutation()
    const profileQuery = useAccountProfileQuery()
    const responsibilityMutation = useWorkItemResponsibilityMutation()

    const [result, setResult] = React.useState<ResultState>(null)
    const [resolveOpen, setResolveOpen] = React.useState(false)
    const [evidenceOpen, setEvidenceOpen] = React.useState(false)
    const [submitOpen, setSubmitOpen] = React.useState(false)
    const [confirmOpen, setConfirmOpen] = React.useState(false)
    const [rejectOpen, setRejectOpen] = React.useState(false)
    const [resolution, setResolution] =
        React.useState<DifferenceResolution>("ERP_ACCEPTED")
    const [reasonCode, setReasonCode] = React.useState("ACCEPT_BILL")
    const [evidenceComment, setEvidenceComment] = React.useState("")
    const [evidenceReferenceId, setEvidenceReferenceId] = React.useState("")
    const [rejectReason, setRejectReason] = React.useState("")
    const resultRef = React.useRef<HTMLDivElement | null>(null)
    const commandIdentities = React.useRef(
        new Map<string, CommandIdentity>(),
    )

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

    async function onResolve() {
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
                resolution,
                reasonCode,
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
            setResult({
                status: "rejected",
                title: "结论登记未完成",
                description: getErrorMessage(error, "提交失败，请稍后重试"),
            })
        }
    }

    async function onEvidence() {
        if (!data || !activeDiff) return
        if (!evidenceReferenceId.trim()) {
            setResult({
                status: "blocked",
                title: "缺少正式证据引用",
                description: "请填写工单、附件或供应商确认记录的正式引用。",
            })
            return
        }
        try {
            const outcome = await evidenceMutation.mutateAsync({
                statementId: data.statement.id,
                differenceId: activeDiff.differenceId,
                expectedDifferenceVersion: activeDiff.version,
                evidenceReferenceIds: [evidenceReferenceId.trim()],
                opinionCode: "PROCUREMENT_NOTE",
                comment: evidenceComment,
                requestId: newKey("req"),
                idempotencyKey: newKey("ev"),
            })
            setResult(outcomeToResult(outcome))
            if (outcome.status === "succeeded") {
                setEvidenceOpen(false)
                setEvidenceComment("")
                setEvidenceReferenceId("")
            }
        } catch (error) {
            setResult({
                status: "rejected",
                title: "证据保存未完成",
                description: getErrorMessage(error, "保存失败，请稍后重试"),
            })
        }
    }

    async function onSubmitReview() {
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
        const identity = idempotencyIdentity("submit-review", st.id)
        const outcome = await submitMutation.mutateAsync({
            statementId: st.id,
            expectedLockVersion: st.lockVersion,
            subjectHash: st.subjectHash,
            refreshCutoffPolicyId:
                data.reviewSubmissionPolicy.refreshCutoffPolicyId,
            expectedRefreshCutoffPolicyVersion:
                data.reviewSubmissionPolicy.version,
            operationId: identity.operationId,
            idempotencyKey: identity.idempotencyKey,
        })
        if (outcome.status !== "unknown") {
            commandIdentities.current.delete(identity.key)
        }
        setResult(outcomeToResult(outcome))
        if (outcome.status === "succeeded") {
            setSubmitOpen(false)
            patchUrl({ section: "review" })
        }
    }

    async function onConfirm() {
        if (!data) return
        const workItem = data.workItem
        if (!workItem) {
            setResult({
                status: "blocked",
                title: "无复核任务",
                description: "未查询到正式复核任务，禁止按结算单状态直接确认。",
            })
            return
        }
        if (responsibilityStatus !== "assigned_to_me") return
        const identity = idempotencyIdentity(
            "confirm-review",
            workItem.workItemId,
        )
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
        }
    }

    async function onReject() {
        if (!data) return
        const workItem = data.workItem
        if (!workItem || responsibilityStatus !== "assigned_to_me") return
        const identity = idempotencyIdentity(
            "reject-review",
            workItem.workItemId,
        )
        const outcome = await decisionMutation.mutateAsync({
            statementId: data.statement.id,
            workItemId: workItem.workItemId,
            expectedTaskVersion: workItem.taskVersion,
            expectedSubjectVersion: workItem.subjectVersion,
            expectedLockVersion: data.statement.lockVersion,
            action: "REJECT",
            operationId: identity.operationId,
            idempotencyKey: identity.idempotencyKey,
            reasonCode: rejectReason || "NEEDS_MORE_EVIDENCE",
        })
        if (outcome.status !== "unknown") {
            commandIdentities.current.delete(identity.key)
        }
        setResult(outcomeToResult(outcome))
        if (outcome.status === "rejected" || outcome.status === "succeeded") {
            setRejectOpen(false)
        }
    }

    async function onStartProcessing() {
        if (!data?.workItem) return
        const workItem = data.workItem
        const identity = idempotencyIdentity(
            "start-processing",
            workItem.workItemId,
        )
        try {
            const response = await responsibilityMutation.mutateAsync({
                kind: "START_PROCESSING",
                workItemId: workItem.workItemId,
                expectedTaskVersion: workItem.taskVersion,
                idempotencyKey: identity.idempotencyKey,
            })
            commandIdentities.current.delete(identity.key)
            await detailQuery.refetch()
            setResult({
                status: "succeeded",
                title: "已开始处理复核",
                description: "正式任务已建立当前用户个人责任。",
                reference: response.id,
            })
        } catch (error) {
            setResult({
                status: "rejected",
                title: "开始处理未完成",
                description: getErrorMessage(error, "开始处理失败，请刷新任务"),
            })
        }
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
        responsibilityMutation,
        result,
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
        resolution,
        setResolution,
        reasonCode,
        setReasonCode,
        evidenceComment,
        setEvidenceComment,
        evidenceReferenceId,
        setEvidenceReferenceId,
        rejectReason,
        setRejectReason,
        onRefresh,
        onResolve,
        onEvidence,
        onSubmitReview,
        onConfirm,
        onReject,
        onStartProcessing,
    }
}

export type SettlementCenterActions = ReturnType<
    typeof useSettlementCenterActions
>

export { useSettlementCenterActions }
