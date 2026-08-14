/**
 * W29 wire adapter.
 *
 * UI models stay camelCase. Every object crossing the HTTP boundary is
 * constructed here with the backend's strict snake_case contract.
 */

import type {
    ControlledEvidenceKind,
    ControlledTerminalEvidenceRef,
    DirectReconciliationInput,
    DirectReconciliationReasonCode,
    FundsImpact,
    IntegrationActionKind,
    IntegrationResolveInput,
    IntegrationTaskActionInput,
    ReconciliationReasonRegistryView,
    ResolutionEvidencePolicyView,
} from "../types"

const EVIDENCE_KINDS: ReadonlySet<string> = new Set([
    "EXTERNAL_CASE_RESULT",
    "BUSINESS_OBJECT_VERIFICATION",
    "FINANCIAL_RECONCILIATION",
    "COMPENSATION_RESULT",
    "DISTINCT_REVIEW",
])

const FUNDS_IMPACTS: ReadonlySet<string> = new Set([
    "NONE",
    "POTENTIAL",
    "POSTED",
])

const REVIEWER_SEPARATIONS: ReadonlySet<string> = new Set([
    "NONE",
    "DISTINCT_REVIEWER",
    "DISTINCT_FINANCE_REVIEWER",
])

const RECONCILIATION_CONCLUSIONS: ReadonlySet<string> = new Set([
    "CONFIRM_NO_ERROR",
    "CONFIRM_VALID_DIFFERENCE",
])

const INTEGRATION_ACTIONS: ReadonlySet<string> = new Set([
    "QUERY_ORIGINAL_RESULT",
    "REPLAY_ORIGINAL",
    "REATTRIBUTE",
    "LINK_COMPENSATION",
    "ADD_EVIDENCE",
    "RESOLVE",
    "CONFIRM_NO_ERROR",
    "CONFIRM_VALID_DIFFERENCE",
])

const DIRECT_RECONCILIATION_REASONS: ReadonlySet<string> = new Set([
    "SOURCE_CORRECTED_AND_REATTRIBUTED",
    "BUSINESS_CONFIRMED_NO_ERROR",
    "COMPENSATION_CLOSED",
])

const DIRECT_REASON_CONCLUSIONS: Readonly<
    Record<
        DirectReconciliationReasonCode,
        "CONFIRM_NO_ERROR" | "CONFIRM_VALID_DIFFERENCE"
    >
> = {
    SOURCE_CORRECTED_AND_REATTRIBUTED: "CONFIRM_VALID_DIFFERENCE",
    BUSINESS_CONFIRMED_NO_ERROR: "CONFIRM_NO_ERROR",
    COMPENSATION_CLOSED: "CONFIRM_VALID_DIFFERENCE",
}

export type BackendControlledEvidenceRef = {
    kind: string
    record_id: string
    label: string
}

export type BackendResolutionEvidencePolicy = {
    evidence_policy_id: string
    evidence_policy_version: number
    key: {
        error_type: string
        funds_impact: string
    }
    required_evidence_kinds: string[]
    reviewer_separation: string
}

export type BackendRegisteredReconciliationReason = {
    registered_reason_id: string
    registered_reason_version: number
    conclusion: string
    label: string
    required_evidence_kinds: string[]
}

export type BackendReconciliationReasonRegistry = {
    reason_registry_id: string
    reason_registry_version: number
    registered_reasons: BackendRegisteredReconciliationReason[]
}

type EvidenceRefWire = {
    kind: ControlledEvidenceKind
    record_id: string
    label: string
}

function isNonEmpty(value: unknown): value is string {
    return typeof value === "string" && value.trim().length > 0
}

function isPositiveVersion(value: unknown): value is number {
    return Number.isSafeInteger(value) && Number(value) > 0
}

function parseEvidenceKinds(
    values: readonly string[],
): ControlledEvidenceKind[] | undefined {
    if (!Array.isArray(values) || values.length === 0) return undefined
    if (!values.every((value) => EVIDENCE_KINDS.has(value))) {
        return undefined
    }
    return [...new Set(values as ControlledEvidenceKind[])]
}

