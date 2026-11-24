import { useRef, useState } from "react"
import { useStore } from "@tanstack/react-form"
import { useQueryClient } from "@tanstack/react-query"

import { useAppForm } from "@/components/form"
import {
    useWorkItemResponsibilityMutation,
    type WorkItemResponsibilityCommand,
} from "@/features/work-items"
import { getErrorMessage, isApiError } from "@/lib/api/errors"
import {
    FormalCommandKeyLedger,
    isCommandOutcomeUnknown,
} from "@/lib/formal-command"
import { hasPermission } from "@/lib/permissions"
import { queryKeyRoots } from "@/lib/query-key-roots"
import {
    BPM_REASON_MAX_UTF8_BYTES,
    utf8ByteLength,
    WORK_ITEM_CLOSE_COMMENT_MAX_CHARS,
} from "@/lib/validation/reason-length"

import {
    useDirectReconciliationMutation,
    useIntegrationActionMutation,
    useResolveIntegrationMutation,
} from "../../../hooks/queries"
import type {
    ControlledEvidenceKind,
    DirectReconciliationInput,
    IntegrationFormalResult,
    IntegrationResolutionItemView,
    IntegrationResolveInput,
    IntegrationTaskActionInput,
} from "../../../types"
import type { IntegrationTaskActionKind } from "../../hooks/use-integration-actions"

type CloseKind = "CLOSE_DUPLICATE" | "CLOSE_MISROUTED"
type Conclusion = "CONFIRM_NO_ERROR" | "CONFIRM_VALID_DIFFERENCE"
export type InvestigationAction =
    | IntegrationTaskActionKind
    | "RESOLVE"
    | CloseKind
    | Conclusion

type Command =
    | {
          kind: "task"
          input: Omit<
              IntegrationTaskActionInput,
              "operationId" | "idempotencyKey"
          >
      }
    | {
          kind: "resolve"
          input: Omit<IntegrationResolveInput, "operationId" | "idempotencyKey">
      }
    | {
          kind: "direct"
          input: Omit<
              DirectReconciliationInput,
              "operationId" | "idempotencyKey"
          >
      }
    | {
          kind: "close"
          input: Omit<
              Extract<WorkItemResponsibilityCommand, { kind: "CLOSE" }>,
              "idempotencyKey"
          >
      }

const commandPermissions = {
    task: "integration_task:process",
    resolve: "integration_task:complete",
    direct: "reconciliation_difference:decide",
    close: "work_item:manage",
} as const

const taskActions: readonly IntegrationTaskActionKind[] = [
    "QUERY_ORIGINAL_RESULT",
    "REPLAY_ORIGINAL",
    "REATTRIBUTE",
    "LINK_COMPENSATION",
    "ADD_EVIDENCE",
]

const conclusions: readonly Conclusion[] = [
    "CONFIRM_NO_ERROR",
    "CONFIRM_VALID_DIFFERENCE",
]

export function isInvestigationTerminal(item: IntegrationResolutionItemView) {
    return (
        item.workItem?.status === "COMPLETED" ||
        item.workItem?.status === "CLOSED" ||
        [
            "resolved",
            "closed",
            "confirmed_no_error",
            "confirmed_valid_difference",
        ].includes(item.status.code)
    )
}

