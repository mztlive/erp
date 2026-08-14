/**
 * W29 处理动作与解决命令的请求函数（mutationFn）。
 * 从 requests.ts 拆出；requests.ts 统一再导出三个公开函数。
 */

import { apiPost } from "@/lib/api"
import type {
    DirectReconciliationInput,
    IntegrationFormalResult,
    IntegrationResolveInput,
    IntegrationTaskActionInput,
} from "../types"
import { mapAllowedIntegrationActions } from "./wire"
import {
    toDirectReconciliationWire,
    toTaskActionWire,
    toTaskCompletionWire,
} from "./wire"

export async function applyIntegrationTaskAction(
    input: IntegrationTaskActionInput,
): Promise<IntegrationFormalResult> {
    const result = await apiPost<{
        work_item_id: string
        work_item_status: "OPEN"
        evidence: {
            operation_id: string
            outcome:
                | "TERMINAL_EVIDENCE_FOUND"
                | "NO_RESULT_CONFIRMED"
                | "RESULT_UNKNOWN"
                | "REPLAY_ACCEPTED"
                | "REATTRIBUTED"
                | "EVIDENCE_LINKED"
                | "EVIDENCE_ADDED"
            business_result_reference?: string | null
            evidence_reference?: string | null
        }
        next_allowed_actions: string[]
    }>("/admin/integration/task-actions", toTaskActionWire(input))
    const titleByOutcome: Record<typeof result.evidence.outcome, string> = {
        TERMINAL_EVIDENCE_FOUND: "已取得可验证结果",
        NO_RESULT_CONFIRMED: "已确认原操作无结果",
        RESULT_UNKNOWN: "结果仍需核实",
        REPLAY_ACCEPTED: "重新提交已受理",
        REATTRIBUTED: "重新归集已记录",
        EVIDENCE_LINKED: "补偿证据已关联",
        EVIDENCE_ADDED: "证据已补充",
    }
    return {
        status:
            result.evidence.outcome === "RESULT_UNKNOWN"
                ? "unknown"
                : "succeeded",
        title: titleByOutcome[result.evidence.outcome],
        description:
            "本次处理记录已追加；当前任务仍为待处理，取得完成凭证后需单独确认解决。",
        reference: result.evidence.operation_id,
        outcome: result.evidence.outcome,
        nextAllowedActions: mapAllowedIntegrationActions(
            result.next_allowed_actions,
            {
                hasWorkItem: true,
                hasResolutionPolicy: true,
                directConclusions: [],
            },
        ),
        workItemStatus: result.work_item_status,
        stayOnItem: true,
        terminal: false,
        facts: [
            ...(result.evidence.business_result_reference
                ? [
                      {
                          label: "业务结果",
                          value: result.evidence.business_result_reference,
                      },
                  ]
                : []),
            ...(result.evidence.evidence_reference
                ? [
                      {
                          label: "证据记录",
                          value: result.evidence.evidence_reference,
                      },
                  ]
                : []),
        ],
    }
}

export async function resolveIntegrationTask(
    input: IntegrationResolveInput,
): Promise<IntegrationFormalResult> {
    const result = await apiPost<{
        work_item_id: string
        work_item_status: "COMPLETED"
        operation_id: string
        resolution_record_id: string
        terminal_evidence_reference: string
    }>("/admin/integration/task-completions", toTaskCompletionWire(input))
    return {
        status: "succeeded",
        title: "已标记解决",
        description: "处理已完成，可进入下一项。",
        reference: result.resolution_record_id,
        outcome: "RESOLVED",
        workItemStatus: result.work_item_status,
        stayOnItem: false,
        terminal: true,
        facts: [
            {
                label: "完成凭证",
                value: result.terminal_evidence_reference,
            },
        ],
    }
}

export async function applyDirectReconciliation(
    input: DirectReconciliationInput,
): Promise<IntegrationFormalResult> {
    const result = await apiPost<{
        difference_id: string
        operation_id: string
        resolution_record_id: string
        resulting_status:
            | "OPEN"
            | "EVIDENCE_PENDING"
            | "CONFIRMED_NO_ERROR"
            | "CONFIRMED_VALID_DIFFERENCE"
        is_terminal: boolean
        outcome:
            | "TERMINAL_EVIDENCE_FOUND"
            | "NO_RESULT_CONFIRMED"
            | "RESULT_UNKNOWN"
            | "REPLAY_ACCEPTED"
            | "REATTRIBUTED"
            | "EVIDENCE_LINKED"
            | "EVIDENCE_ADDED"
            | "CONFIRMED_NO_ERROR"
            | "CONFIRMED_VALID_DIFFERENCE"
        business_result_reference?: string | null
    }>(
        `/admin/integration/differences/${encodeURIComponent(input.differenceId)}/decisions`,
        toDirectReconciliationWire(input),
    )

    return {
        status: result.outcome === "RESULT_UNKNOWN" ? "unknown" : "succeeded",
        title: result.is_terminal ? "对账结论已登记" : "对账证据已追加",
        description: result.is_terminal
            ? "直接对账结论已登记；未完成或关闭任何处理任务。"
            : "差异处理记录已追加，当前差异仍待处理。",
        reference: result.resolution_record_id,
        outcome: result.outcome,
        stayOnItem: !result.is_terminal,
        terminal: result.is_terminal,
        facts: result.business_result_reference
            ? [
                  {
                      label: "业务结果",
                      value: result.business_result_reference,
                  },
              ]
            : undefined,
    }
}
