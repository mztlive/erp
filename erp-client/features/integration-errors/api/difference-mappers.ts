/**
 * W29 对账差异 DTO → 界面视图映射。
 * 从 mappers.ts 拆出；mappers.ts 统一再导出 mapDifference。
 */

import type { WorkItemProjection } from "@/features/work-items"
import { displayName } from "@/lib/display-name"
import type { IntegrationResolutionItemView } from "../types"
import { DIFFERENCE_TYPE_LABEL, FUNDS_LABEL } from "../types"
import {
    mapAllowedIntegrationActions,
    mapBackendEvidenceRefs,
    mapBackendReconciliationReasonRegistry,
    mapBackendResolutionEvidencePolicy,
} from "./wire"
import type { BackendDifference } from "./backend-types"
import { ageLabel, mapFormalWorkItem, tsToIso } from "./shared-mappers"

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
            number: diff.display_number?.trim() || "",
            subjectHash: `v${diff.version}`,
        },
        workItem,
        businessObject: {
            objectType: diff.business_object_type,
            objectId: diff.business_object_id,
            title:
                diff.business_object_label?.trim() ||
                formalWorkItem?.businessObjectLabel ||
                "业务对象名称未提供",
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
        ownerUser:
            displayName(
                formalWorkItem?.ownerUser?.displayName,
                formalWorkItem?.ownerUser?.id,
            ) ?? displayName(diff.owner_user_name, diff.owner_user_id),
        createdAt: tsToIso(diff.created_at),
        difference: {
            leftLabel: "左侧证据",
            leftSummary:
                diff.left_fact_label?.trim() ||
                (diff.left_fact_reference
                    ? "左侧证据名称未提供"
                    : "未关联证据"),
            rightLabel: "右侧证据",
            rightSummary:
                diff.right_fact_label?.trim() ||
                (diff.right_fact_reference
                    ? "右侧证据名称未提供"
                    : "未关联证据"),
            boundary: diff.business_object_label?.trim() || "关联业务对象",
            watermark: tsToIso(diff.created_at),
            differenceType: diff.difference_type,
            differenceSummary:
                DIFFERENCE_TYPE_LABEL[diff.difference_type.toUpperCase()] ||
                "差异说明未提供",
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
            actor:
                displayName(r.handled_by_name, r.handled_by) ||
                "操作人名称未提供",
            action: r.resolution_action,
            detail:
                r.evidence_label?.trim() ||
                (r.evidence_reference ? "已登记处理证据" : "已登记处理结果"),
        })),
        evidenceTimeline: [],
        linkedEvidence,
        freshness: { updatedAt: tsToIso(diff.created_at) },
    }
}