function getInvestigationActions(
    item: IntegrationResolutionItemView,
    permissions: readonly string[] | undefined,
    userId: string | undefined,
): InvestigationAction[] {
    if (isInvestigationTerminal(item)) return []
    const actions: InvestigationAction[] = []
    const workItem = item.workItem
    if (workItem) {
        if (
            workItem.allowedActions.includes("CLOSE") &&
            hasPermission(permissions, commandPermissions.close)
        ) {
            if (hasPermission(permissions, "work_item:list")) {
                actions.push("CLOSE_DUPLICATE")
            }
            actions.push("CLOSE_MISROUTED")
        }
        if (
            !userId ||
            workItem.ownerUser?.id !== userId ||
            workItem.processingState !== "READY" ||
            !workItem.allowedActions.includes("PROCESS")
        )
            return actions
        if (hasPermission(permissions, commandPermissions.task)) {
            actions.push(
                ...taskActions.filter((kind) =>
                    item.allowedActions.includes(kind),
                ),
            )
        }
        if (
            item.resolutionEvidencePolicy &&
            item.allowedActions.includes("RESOLVE") &&
            hasPermission(permissions, commandPermissions.resolve)
        )
            actions.push("RESOLVE")
    } else if (
        item.identity.itemType === "RECONCILIATION_DIFFERENCE" &&
        !item.hasWorkItem &&
        item.reconciliationReasonRegistry &&
        hasPermission(permissions, commandPermissions.direct)
    ) {
        actions.push(
            ...taskActions.filter((kind) => item.allowedActions.includes(kind)),
        )
        actions.push(
            ...conclusions.filter((kind) => item.allowedActions.includes(kind)),
        )
    }
    return actions
}

export function isInvestigationClose(
    action: InvestigationAction,
): action is CloseKind {
    return action === "CLOSE_DUPLICATE" || action === "CLOSE_MISROUTED"
}

export function isInvestigationConclusion(
    action: InvestigationAction,
): action is Conclusion {
    return (
        action === "CONFIRM_NO_ERROR" || action === "CONFIRM_VALID_DIFFERENCE"
    )
}

function buildCommand(
    item: IntegrationResolutionItemView,
    action: InvestigationAction,
    remark: string,
    replacementTaskId: string,
    reasonId: string,
): Command | undefined {
    if (utf8ByteLength(remark) > BPM_REASON_MAX_UTF8_BYTES) return
    const comment = remark.trim() || undefined
    const workItem = item.workItem
    const registry = item.reconciliationReasonRegistry
    const evidenceKinds = new Set(
        item.linkedEvidence.map((evidence) => evidence.kind),
    )
    const hasEvidence = (kinds: readonly ControlledEvidenceKind[]) =>
        kinds.every((kind) => evidenceKinds.has(kind))

    if (isInvestigationClose(action)) {
        if (
            !workItem ||
            (comment &&
                Array.from(comment).length >
                    WORK_ITEM_CLOSE_COMMENT_MAX_CHARS) ||
            (action === "CLOSE_DUPLICATE" &&
                (!replacementTaskId ||
                    replacementTaskId === workItem.workItemId)) ||
            (action === "CLOSE_MISROUTED" && !comment)
        )
            return
        return {
            kind: "close",
            input: {
                kind: "CLOSE",
                workItemId: workItem.workItemId,
                expectedTaskVersion: workItem.taskVersion,
                reasonCode:
                    action === "CLOSE_DUPLICATE" ? "DUPLICATE" : "MISROUTED",
                replacementWorkItemId:
                    action === "CLOSE_DUPLICATE"
                        ? replacementTaskId
                        : undefined,
                comment,
            },
        }
    }
    if (isInvestigationConclusion(action)) {
        const reason = registry?.registeredReasons.find(
            (entry) => entry.registeredReasonId === reasonId,
        )
        if (
            !registry ||
            !reason ||
            reason.conclusion !== action ||
            !hasEvidence(reason.requiredEvidenceKinds)
        )
            return
        return {
            kind: "direct",
            input: {
                differenceId: item.identity.id,
                expectedDifferenceVersion: item.objectVersion,
                decision: {
                    kind: "TERMINAL_CONCLUSION",
                    reasonCode: reason.registeredReasonId,
                    reasonRegistryId: registry.reasonRegistryId,
                    reasonRegistryVersion: registry.reasonRegistryVersion,
                    registeredReasonId: reason.registeredReasonId,
                    conclusion: action,
                    evidenceRefs: item.linkedEvidence,
                    comment,
                },
            },
        }
    }

    const policy = item.resolutionEvidencePolicy
    const taskInput = workItem && {
        itemType: item.identity.itemType,
        itemId: item.identity.id,
        workItemId: workItem.workItemId,
        expectedSubjectVersion: workItem.subjectVersion,
        expectedTaskVersion: workItem.taskVersion,
        comment,
    }
    if (action === "RESOLVE") {
        if (!taskInput || !policy || !hasEvidence(policy.requiredEvidenceKinds))
            return
        return {
            kind: "resolve",
            input: {
                ...taskInput,
                reasonCode: "TERMINAL_EVIDENCE_VERIFIED",
                evidencePolicyId: policy.evidencePolicyId,
                evidencePolicyVersion: policy.evidencePolicyVersion,
                policyKey: policy.key,
                evidenceRefs: item.linkedEvidence,
            },
        }
    }

    const needsEvidence =
        action === "ADD_EVIDENCE" || action === "LINK_COMPENSATION"
    const evidenceRefs = needsEvidence ? item.linkedEvidence : undefined
    if (evidenceRefs?.length === 0) return
    if (taskInput) {
        return {
            kind: "task",
            input: { ...taskInput, kind: action, evidenceRefs },
        }
    }
    return {
        kind: "direct",
        input: {
            differenceId: item.identity.id,
            expectedDifferenceVersion: item.objectVersion,
            decision: {
                kind: "NON_TERMINAL_ACTION",
                action,
                evidenceRefs,
                comment,
            },
        },
    }
}

