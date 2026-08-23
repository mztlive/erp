/**
 * W18 导入与期初 · 真实 HTTP API（P4 F8）。
 * 后端域：legacy_import（/admin/legacy-import-*）。
 * 导出签名保持与 hooks/queries.ts 一致；Page 形状经 mappers.ts 适配为 feature view。
 */

import { apiGet, apiPost } from "@/lib/api"
import type { Page } from "@/lib/api/paging"
import {
    type BackendBatchDetail,
    type BackendBatchListItem,
    type BackendConfirmation,
    type BackendRow,
    buildBatchView,
    environmentFromQuery,
    instantToIso,
    mapIssueCode,
    mapObjectType,
    mapRowStatus,
    toBackendStatusFilter,
    toListItem,
} from "@/features/import-opening/api/mappers"
import type {
    ImportBatchListQuery,
    ImportBatchListView,
    ImportBatchDetailContext,
    ImportBatchView,
    ImportExecutionAction,
    ImportExecutionResult,
    ImportIssuePage,
    ImportIssueQuery,
} from "@/features/import-opening/types"

// ─── API ─────────────────────────────────────────────────────────────────────

export async function fetchImportBatchList(
    query: ImportBatchListQuery,
): Promise<ImportBatchListView> {
    const env = environmentFromQuery(query.environment)
    const backendStatus = toBackendStatusFilter(query.status)

    const page = await apiGet<Page<BackendBatchListItem>>(
        "/admin/legacy-import-batches",
        {
            page: query.page,
            page_size: query.pageSize,
            batch_no: query.q?.trim() || undefined,
            status: backendStatus,
        },
    )

    let rows = page.items.map((b) => toListItem(b, env))
    if (query.objectType && query.objectType !== "all") {
        const ot = query.objectType
        rows = rows.filter((r) => r.sourceObjectSet.includes(ot))
    }

    // 指标：再拉一页较大集合做计数（后端无 metrics 聚合端点）
    const metricsSource = await apiGet<Page<BackendBatchListItem>>(
        "/admin/legacy-import-batches",
        { page: 1, page_size: 100 },
    )
    const all = metricsSource.items
    const metrics = {
        pendingValidate: all.filter((b) =>
            ["pending_validation", "validating"].includes(b.status),
        ).length,
        pendingConfirm: all.filter((b) => b.status === "pending_confirmation")
            .length,
        applying: all.filter((b) => b.status === "importing").length,
        failedOrPartial: all.filter((b) =>
            ["partial_failed", "failed"].includes(b.status),
        ).length,
    }

    const asOf =
        all[0] != null
            ? instantToIso(all[0].created_at)
            : instantToIso(Math.floor(Date.now() / 1000))

    return {
        metrics,
        rows,
        totalCount: page.total,
        queriedAt: asOf,
    }
}

export async function fetchImportBatchDetail(
    input: ImportBatchDetailContext,
): Promise<ImportBatchView | null> {
    let batch: BackendBatchDetail
    try {
        batch = await apiGet<BackendBatchDetail>(
            `/admin/legacy-import-batches/${input.batchId}`,
        )
    } catch {
        return null
    }

    const confPage = await apiGet<Page<BackendConfirmation>>(
        "/admin/legacy-import-confirmations",
        {
            page: 1,
            page_size: 50,
            batch_id: input.batchId,
        },
    )

    // environment 后端无字段 — 默认 PRODUCTION 展示（缺口见 evidence）
    return buildBatchView(batch, confPage.items, "PRODUCTION", input)
}

export type CompleteImportConfirmationInput = Readonly<{
    batchId: string
    batchVersion: string
    trialVersion: string
    confirmationScope: string
    workItemId: string
    taskVersion: string
    subjectVersion: string
    action: "CONFIRM_SCOPE" | "RETURN_FOR_FIX"
    reasonCode?: string
    comment?: string
    idempotencyKey: string
}>

