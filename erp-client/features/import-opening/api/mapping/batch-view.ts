/** W18 导入与期初 · DTO → feature 视图组装。 */

import type {
    ImportBatchDetailContext,
    ImportBatchListView,
    ImportBatchView,
    ImportConfirmationAllowedAction,
    ImportConfirmationView,
    ImportEnvironment,
    ImportExecutionAction,
} from "@/features/import-opening/types"
import { BATCH_STATUS_LABEL } from "@/features/import-opening/types"
import type {
    BackendBatchDetail,
    BackendBatchListItem,
    BackendConfirmation,
} from "./dto"
import {
    instantToIso,
    mapBatchStatus,
    mapConfirmResult,
    mapScope,
    parseObjectSet,
} from "./fields"

export function toListItem(
    batch: BackendBatchListItem,
    env: ImportEnvironment,
): ImportBatchListView["rows"][number] {
    const { status, stage } = mapBatchStatus(batch.status)
    return {
        batchId: batch.id,
        batchNo: batch.batch_no,
        environment: env,
        sourceObjectSet: parseObjectSet(batch.source_object_set),
        baselineDate: batch.baseline_date,
        importRuleVersion: batch.import_rule_version,
        stage,
        status,
        progressLabel:
            batch.total_rows > 0
                ? `${batch.success_rows}/${batch.total_rows}`
                : BATCH_STATUS_LABEL[status],
        confirmationSummary: batch.confirmation_status_summary ?? "—",
        initiatorLabel: "—",
        updatedAt: instantToIso(batch.created_at),
    }
}