type SubmitState =
    | { status: "idle" }
    | { status: "pending"; action: InvestigationAction }
    | { status: "unknown"; action: InvestigationAction }
    | { status: "conflict" | "failed"; message: string }
    | { status: "settled"; result: IntegrationFormalResult }

export function useInvestigationCommands({
    item,
    workItemId,
    permissions,
    userId,
    refetch,
    onTaskCompleted,
}: {
    item?: IntegrationResolutionItemView
    workItemId?: string
    permissions?: readonly string[]
    userId?: string
    refetch: () => Promise<boolean>
    onTaskCompleted?: (workItemId: string) => void
}) {
    const client = useQueryClient()
    const taskMutation = useIntegrationActionMutation()
    const resolveMutation = useResolveIntegrationMutation()
    const directMutation = useDirectReconciliationMutation()
    const closeMutation = useWorkItemResponsibilityMutation()

    const [ledger] = useState(() => new FormalCommandKeyLedger())
    const inFlight = useRef(false)
    const [state, setState] = useState<SubmitState>({ status: "idle" })
    const form = useAppForm({
        defaultValues: { comment: "", replacementTaskId: "", reasonId: "" },
    })
    const { comment, replacementTaskId, reasonId } = useStore(
        form.store,
        (formState) => formState.values,
    )
    const [confirmAction, setConfirmAction] = useState<InvestigationAction>()
    const [showComment, setShowComment] = useState(false)
    const slot = "investigation"
    const locked = state.status === "pending" || state.status === "unknown"
    const actions =
        item && !(state.status === "settled" && state.result.terminal)
            ? getInvestigationActions(item, permissions, userId)
            : []
    const selectedReasonId =
        reasonId ||
        item?.reconciliationReasonRegistry?.registeredReasons[0]
            ?.registeredReasonId ||
        ""

    function commandFor(action: InvestigationAction) {
        if (!item || !actions.includes(action)) return
        return buildCommand(
            item,
            action,
            comment,
            replacementTaskId,
            selectedReasonId,
        )
    }

    async function execute(action: InvestigationAction, recover = false) {
        const current = ledger.peek<Command>(slot)
        if (inFlight.current || recover !== Boolean(current)) return
        if (recover && state.status !== "unknown") return
        if (!recover && (locked || state.status === "conflict")) return

        const command = current?.payload ?? commandFor(action)
        if (
            !command ||
            !hasPermission(permissions, commandPermissions[command.kind])
        )
            return
        let identity: ReturnType<typeof ledger.acquire<Command>>
        try {
            identity = ledger.acquire(slot, "w29", command)
        } catch (error) {
            setState({
                status: "failed",
                message: getErrorMessage(error, "无法提交，请刷新重试。"),
            })
            return
        }

        inFlight.current = true
        setState({ status: "pending", action })
        const keys = {
            idempotencyKey: identity.idempotencyKey,
            operationId: identity.idempotencyKey,
        }
        const snapshot = identity.payload
        let result: IntegrationFormalResult
        try {
            switch (snapshot.kind) {
                case "task":
                    result = await taskMutation.mutateAsync({
                        ...snapshot.input,
                        ...keys,
                    })
                    break
                case "resolve":
                    result = await resolveMutation.mutateAsync({
                        ...snapshot.input,
                        ...keys,
                    })
                    break
                case "direct":
                    result = await directMutation.mutateAsync({
                        ...snapshot.input,
                        ...keys,
                    })
                    break
                case "close":
                    await closeMutation.mutateAsync({
                        ...snapshot.input,
                        idempotencyKey: keys.idempotencyKey,
                    })
                    result = {
                        status: "succeeded",
                        title: "已关闭任务",
                        description:
                            "当前任务已退出待处理队列，业务记录保持原状。",
                        terminal: true,
                        workItemStatus: "CLOSED",
                        stayOnItem: false,
                    }
                    break
            }
        } catch (error) {
            if (isCommandOutcomeUnknown(error)) {
                ledger.settle(slot, "unknown")
                setState({ status: "unknown", action })
            } else if (isApiError(error) && error.status === 409) {
                ledger.settle(slot, "failed")
                setConfirmAction(undefined)
                setState({
                    status: "conflict",
                    message: getErrorMessage(
                        error,
                        "任务已变化，请刷新最新数据。",
                    ),
                })
            } else {
                ledger.settle(slot, "failed")
                setState({
                    status: "failed",
                    message: getErrorMessage(error, "提交失败，请核对后重试。"),
                })
            }
            return
        } finally {
            inFlight.current = false
        }
        // RESULT_UNKNOWN 是已收到的外部查询结论。提交回执已确认，可发起下一次查询。
        ledger.settle(slot, "succeeded")
        setState({ status: "settled", result })
        setConfirmAction(undefined)
        setShowComment(false)
        form.reset()
        if (snapshot.kind === "close" || snapshot.kind === "direct") {
            void refetch()
        }
        if (snapshot.kind === "close" || snapshot.kind === "resolve") {
            onTaskCompleted?.(snapshot.input.workItemId)
        } else if (
            snapshot.kind === "direct" &&
            result.terminal &&
            workItemId
        ) {
            onTaskCompleted?.(workItemId)
        }
    }

    return {
        actions,
        state,
        locked,
        canRecover:
            state.status === "unknown" &&
            currentCommandPermitted(
                ledger.peek<Command>(slot)?.payload,
                permissions,
            ),
        comment,
        setComment: (value: string) => form.setFieldValue("comment", value),
        replacementTaskId,
        setReplacementTaskId: (value: string) =>
            form.setFieldValue("replacementTaskId", value),
        reasonId: selectedReasonId,
        setReasonId: (value: string) => form.setFieldValue("reasonId", value),
        showComment,
        setShowComment,
        confirmAction,
        setConfirmAction,
        canSubmit: (action: InvestigationAction) =>
            !locked &&
            state.status !== "conflict" &&
            Boolean(commandFor(action)),
        execute,
        async refresh() {
            const [refreshed] = await Promise.all([
                refetch(),
                client.invalidateQueries({ queryKey: queryKeyRoots.workItems }),
            ])
            if (refreshed)
                setState((previous) =>
                    previous.status === "conflict"
                        ? { status: "idle" }
                        : previous,
                )
        },
    }
}

function currentCommandPermitted(
    command: Command | undefined,
    permissions: readonly string[] | undefined,
) {
    return Boolean(
        command && hasPermission(permissions, commandPermissions[command.kind]),
    )
}

export type InvestigationCommands = ReturnType<typeof useInvestigationCommands>