/** 提交 W18 唯一强类型业务确认命令；领域事实与任务终态由服务端同事务写入。 */
export async function completeImportConfirmation(
    input: CompleteImportConfirmationInput,
): Promise<void> {
    await apiPost("/admin/legacy-import-confirmations/complete", {
        work_item_id: input.workItemId,
        expected_task_version: input.taskVersion,
        expected_subject_version: input.subjectVersion,
        decision: {
            batch_id: input.batchId,
            expected_batch_version: input.batchVersion,
            expected_trial_version: input.trialVersion,
            confirmation_scope: input.confirmationScope,
            action: input.action,
            reason_code: input.reasonCode,
            comment: input.comment,
        },
        idempotency_key: input.idempotencyKey,
    })
}

export type ExecuteImportCommandInput = Readonly<{
    batchId: string
    expectedBatchVersion: string
    expectedTrialVersion?: string
    action: ImportExecutionAction
    reasonCode?: string
    comment?: string
    requestId: string
}>

type BackendImportExecutionResult = {
    action: ImportExecutionAction
    result_status: ImportExecutionResult["resultStatus"]
    batch_id: string
    batch_status: string
    batch_version: string
    trial_version?: string | null
    background_job_id: string
    background_job_status: string
    background_job_version: string
    affected_items: number
    next_step: ImportExecutionResult["nextStep"]
    audit_receipt: string
}

/** 执行独立导入应用强命令；业务确认本身不得调用本端点启动后台任务。 */
export async function executeImportCommand(
    input: ExecuteImportCommandInput,
): Promise<ImportExecutionResult> {
    const result = await apiPost<BackendImportExecutionResult>(
        `/admin/legacy-import-batches/${encodeURIComponent(input.batchId)}/commands`,
        {
            batch_id: input.batchId,
            expected_batch_version: input.expectedBatchVersion,
            expected_trial_version: input.expectedTrialVersion,
            action: input.action,
            reason_code: input.reasonCode,
            comment: input.comment,
            request_id: input.requestId,
        },
    )
    return {
        action: result.action,
        resultStatus: result.result_status,
        batchId: result.batch_id,
        batchStatus: result.batch_status,
        batchVersion: result.batch_version,
        trialVersion: result.trial_version ?? undefined,
        backgroundJobId: result.background_job_id,
        backgroundJobStatus: result.background_job_status,
        backgroundJobVersion: result.background_job_version,
        affectedItems: result.affected_items,
        nextStep: result.next_step,
        auditReceipt: result.audit_receipt,
    }
}

export async function fetchImportIssues(
    query: ImportIssueQuery,
): Promise<ImportIssuePage> {
    const page = await apiGet<Page<BackendRow>>(
        `/admin/legacy-import-batches/${query.batchId}/rows`,
        {
            page: query.page,
            page_size: query.pageSize,
            source_object_type:
                query.objectType && query.objectType !== "all"
                    ? query.objectType
                    : undefined,
        },
    )

    let rows = page.items
        .map((row) => {
            const rowStatus = mapRowStatus(row)
            if (!rowStatus) return null
            return {
                issueId: row.id,
                batchId: row.batch_id,
                issueCode: mapIssueCode(row.error_code),
                objectType: mapObjectType(row.source_object_type),
                sourceRowNo: Number.parseInt(row.source_row_key, 10) || 0,
                sourceColumnName: row.source_object_type,
                rowStatus,
                errorDetail: row.error_code ?? rowStatus,
                repairable:
                    rowStatus === "FAILED" ||
                    rowStatus === "CONFLICT" ||
                    rowStatus === "PENDING_MAPPING",
            }
        })
        .filter((r): r is NonNullable<typeof r> => r != null)

    if (query.issueCode && query.issueCode !== "all") {
        rows = rows.filter((r) => r.issueCode === query.issueCode)
    }
    if (query.rowStatus && query.rowStatus !== "all") {
        rows = rows.filter((r) => r.rowStatus === query.rowStatus)
    }

    const asOf =
        page.items[0] != null
            ? instantToIso(page.items[0].created_at)
            : instantToIso(Math.floor(Date.now() / 1000))

    return {
        rows,
        totalCount: rows.length,
        issueVersion: `issv-${query.batchId}-${page.page}`,
        queriedAt: asOf,
    }
}