export function buildBatchView(
    batch: BackendBatchDetail,
    confirmations: BackendConfirmation[],
    env: ImportEnvironment,
    context: ImportBatchDetailContext,
): ImportBatchView {
    const mappedBatch = mapBatchStatus(batch.status)
    const explicitTaskContext = Boolean(
        context.workItemId ||
        context.confirmationScope ||
        context.queueContextId,
    )
    const completeTaskContext = Boolean(
        context.workItemId &&
        context.confirmationScope &&
        context.queueContextId,
    )
    const stage = mappedBatch.stage
    const formal =
        mappedBatch.status === "SUCCEEDED" ||
        mappedBatch.status === "PARTIAL_SUCCESS"
    const confViews: ImportConfirmationView[] = confirmations.map((c) => {
        const scope = mapScope(c.confirmation_scope)
        const task = c.work_item
        const registered =
            task?.work_item_type === "IMPORT_BUSINESS_CONFIRMATION" &&
            task.handler_key === "import_business_confirmation" &&
            task.destination_workspace_id === "W18" &&
            task.work_item_id === c.work_item_id
        const focused = Boolean(
            completeTaskContext &&
            context.workItemId === c.work_item_id &&
            context.confirmationScope === scope,
        )
        const contextAllowsAction = !explicitTaskContext || focused
        const allowedActions = registered
            ? normalizeConfirmationActions(task.allowed_actions).filter(
                  (action) => contextAllowsAction || action === "VIEW",
              )
            : []
        return {
            confirmationId: c.id,
            scope,
            result: mapConfirmResult(c.status),
            confirmedByLabel: c.decided_by ?? undefined,
            confirmedAt:
                c.decided_at != null ? instantToIso(c.decided_at) : undefined,
            trialVersion: String(c.trial_version),
            comment: c.comment ?? undefined,
            inViewerResponsibility:
                allowedActions.includes("PROCESS") ||
                allowedActions.includes("CONFIRM_SCOPE") ||
                allowedActions.includes("RETURN_FOR_FIX"),
            focused,
            workItem:
                registered && task
                    ? {
                          workItemId: task.work_item_id,
                          taskVersion: task.task_version,
                          subjectVersion: task.subject_version,
                          status: task.status,
                          ownerUserId: task.owner_user_id ?? undefined,
                          processingState: task.processing_state,
                          allowedActions,
                          actionBlockers: task.action_blockers,
                      }
                    : undefined,
        }
    })
    const allTasksRegistered =
        confViews.length > 0 && confViews.every((item) => item.workItem != null)
    const focusedConfirmation = confViews.find((item) => item.focused)
    const taskContextInvalid =
        explicitTaskContext &&
        (!completeTaskContext || focusedConfirmation == null)
    const status =
        mappedBatch.status === "AWAITING_CONFIRMATION" &&
        (!allTasksRegistered || taskContextInvalid)
            ? "CONFIRMATION_BLOCKED"
            : mappedBatch.status
    const confirmationBlockers: Array<{
        action: string
        code: string
        message: string
    }> = []
    if (!allTasksRegistered && mappedBatch.status === "AWAITING_CONFIRMATION") {
        confirmationBlockers.push({
            action: "CONFIRM_SCOPE",
            code: "IMPORT_CONFIRMATION_TASK_MISSING",
            message:
                "当前试算的责任确认任务不完整，请联系管理员重新生成确认任务。",
        })
    }
    if (taskContextInvalid) {
        confirmationBlockers.push({
            action: "CONFIRM_SCOPE",
            code: "IMPORT_CONFIRMATION_CONTEXT_MISMATCH",
            message:
                "任务入口与当前批次责任范围不一致，请返回待处理列表重新打开。",
        })
    }
    const allConfirmationsComplete =
        confViews.length > 0 &&
        confViews.every((confirmation) => confirmation.result === "CONFIRMED")
    const executionActions: ImportExecutionAction[] = []
    if (
        status === "READY_TO_APPLY" &&
        allConfirmationsComplete &&
        allTasksRegistered &&
        batch.failed_rows === 0
    ) {
        executionActions.push("START_APPLY", "CANCEL_PENDING")
    }
    if (status === "APPLYING") {
        executionActions.push("CANCEL_PENDING")
    }
    if (
        (status === "PARTIAL_SUCCESS" || status === "FAILED") &&
        batch.failed_rows > 0
    ) {
        executionActions.push("RETRY_FAILED")
    }
    const activeTrialVersion = confViews
        .filter((confirmation) => confirmation.result !== "INVALIDATED")
        .reduce(
            (latest, confirmation) =>
                Math.max(latest, Number(confirmation.trialVersion) || 0),
            0,
        )

    return {
        batchId: batch.id,
        batchNo: batch.batch_no,
        environment: env,
        sourceSystem: {
            id: batch.source_system_id,
            name: batch.source_system_id,
        },
        sourceObjectSet: parseObjectSet(batch.source_object_set),
        baselineDate: batch.baseline_date,
        importRuleVersion: batch.import_rule_version,
        trialVersion: String(activeTrialVersion),
        stage,
        status,
        formalDataFormed: formal,
        notFormalDataMessage: formal
            ? ""
            : "尚未形成业务数据；上传/校验/确认完成前禁止当正式数据使用。",
        resultAssets: [],
        metrics: {
            total: batch.total_rows,
            valid: batch.success_rows,
            conflict: 0,
            failed: batch.failed_rows,
            skipped: Math.max(
                0,
                batch.total_rows - batch.success_rows - batch.failed_rows,
            ),
        },
        confirmations: confViews,
        backgroundJob: batch.background_job_id
            ? {
                  jobId: batch.background_job_id,
                  status:
                      status === "APPLYING"
                          ? "running"
                          : status === "SUCCEEDED"
                            ? "succeeded"
                            : status === "PARTIAL_SUCCESS"
                              ? "partial"
                              : status === "FAILED"
                                ? "failed"
                                : "queued",
                  mode: "partialAllowed",
                  total: batch.total_rows,
                  processed: batch.success_rows + batch.failed_rows,
                  succeeded: batch.success_rows,
                  skipped: 0,
                  failed: batch.failed_rows,
                  updatedAt: instantToIso(batch.created_at),
              }
            : undefined,
        productionGates: {
            validationEnvPassed: env === "PRODUCTION" ? true : true,
            allConfirmationsComplete,
            noBlockingIssues: batch.failed_rows === 0,
            trialVersionMatches: true,
            ruleVersionStable: true,
            workItemTypeRegistered: allTasksRegistered,
        },
        openingPolicyHints: [],
        allowedActions: [
            ...confViews.flatMap((item) => item.workItem?.allowedActions ?? []),
            ...executionActions,
        ],
        actionBlockers: confirmationBlockers,
        version: String(batch.version),
        updatedAt: instantToIso(batch.created_at),
        initiatorLabel: "—",
    }
}

/** 只接受 W18 已注册的责任与领域动作，未知后端值一律丢弃。 */
function normalizeConfirmationActions(
    actions: readonly string[],
): ImportConfirmationAllowedAction[] {
    const registered = new Set<ImportConfirmationAllowedAction>([
        "VIEW",
        "PROCESS",
        "REASSIGN",
        "CLOSE",
        "CONFIRM_SCOPE",
        "RETURN_FOR_FIX",
    ])
    return actions.filter((action): action is ImportConfirmationAllowedAction =>
        registered.has(action as ImportConfirmationAllowedAction),
    )
}

/** 环境在后端批次上无字段；前端仍按 query.environment 标注视图（backend_gap）。 */
export function environmentFromQuery(
    env: ImportEnvironment,
): ImportEnvironment {
    return env
}
