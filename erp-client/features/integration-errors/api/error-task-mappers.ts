/**
 * W29 错误任务 DTO → 界面视图映射。
 * 从 mappers.ts 拆出；mappers.ts 统一再导出 mapErrorTask。
 */

import type { WorkItemProjection } from "@/features/work-items"
import type { IntegrationResolutionItemView } from "../types"
import { ERROR_CLASS_LABEL, FUNDS_LABEL } from "../types"
import {
    mapAllowedIntegrationActions,
    mapBackendEvidenceRefs,
    mapBackendResolutionEvidencePolicy,
} from "./wire"
import type { BackendErrorTask } from "./backend-types"
import {
    ageLabel,
    mapErrorClass,
    mapFormalWorkItem,
    severityOf,
    statusLabel,
    tsToIso,
} from "./shared-mappers"

export function mapErrorTask(
    task: BackendErrorTask,
    formalWorkItem?: WorkItemProjection,
): IntegrationResolutionItemView {
    const errorClass = mapErrorClass(task.error_class)
    const label = ERROR_CLASS_LABEL[errorClass] ?? task.error_class
    const severity = severityOf(task.error_class)
    const workItem = formalWorkItem
        ? mapFormalWorkItem(formalWorkItem)
        : undefined
    const resolutionEvidencePolicy = workItem
        ? mapBackendResolutionEvidencePolicy(task.resolution_evidence_policy)
        : undefined
    const linkedEvidence = mapBackendEvidenceRefs(task.linked_evidence)
    const fundsImpact = resolutionEvidencePolicy?.key.fundsImpact ?? "NONE"
    const allowedActions = workItem
        ? mapAllowedIntegrationActions(task.allowed_actions, {
              hasWorkItem: true,
              hasResolutionPolicy: resolutionEvidencePolicy !== undefined,
              directConclusions: [],
          })
        : []

    return {
        identity: {
            itemType: "ERROR_TASK",
            id: task.id,
            number: task.id,
            subjectHash: `v${task.version}`,
        },
        workItem,
        businessObject: {
            objectType: task.message_id ? "INBOX_MESSAGE" : "BUSINESS_OBJECT",
            objectId: task.business_object_id ?? task.message_id ?? task.id,
            title: task.business_object_id ?? task.message_id ?? task.id,
        },
        classification: {
            code: task.error_class,
            errorClass,
            label,
            severity,
            severityLabel:
                severity === "critical"
                    ? "阻断"
                    : severity === "high"
                      ? "高"
                      : severity === "low"
                        ? "低"
                        : "中",
        },
        environment: "production",
        environmentLabel: "生产",
        status: {
            code: task.status,
            label: statusLabel(task.status),
        },
        fundsImpact,
        fundsImpactLabel: FUNDS_LABEL[fundsImpact],
        compensationOpen: false,
        ageLabel: ageLabel(task.created_at),
        ownerRole: formalWorkItem?.ownerRoleLabel ?? task.owner_role ?? "—",
        ownerUser:
            formalWorkItem?.ownerUser?.displayName ??
            task.owner_user_id ??
            undefined,
        createdAt: tsToIso(task.created_at),
        message: task.message_id
            ? {
                  eventIdSummary: task.message_id,
                  idempotencyKeySummary: "—",
                  businessFactKeySummary: "—",
                  schemaVersion: "—",
                  directionLabel: "入站",
                  maskedPayloadSummary: task.last_attempt_summary ?? "—",
              }
            : undefined,
        hasWorkItem: workItem !== undefined,
        resolutionEvidencePolicy,
        attempts: task.last_attempt_summary
            ? [
                  {
                      attemptNumber: task.attempt_count,
                      attemptedAt:
                          tsToIso(task.last_attempt_at) ||
                          tsToIso(task.created_at),
                      result: task.last_attempt_summary,
                  },
              ]
            : [],
        objectVersion: String(task.version),
        allowedActions,
        actionBlockers: [
            ...(task.action_blockers ?? []),
            ...(workItem
                ? []
                : [
                      {
                          action: "PROCESS",
                          code: "FORMAL_WORK_ITEM_MISSING",
                          message:
                              "尚未建立 W29 正式处理责任，当前错误只能查看。",
                      },
                  ]),
        ],
        repairLinks: [],
        auditTrail: [],
        evidenceTimeline: [],
        linkedEvidence,
        freshness: {
            updatedAt:
                tsToIso(task.last_attempt_at) || tsToIso(task.created_at),
        },
    }
}
