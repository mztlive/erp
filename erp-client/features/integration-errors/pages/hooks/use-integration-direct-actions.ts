import { getErrorMessage } from "@/lib/api/errors"
import { INTEGRATION_ACTION_LABEL } from "../../lib/presentation"
import type {
    DirectReconciliationInput,
    IntegrationActionKind,
    IntegrationFormalResult,
    IntegrationResolutionItemView,
} from "../../types"
import type { CommandIdentityStore } from "../lib/command-identity"

export type IntegrationTaskActionKind =
    | "QUERY_ORIGINAL_RESULT"
    | "REPLAY_ORIGINAL"
    | "REATTRIBUTE"
    | "LINK_COMPENSATION"
    | "ADD_EVIDENCE"

export function useIntegrationDirectActions({
    item,
    can,
    reconReasonId,
    comment,
    commandIdentities,
    directMutation,
    afterResult,
    setActionError,
}: {
    item: IntegrationResolutionItemView | undefined
    can: (action: IntegrationActionKind) => boolean
    reconReasonId: string
    comment: string
    commandIdentities: CommandIdentityStore
    directMutation: {
        mutateAsync: (
            input: DirectReconciliationInput,
        ) => Promise<IntegrationFormalResult>
        isPending: boolean
    }
    afterResult: (result: IntegrationFormalResult) => void
    setActionError: (error: string | null) => void
}) {
    async function handleDirectTerminal(
        conclusion: "CONFIRM_NO_ERROR" | "CONFIRM_VALID_DIFFERENCE",
    ) {
        if (
            !item ||
            item.hasWorkItem ||
            item.identity.itemType !== "RECONCILIATION_DIFFERENCE" ||
            !can(conclusion)
        )
            return
        const reg = item.reconciliationReasonRegistry
        if (!reg) return
        const reason = reg.registeredReasons.find(
            (r) => r.registeredReasonId === reconReasonId,
        )
        if (!reason || reason.conclusion !== conclusion) {
            setActionError("请选择与结论匹配的注册原因")
            return
        }
        const evidence = item.linkedEvidence
        const evidenceKinds = new Set(evidence.map((entry) => entry.kind))
        if (
            reason.requiredEvidenceKinds.some(
                (kind) => !evidenceKinds.has(kind),
            )
        ) {
            setActionError("结论所需证据尚未齐备")
            return
        }
        const identity = commandIdentities.get(conclusion, item.identity.id)
        try {
            const result = await directMutation.mutateAsync({
                differenceId: item.identity.id,
                expectedDifferenceVersion: item.objectVersion,
                operationId: identity.operationId,
                idempotencyKey: identity.idempotencyKey,
                decision: {
                    kind: "TERMINAL_CONCLUSION",
                    reasonCode: reason.registeredReasonId,
                    reasonRegistryId: reg.reasonRegistryId,
                    reasonRegistryVersion: reg.reasonRegistryVersion,
                    registeredReasonId: reason.registeredReasonId,
                    conclusion,
                    evidenceRefs: evidence,
                    comment: comment || undefined,
                },
            })
            commandIdentities.delete(identity.key)
            afterResult(result)
        } catch (e) {
            setActionError(getErrorMessage(e, "对账确认失败"))
            throw e
        }
    }

    async function handleDirectAction(kind: IntegrationTaskActionKind) {
        const needsEvidence =
            kind === "ADD_EVIDENCE" || kind === "LINK_COMPENSATION"
        if (
            !item ||
            item.hasWorkItem ||
            item.identity.itemType !== "RECONCILIATION_DIFFERENCE" ||
            !can(kind) ||
            (needsEvidence && item.linkedEvidence.length === 0)
        ) {
            if (needsEvidence && item?.linkedEvidence.length === 0) {
                setActionError("请先从受控证据入口关联已有证据")
            }
            return
        }
        const identity = commandIdentities.get(
            `direct-${kind}`,
            item.identity.id,
        )
        try {
            const result = await directMutation.mutateAsync({
                differenceId: item.identity.id,
                expectedDifferenceVersion: item.objectVersion,
                operationId: identity.operationId,
                idempotencyKey: identity.idempotencyKey,
                decision: {
                    kind: "NON_TERMINAL_ACTION",
                    action: kind,
                    evidenceRefs: needsEvidence
                        ? item.linkedEvidence
                        : undefined,
                    comment: comment || undefined,
                },
            })
            if (result.status === "succeeded") {
                commandIdentities.delete(identity.key)
            }
            afterResult(result)
        } catch (error) {
            setActionError(
                getErrorMessage(
                    error,
                    `${INTEGRATION_ACTION_LABEL[kind] ?? kind}失败`,
                ),
            )
        }
    }

    return { handleDirectTerminal, handleDirectAction }
}