export function mapAllowedIntegrationActions(
    raw: readonly string[] | null | undefined,
    options: {
        hasWorkItem: boolean
        hasResolutionPolicy: boolean
        directConclusions: readonly IntegrationActionKind[]
    },
): IntegrationActionKind[] {
    if (!Array.isArray(raw)) return []
    const directConclusions = new Set(options.directConclusions)
    const actions = raw.filter((action): action is IntegrationActionKind =>
        INTEGRATION_ACTIONS.has(action),
    )
    return [...new Set(actions)].filter((action) => {
        if (options.hasWorkItem) {
            if (
                action === "CONFIRM_NO_ERROR" ||
                action === "CONFIRM_VALID_DIFFERENCE"
            ) {
                return false
            }
        } else if (action === "RESOLVE") {
            return false
        }
        if (action === "RESOLVE" && !options.hasResolutionPolicy) return false
        if (
            (action === "CONFIRM_NO_ERROR" ||
                action === "CONFIRM_VALID_DIFFERENCE") &&
            !directConclusions.has(action)
        ) {
            return false
        }
        return true
    })
}

export function toEvidenceRefsWire(
    evidenceRefs: readonly ControlledTerminalEvidenceRef[] | undefined,
): EvidenceRefWire[] {
    return (evidenceRefs ?? []).map((evidence) => ({
        kind: evidence.kind,
        record_id: evidence.recordId,
        label: evidence.label,
    }))
}

export function mapBackendEvidenceRefs(
    evidenceRefs: readonly BackendControlledEvidenceRef[] | null | undefined,
): ControlledTerminalEvidenceRef[] {
    if (!Array.isArray(evidenceRefs)) return []
    const mapped: ControlledTerminalEvidenceRef[] = []
    for (const evidence of evidenceRefs) {
        if (
            !EVIDENCE_KINDS.has(evidence.kind) ||
            !isNonEmpty(evidence.record_id) ||
            !isNonEmpty(evidence.label)
        ) {
            continue
        }
        mapped.push({
            kind: evidence.kind as ControlledEvidenceKind,
            recordId: evidence.record_id,
            label: evidence.label,
        })
    }
    return mapped
}

export function mapBackendResolutionEvidencePolicy(
    policy: BackendResolutionEvidencePolicy | null | undefined,
): ResolutionEvidencePolicyView | undefined {
    if (
        !policy ||
        !isNonEmpty(policy.evidence_policy_id) ||
        !isPositiveVersion(policy.evidence_policy_version) ||
        !isNonEmpty(policy.key?.error_type) ||
        !FUNDS_IMPACTS.has(policy.key?.funds_impact) ||
        !REVIEWER_SEPARATIONS.has(policy.reviewer_separation)
    ) {
        return undefined
    }
    const requiredEvidenceKinds = parseEvidenceKinds(
        policy.required_evidence_kinds,
    )
    if (!requiredEvidenceKinds) return undefined
    return {
        evidencePolicyId: policy.evidence_policy_id,
        evidencePolicyVersion: policy.evidence_policy_version,
        key: {
            errorType: policy.key.error_type,
            fundsImpact: policy.key.funds_impact as FundsImpact,
        },
        requiredEvidenceKinds,
        reviewerSeparation:
            policy.reviewer_separation as ResolutionEvidencePolicyView["reviewerSeparation"],
    }
}

