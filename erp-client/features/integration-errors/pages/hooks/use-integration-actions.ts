"use client"

import * as React from "react"

import { getErrorMessage } from "@/lib/api/errors"
import { useWorkItemResponsibilityMutation } from "@/features/work-items"
import type { TerminalConfirm } from "../../components/terminal-action-dialog"
import {
    useDirectReconciliationMutation,
    useIntegrationActionMutation,
    useResolveIntegrationMutation,
} from "../../hooks/queries"
import type {
    IntegrationActionKind,
    IntegrationFormalResult,
    IntegrationResolutionItemView,
} from "../../types"
import { createCommandIdentityStore } from "../lib/command-identity"
import { deriveResponsibilityStatus } from "../lib/selection"
import { useIntegrationActionFocus } from "./use-integration-action-focus"
import {
    useIntegrationDirectActions,
    type IntegrationTaskActionKind,
} from "./use-integration-direct-actions"
import { useIntegrationResponsibilityCommands } from "./use-integration-responsibility-commands"

export type { IntegrationTaskActionKind } from "./use-integration-direct-actions"

export function useIntegrationActions({
    item,
    focusMode,
    autoNext,
    lastResult,
    setLastResult,
    setActionError,
    userId,
    refetch,
    goToItem,
    neighbor,
    onTaskCompleted,
}: {
    item: IntegrationResolutionItemView | undefined
    focusMode: boolean
    autoNext: boolean
    lastResult: IntegrationFormalResult | null
    setLastResult: (result: IntegrationFormalResult | null) => void
    setActionError: (error: string | null) => void
    userId: string | undefined
    refetch: () => void
    goToItem: (next: IntegrationResolutionItemView | null | undefined) => void
    neighbor: (delta: number) => IntegrationResolutionItemView | null
    onTaskCompleted?: (workItemId: string) => void
}) {
    const responsibilityMutation = useWorkItemResponsibilityMutation()
    const actionMutation = useIntegrationActionMutation()
    const resolveMutation = useResolveIntegrationMutation()
    const directMutation = useDirectReconciliationMutation()

    const [replacementTaskId, setReplacementTaskId] = React.useState("")
    const [reconReasonId, setReconReasonId] = React.useState("")
    const [comment, setComment] = React.useState("")
    const [terminalConfirm, setTerminalConfirm] =
        React.useState<TerminalConfirm | null>(null)

    const commandIdentities = React.useRef(createCommandIdentityStore())

    const { resultRef, headingRef, actionZoneRef, focusFirstAction } =
        useIntegrationActionFocus({ item, lastResult })

    const refresh = React.useCallback(() => {
        refetch()
        setLastResult(null)
    }, [refetch, setLastResult])

    const afterResult = React.useCallback(
        (result: IntegrationFormalResult) => {
            setLastResult(result)
            const workItemId = item?.workItem?.workItemId
            if (
                workItemId &&
                result.status === "succeeded" &&
                (result.workItemStatus === "COMPLETED" ||
                    result.workItemStatus === "CLOSED")
            ) {
                onTaskCompleted?.(workItemId)
            }
            // 详情模式（focusMode）无队列导航控件，autoNext 不得静默自动跳转；
            // 自动下一项仅在带队列的列表模式生效，避免 URL 隐形状态驱动用户预期外的跳转。
            if (
                !focusMode &&
                result.terminal &&
                !result.stayOnItem &&
                autoNext &&
                result.status === "succeeded"
            ) {
                const next = neighbor(1) ?? neighbor(-1)
                if (next) {
                    window.setTimeout(() => goToItem(next), 400)
                }
            }
        },
        [
            autoNext,
            focusMode,
            goToItem,
            item?.workItem?.workItemId,
            neighbor,
            onTaskCompleted,
            setLastResult,
        ],
    )

    const responsibilityStatus = deriveResponsibilityStatus(item, userId)

    const can = (action: IntegrationActionKind) =>
        Boolean(item?.allowedActions.includes(action))

    const { handleClose } = useIntegrationResponsibilityCommands({
        item,
        comment,
        replacementTaskId,
        responsibilityMutation,
        commandIdentities: commandIdentities.current,
        setActionError,
        afterResult,
    })

    const runTaskAction = async (kind: IntegrationTaskActionKind) => {
        if (
            !item?.workItem ||
            responsibilityStatus !== "assigned_to_me" ||
            !can(kind)
        )
            return
        const evidenceRefs =
            kind === "ADD_EVIDENCE" || kind === "LINK_COMPENSATION"
                ? item.linkedEvidence
                : undefined
        if (
            (kind === "ADD_EVIDENCE" || kind === "LINK_COMPENSATION") &&
            evidenceRefs?.length === 0
        ) {
            setActionError("请先从受控证据入口关联已有证据")
            return
        }
        const identity = commandIdentities.current.get(kind, item.identity.id)
        try {
            const result = await actionMutation.mutateAsync({
                itemType: item.identity.itemType,
                itemId: item.identity.id,
                workItemId: item.workItem.workItemId,
                expectedSubjectVersion: item.workItem.subjectVersion,
                expectedTaskVersion: item.workItem.taskVersion,
                kind,
                operationId: identity.operationId,
                idempotencyKey: identity.idempotencyKey,
                comment: comment || undefined,
                evidenceRefs,
            })
            if (result.status === "succeeded") {
                commandIdentities.current.delete(identity.key)
            }
            afterResult(result)
        } catch (e) {
            setActionError(getErrorMessage(e, "动作失败"))
        }
    }

    const formalPending =
        actionMutation.isPending ||
        resolveMutation.isPending ||
        directMutation.isPending ||
        responsibilityMutation.isPending

    const reconReason =
        item?.reconciliationReasonRegistry?.registeredReasons.find(
            (r) => r.registeredReasonId === reconReasonId,
        )
    const reasonMismatches = (
        conclusion: "CONFIRM_NO_ERROR" | "CONFIRM_VALID_DIFFERENCE",
    ) => !reconReason || reconReason.conclusion !== conclusion

    async function handleResolve() {
        if (
            !item?.workItem ||
            !item.resolutionEvidencePolicy ||
            responsibilityStatus !== "assigned_to_me" ||
            !can("RESOLVE")
        )
            return
        const evidence = item.linkedEvidence
        const kinds = new Set(evidence.map((entry) => entry.kind))
        if (
            item.resolutionEvidencePolicy.requiredEvidenceKinds.some(
                (kind) => !kinds.has(kind),
            )
        ) {
            setActionError("完成凭证尚未齐备，请先从证据入口完成关联")
            return
        }
        const identity = commandIdentities.current.get(
            "resolve",
            item.identity.id,
        )
        try {
            const result = await resolveMutation.mutateAsync({
                itemType: item.identity.itemType,
                itemId: item.identity.id,
                workItemId: item.workItem.workItemId,
                expectedSubjectVersion: item.workItem.subjectVersion,
                expectedTaskVersion: item.workItem.taskVersion,
                operationId: identity.operationId,
                idempotencyKey: identity.idempotencyKey,
                reasonCode: "TERMINAL_EVIDENCE_VERIFIED",
                evidencePolicyId:
                    item.resolutionEvidencePolicy.evidencePolicyId,
                evidencePolicyVersion:
                    item.resolutionEvidencePolicy.evidencePolicyVersion,
                policyKey: item.resolutionEvidencePolicy.key,
                evidenceRefs: evidence,
                comment: comment || undefined,
            })
            commandIdentities.current.delete(identity.key)
            afterResult(result)
        } catch (e) {
            setActionError(getErrorMessage(e, "解决失败"))
            throw e
        }
    }

    const { handleDirectTerminal, handleDirectAction } =
        useIntegrationDirectActions({
            item,
            can,
            reconReasonId,
            comment,
            commandIdentities: commandIdentities.current,
            directMutation,
            afterResult,
            setActionError,
        })

    // Reset UI on item switch
    const firstReasonId =
        item?.reconciliationReasonRegistry?.registeredReasons[0]
            ?.registeredReasonId
    React.useEffect(() => {
        setActionError(null)
        setComment("")
        setReplacementTaskId("")
        setReconReasonId(firstReasonId ?? "")
        // setActionError 来自页面 useState，引用稳定；仅项目切换时重置
    }, [item?.identity.id, firstReasonId, setActionError])

    return {
        comment,
        setComment,
        replacementTaskId,
        setReplacementTaskId,
        reconReasonId,
        setReconReasonId,
        terminalConfirm,
        setTerminalConfirm,
        formalPending,
        responsibilityStatus,
        can,
        reasonMismatches,
        refresh,
        resultRef,
        headingRef,
        actionZoneRef,
        focusFirstAction,
        runTaskAction,
        handleClose,
        handleResolve,
        handleDirectTerminal,
        handleDirectAction,
    }
}
