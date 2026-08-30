"use client"

import * as React from "react"

import { useAppForm } from "@/components/form"
import {
    confirmSchema,
    sourceFixSchema,
} from "@/features/mall-sync/lib/presentation"
import {
    useConfirmMappingMutation,
    useReapplyMutation,
    useResolveUnknownReapplyMutation,
    useRequestSourceFixMutation,
} from "@/features/mall-sync/hooks/queries"
import type { PatchUrl } from "@/features/mall-sync/pages/hooks/use-mall-sync-url-state"
import type { MallSyncPageData } from "@/features/mall-sync/pages/hooks/use-mall-sync-page-data"
import type { MallSyncActionFeedback } from "@/features/mall-sync/pages/hooks/use-mall-sync-action-feedback"
import { useCommandIdentities } from "@/features/mall-sync/pages/hooks/use-command-identities"

export function useMallSyncMappingActions(
    data: MallSyncPageData,
    feedback: MallSyncActionFeedback,
    patchUrl: PatchUrl,
    advanceAfterConfirm = true,
    onTaskCompleted?: (workItemId: string) => void,
) {
    const { pageQuery, mappingTask, firstPhase, responsibilityStatus } = data
    const { setResult, setActionError } = feedback

    const { commandIdentity, clearIdentity } = useCommandIdentities()

    const confirmMutation = useConfirmMappingMutation()
    const sourceFixMutation = useRequestSourceFixMutation()
    const reapplyMutation = useReapplyMutation()
    const resolveReapply = useResolveUnknownReapplyMutation()

    const [selectedCandidateId, setSelectedCandidateId] = React.useState<
        string | null
    >(null)
    const [confirmOpen, setConfirmOpen] = React.useState(false)
    const [sourceFixOpen, setSourceFixOpen] = React.useState(false)

    // 切换映射任务时重置候选与动作错误；责任始终从服务端重取。
    React.useEffect(() => {
        setSelectedCandidateId(null)
        setActionError(null)
        // eslint-disable-next-line react-hooks/exhaustive-deps
    }, [mappingTask?.mappingTaskId])

    const confirmForm = useAppForm({
        defaultValues: { evidenceNote: "" },
        validators: { onChange: confirmSchema },
        onSubmit: async () => {
            setConfirmOpen(true)
        },
    })

    const sourceFixForm = useAppForm({
        defaultValues: {
            reasonCode: "SOURCE_FIELD_MISSING" as
                | "SOURCE_FIELD_MISSING"
                | "SOURCE_FIELD_CONFLICT"
                | "SOURCE_EVIDENCE_REQUIRED"
                | "OTHER",
            note: "",
            requestedEvidence: "",
        },
        validators: { onChange: sourceFixSchema },
        onSubmit: async ({ value }) => {
            await handleRequestSourceFix(
                value.reasonCode,
                value.note,
                value.requestedEvidence,
            )
        },
    })

    async function handleConfirm() {
        if (
            mappingTask?.ownerRoutingState !== "CONFIGURED" ||
            responsibilityStatus !== "assigned_to_me" ||
            !firstPhase
        )
            return
        const candidate = mappingTask.candidateTargets.find(
            (c) => c.objectId === selectedCandidateId,
        )
        if (!candidate || candidate.eligibility !== "ELIGIBLE") {
            setActionError("请选择可用的 ERP 候选（相似不自动确认）")
            return
        }
        const evidenceNote = String(
            confirmForm.getFieldValue("evidenceNote") ?? "",
        ).trim()
        const identity = commandIdentity(
            "confirm-mapping",
            mappingTask.mappingTaskId,
        )
        const res = await confirmMutation.mutateAsync({
            mappingTaskId: mappingTask.mappingTaskId,
            sourceSnapshotId: mappingTask.sourceSnapshotId,
            externalIdentityMapId: mappingTask.externalIdentityMapId,
            workItemId: mappingTask.workItem.workItemId,
            expectedTaskVersion: mappingTask.workItem.taskVersion,
            expectedSubjectVersion: mappingTask.workItem.subjectVersion,
            expectedMappingTaskVersion: mappingTask.lockVersion,
            mappingOperationId: identity.operationId,
            targetObjectType: candidate.objectType,
            targetObjectId: candidate.objectId,
            relationRole: mappingTask.mappingType,
            evidenceNote,
            executionStage: "FIRST_PHASE_MALL_OWNED",
            idempotencyKey: identity.idempotencyKey,
        })
        setConfirmOpen(false)
        if (res.status === "succeeded") {
            clearIdentity(identity.key)
            setResult({
                status: "succeeded",
                title: "映射已确认",
                description: res.message,
                facts: [
                    {
                        label: "已确认目标",
                        value: `${candidate.stableNo} ${candidate.label}`,
                    },
                ],
            })
            void pageQuery.refetch()
            onTaskCompleted?.(mappingTask.workItem.workItemId)
            const tasks = data.data?.mappingTasks ?? []
            const idx = tasks.findIndex(
                (t) => t.mappingTaskId === mappingTask.mappingTaskId,
            )
            const next = tasks[idx + 1]
            if (advanceAfterConfirm && next) {
                patchUrl({
                    view: "mapping",
                    mappingTaskId: next.mappingTaskId,
                    workItemId:
                        next.ownerRoutingState === "CONFIGURED"
                            ? next.workItem.workItemId
                            : null,
                })
            }
        } else {
            setActionError(res.message)
        }
    }

    async function handleRequestSourceFix(
        reasonCode: string,
        reasonText: string,
        requestedEvidence: string,
    ) {
        if (
            mappingTask?.ownerRoutingState !== "CONFIGURED" ||
            responsibilityStatus !== "assigned_to_me"
        ) {
            setActionError("当前责任不允许提交来源修复说明")
            return
        }
        const identity = commandIdentity(
            "request-source-fix",
            mappingTask.mappingTaskId,
        )
        const res = await sourceFixMutation.mutateAsync({
            mappingTaskId: mappingTask.mappingTaskId,
            sourceSnapshotId: mappingTask.sourceSnapshotId,
            workItemId: mappingTask.workItem.workItemId,
            expectedTaskVersion: mappingTask.workItem.taskVersion,
            expectedSubjectVersion: mappingTask.workItem.subjectVersion,
            expectedMappingTaskVersion: mappingTask.lockVersion,
            requestOperationId: identity.operationId,
            reasonCode,
            reasonText,
            requestedEvidence: requestedEvidence
                .split(/[，,\n]/)
                .map((value) => value.trim())
                .filter(Boolean),
            idempotencyKey: identity.idempotencyKey,
        })
        setSourceFixOpen(false)
        if (res.status === "succeeded") {
            clearIdentity(identity.key)
            setResult({
                status: "succeeded",
                title: "来源修复说明已记录",
                description: res.message,
                reference: res.mappingEvidenceEntryId,
            })
            void pageQuery.refetch()
        } else {
            setActionError(res.message)
        }
    }

    async function handleReapply() {
        if (!mappingTask || !firstPhase) return
        const identity = commandIdentity("reapply", mappingTask.mappingTaskId)
        const res = await reapplyMutation.mutateAsync({
            mappingTaskId: mappingTask.mappingTaskId,
            sourceSnapshotId: mappingTask.sourceSnapshotId,
            expectedMappingVersion: mappingTask.lockVersion,
            operationId: identity.operationId,
            executionStage: "FIRST_PHASE_MALL_OWNED",
            idempotencyKey: identity.idempotencyKey,
        })
        if (res.status === "succeeded") {
            clearIdentity(identity.key)
            setResult({
                status: "succeeded",
                title: "重新归集成功",
                description: res.message,
                reference: res.salesOrderNo,
            })
            void pageQuery.refetch()
        } else if (res.status === "unknown") {
            setResult({
                status: "unknown",
                title: "重新归集结果未知",
                description: res.message,
                stayOnItem: true,
                pendingIdempotencyKey: res.idempotencyKey,
                reference: res.operationId,
            })
            void pageQuery.refetch()
        } else {
            setActionError(res.message)
        }
    }

    async function handleResolveUnknownReapply() {
        if (!mappingTask?.reapplyOperation) return
        const res = await resolveReapply.mutateAsync({
            mappingTaskId: mappingTask.mappingTaskId,
            operationId: mappingTask.reapplyOperation.operationId,
            settle: true,
        })
        if (res.status === "succeeded") {
            setResult({
                status: "succeeded",
                title: "重新归集结果已确认",
                description: res.message,
                reference: res.salesOrderNo,
            })
        } else if (res.status === "unknown") {
            setResult({
                status: "unknown",
                title: "仍为结果未知",
                description: res.message,
                stayOnItem: true,
            })
        } else {
            setActionError(res.message)
        }
    }

    const canConfirmMapping =
        firstPhase &&
        mappingTask?.ownerRoutingState === "CONFIGURED" &&
        mappingTask.mappingTaskStatus === "PENDING" &&
        mappingTask.allowedActions.includes("CONFIRM_TARGET") &&
        responsibilityStatus === "assigned_to_me" &&
        !!selectedCandidateId &&
        !mappingTask.hasConflict

    return {
        selectedCandidateId,
        setSelectedCandidateId,
        confirmOpen,
        setConfirmOpen,
        sourceFixOpen,
        setSourceFixOpen,
        confirmForm,
        sourceFixForm,
        canConfirmMapping,
        confirmPending: confirmMutation.isPending,
        actionPending: confirmMutation.isPending,
        reapplyPending: reapplyMutation.isPending,
        handleConfirm,
        handleRequestSourceFix,
        handleReapply,
        handleResolveUnknownReapply,
    }
}