export function mapBackendReconciliationReasonRegistry(
    registry: BackendReconciliationReasonRegistry | null | undefined,
): ReconciliationReasonRegistryView | undefined {
    if (
        !registry ||
        !isNonEmpty(registry.reason_registry_id) ||
        !isPositiveVersion(registry.reason_registry_version) ||
        !Array.isArray(registry.registered_reasons) ||
        registry.registered_reasons.length === 0
    ) {
        return undefined
    }

    const registeredReasons = []
    const registeredReasonIds = new Set<string>()
    for (const reason of registry.registered_reasons) {
        const requiredEvidenceKinds = parseEvidenceKinds(
            reason.required_evidence_kinds,
        )
        if (
            !isNonEmpty(reason.registered_reason_id) ||
            !DIRECT_RECONCILIATION_REASONS.has(reason.registered_reason_id) ||
            !isPositiveVersion(reason.registered_reason_version) ||
            !RECONCILIATION_CONCLUSIONS.has(reason.conclusion) ||
            !isNonEmpty(reason.label) ||
            !requiredEvidenceKinds ||
            registeredReasonIds.has(reason.registered_reason_id) ||
            DIRECT_REASON_CONCLUSIONS[
                reason.registered_reason_id as DirectReconciliationReasonCode
            ] !== reason.conclusion
        ) {
            return undefined
        }
        registeredReasonIds.add(reason.registered_reason_id)
        registeredReasons.push({
            registeredReasonId:
                reason.registered_reason_id as DirectReconciliationReasonCode,
            registeredReasonVersion: reason.registered_reason_version,
            conclusion:
                reason.conclusion as ReconciliationReasonRegistryView["registeredReasons"][number]["conclusion"],
            label: reason.label,
            requiredEvidenceKinds,
        })
    }
    if (registeredReasonIds.size !== DIRECT_RECONCILIATION_REASONS.size) {
        return undefined
    }

    return {
        reasonRegistryId: registry.reason_registry_id,
        reasonRegistryVersion: registry.reason_registry_version,
        registeredReasons,
    }
}

export function toTaskActionWire(input: IntegrationTaskActionInput) {
    return {
        work_item_id: input.workItemId,
        expected_task_version: input.expectedTaskVersion,
        expected_subject_version: input.expectedSubjectVersion,
        action: {
            item_type: input.itemType,
            item_id: input.itemId,
            kind: input.kind,
            operation_id: input.operationId,
            ...(input.reasonCode === undefined
                ? {}
                : { reason_code: input.reasonCode }),
            ...(input.comment === undefined ? {} : { comment: input.comment }),
            evidence_refs: toEvidenceRefsWire(input.evidenceRefs),
        },
        idempotency_key: input.idempotencyKey,
    }
}

export function toTaskCompletionWire(input: IntegrationResolveInput) {
    return {
        work_item_id: input.workItemId,
        expected_task_version: input.expectedTaskVersion,
        expected_subject_version: input.expectedSubjectVersion,
        decision: {
            item_type: input.itemType,
            item_id: input.itemId,
            kind: "RESOLVE" as const,
            operation_id: input.operationId,
            reason_code: input.reasonCode,
            ...(input.comment === undefined ? {} : { comment: input.comment }),
            evidence_policy_id: input.evidencePolicyId,
            evidence_policy_version: input.evidencePolicyVersion,
            policy_key: {
                error_type: input.policyKey.errorType,
                funds_impact: input.policyKey.fundsImpact,
            },
            evidence_refs: toEvidenceRefsWire(input.evidenceRefs),
        },
        idempotency_key: input.idempotencyKey,
    }
}

export function toDirectReconciliationWire(input: DirectReconciliationInput) {
    if (
        input.decision.kind === "TERMINAL_CONCLUSION" &&
        (input.decision.reasonCode !== input.decision.registeredReasonId ||
            DIRECT_REASON_CONCLUSIONS[input.decision.reasonCode] !==
                input.decision.conclusion)
    ) {
        throw new Error("直接对账原因代码、注册原因与结论必须一致")
    }
    const decision =
        input.decision.kind === "NON_TERMINAL_ACTION"
            ? {
                  kind: input.decision.kind,
                  action: input.decision.action,
                  evidence_refs: toEvidenceRefsWire(
                      input.decision.evidenceRefs,
                  ),
                  ...(input.decision.comment === undefined
                      ? {}
                      : { comment: input.decision.comment }),
              }
            : {
                  kind: input.decision.kind,
                  reason_code: input.decision.reasonCode,
                  reason_registry_id: input.decision.reasonRegistryId,
                  reason_registry_version: input.decision.reasonRegistryVersion,
                  registered_reason_id: input.decision.registeredReasonId,
                  conclusion: input.decision.conclusion,
                  evidence_refs: toEvidenceRefsWire(
                      input.decision.evidenceRefs,
                  ),
                  ...(input.decision.comment === undefined
                      ? {}
                      : { comment: input.decision.comment }),
              }

    return {
        difference_id: input.differenceId,
        expected_difference_version: input.expectedDifferenceVersion,
        decision,
        operation_id: input.operationId,
        idempotency_key: input.idempotencyKey,
    }
}
