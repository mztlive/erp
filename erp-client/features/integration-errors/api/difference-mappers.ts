/**
 * W29 对账差异 DTO → 界面视图映射。
 * 从 mappers.ts 拆出；mappers.ts 统一再导出 mapDifference。
 */

import type { WorkItemProjection } from "@/features/work-items"
import type { IntegrationResolutionItemView } from "../types"
import { FUNDS_LABEL } from "../types"
import {
    mapAllowedIntegrationActions,
    mapBackendEvidenceRefs,
    mapBackendReconciliationReasonRegistry,
    mapBackendResolutionEvidencePolicy,
} from "./wire"
import type { BackendDifference } from "./backend-types"
import {
    ageLabel,
    mapFormalWorkItem,
    tsToIso,
} from "./shared-mappers"

export function mapDifference(
    diff: BackendDifference,
    formalWorkItem?: WorkItemProjection,
): IntegrationResolutionItemView {
    const terminal =
        diff.status === "confirmed_no_error" ||
        diff.status === "confirmed_valid_difference"

    const workItem = formalWorkItem
        ? mapFormalWorkItem(formalWorkItem)
        : undefined
    const resolutionEvidencePolicy = workItem
        ? mapBackendResolutionEvidencePolicy(diff.resolution_evidence_policy)
        : undefined
    const reconciliationReasonRegistry = workItem
        ? undefined
        : mapBackendReconciliationReasonRegistry(
              diff.reconciliation_reason_registry,
          )
    const directConclusions =
        reconciliationReasonRegistry?.registeredReasons.map(
            (reason) => reason.conclusion,
        ) ?? []
    const linkedEvidence = mapBackendEvidenceRefs(diff.linked_evidence)
    const fundsImpact = resolutionEvidencePolicy?.key.fundsImpact ?? "POTENTIAL"
    const allowedActions = terminal
        ? []
        : mapAllowedIntegrationActions(diff.allowed_actions, {
              hasWorkItem: workItem !== undefined,
              hasResolutionPolicy: resolutionEvidencePolicy !== undefined,
              directConclusions,
          })
    return {
        identity: {
            itemType: "RECONCILIATION_DIFFERENCE",
            id: diff.id,
            number: diff.id,
            subjectHash: `v${diff.version}`,
        },
        workItem,
        businessObject: {
            objectType: diff.business_object_type,
            objectId: diff.business_object_id,
            title: `${diff.business_object_type} · ${diff.business_object_id}`,
        },
        classification: {
            code: diff.difference_type,
            errorClass: "reconciliation-difference",
            label: "对账差异",
            severity: "high",
            severityLabel: "高",
        },
        environment: "production",
        environmentLabel: "生产",
        status: {
            code: diff.status ?? "open",
            label: terminal
                ? diff.status === "confirmed_no_error"
                    ? "确认无误"
                    : "确认有效差异"
                : "待处理",
        },
        fundsImpact,
        fundsImpactLabel: FUNDS_LABEL[fundsImpact],
        compensationOpen: false,
        ageLabel: ageLabel(diff.created_at),
        ownerRole: formalWorkItem?.ownerRoleLabel ?? "财务",
        ownerUser: formalWorkItem?.ownerUser?.displayName,
        createdAt: tsToIso(diff.created_at),
        difference: {
            leftLabel: "左侧证据",
            leftSummary: diff.left_fact_reference ?? "—",
            rightLabel: "右侧证据",
            rightSummary: diff.right_fact_reference ?? "—",
            boundary: diff.business_object_type,
            watermark: tsToIso(diff.created_at),
            differenceType: diff.difference_type,
            differenceSummary: diff.difference_type,
        },
        hasWorkItem: workItem !== undefined,
        resolutionEvidencePolicy,
        reconciliationReasonRegistry,
        attempts: [],
        objectVersion: String(diff.version),
        allowedActions,
        actionBlockers: diff.action_blockers ?? [],
        repairLinks: [],
        auditTrail: (diff.resolutions ?? []).map((r) => ({
            id: r.id,
            at: tsToIso(r.handled_at),
            actor: r.handled_by,
            action: r.resolution_action,
            detail: r.evidence_reference ?? r.resulting_status,
        })),
        evidenceTimeline: [],
        linkedEvidence,
        freshness: { updatedAt: tsToIso(diff.created_at) },
    }
}
