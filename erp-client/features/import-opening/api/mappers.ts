/**
 * W18 导入与期初 · 后端 DTO → feature view 映射（P4 F8）。
 * 仅纯函数，无 React；请求函数见 api/legacy-import.ts。
 */

import type {
    ImportBatchListView,
    ImportBatchStatus,
    ImportBatchView,
    ImportConfirmationView,
    ImportEnvironment,
    ImportIssueCode,
    ImportObjectCode,
    ImportPipelineStage,
    IssueRowStatus,
} from "@/features/import-opening/types"
import {
    BATCH_STATUS_LABEL,
    OBJECT_CODE_LABEL,
} from "@/features/import-opening/types"

// ─── Backend DTOs ────────────────────────────────────────────────────────────

export type BackendBatchListItem = {
    id: string
    batch_no: string
    source_system_id: string
    source_object_set: string
    baseline_date: string
    import_rule_version: string
    status:
        | "pending_validation"
        | "validating"
        | "pending_confirmation"
        | "importing"
        | "completed"
        | "partial_failed"
        | "failed"
    total_rows: number
    success_rows: number
    failed_rows: number
    failure_code_summary?: string | null
    confirmation_status_summary?: string | null
    version: number
    created_at: number
}

export type BackendBatchDetail = BackendBatchListItem & {
    successful_sanitized_file_asset_id?: string | null
    success_manifest_file_asset_id?: string | null
    failure_diagnostic_file_asset_id?: string | null
    source_file_hmac?: string | null
    background_job_id?: string | null
}

export type BackendRow = {
    id: string
    batch_id: string
    source_object_type: string
    source_row_key: string
    parse_status: "pending_parse" | "valid" | "invalid"
    mapping_status: "pending_mapping" | "mapped" | "conflict"
    import_status: "pending_import" | "imported" | "failed" | "skipped"
    external_identity_map_id?: string | null
    error_code?: string | null
    target_document_id?: string | null
    version: number
    created_at: number
}

export type BackendConfirmation = {
    id: string
    batch_id: string
    confirmation_scope: string
    owner_role: string
    batch_version: number
    trial_version: number
    status: "PENDING" | "CONFIRMED" | "REJECTED" | "INVALIDATED"
    decision?: "CONFIRM_SCOPE" | "RETURN_FOR_FIX" | null
    reason_code?: string | null
    comment?: string | null
    work_item_id: string
    decided_by?: string | null
    decided_at?: number | null
    version: number
    created_at: number
}

// ─── Mapping ─────────────────────────────────────────────────────────────────

export function instantToIso(secs: number | null | undefined): string {
    if (secs == null || !Number.isFinite(secs)) return ""
    return new Date(secs * 1000).toISOString()
}

function mapBatchStatus(s: BackendBatchListItem["status"]): {
    status: ImportBatchStatus
    stage: ImportPipelineStage
} {
    switch (s) {
        case "pending_validation":
            return { status: "RECEIVING", stage: "RECEIVE" }
        case "validating":
            return { status: "VALIDATING", stage: "VALIDATE" }
        case "pending_confirmation":
            return { status: "AWAITING_CONFIRMATION", stage: "CONFIRM" }
        case "importing":
            return { status: "APPLYING", stage: "APPLY" }
        case "completed":
            return { status: "SUCCEEDED", stage: "RESULT" }
        case "partial_failed":
            return { status: "PARTIAL_SUCCESS", stage: "RESULT" }
        case "failed":
            return { status: "FAILED", stage: "RESULT" }
        default:
            return { status: "FAILED", stage: "RESULT" }
    }
}

/** 前端筛选 status → 后端 status（无法精确覆盖的前端细态映射到最近后端态） */
export function toBackendStatusFilter(
    status?: string,
): BackendBatchListItem["status"] | undefined {
    if (!status || status === "all") return undefined
    const map: Record<string, BackendBatchListItem["status"]> = {
        RECEIVING: "pending_validation",
        SCANNING: "pending_validation",
        VALIDATING: "validating",
        TRIAL_READY: "validating",
        AWAITING_CONFIRMATION: "pending_confirmation",
        CONFIRMATION_BLOCKED: "pending_confirmation",
        READY_TO_APPLY: "pending_confirmation",
        APPLYING: "importing",
        PARTIAL_SUCCESS: "partial_failed",
        SUCCEEDED: "completed",
        FAILED: "failed",
        CANCELLED: "failed",
        pending_validation: "pending_validation",
        validating: "validating",
        pending_confirmation: "pending_confirmation",
        importing: "importing",
        completed: "completed",
        partial_failed: "partial_failed",
        failed: "failed",
    }
    return map[status]
}

function parseObjectSet(raw: string): ImportObjectCode[] {
    if (!raw.trim()) return []
    return raw
        .split(/[,;|/\s]+/)
        .map((p) => p.trim().toUpperCase())
        .filter(Boolean)
        .map((p) => {
            const known = p as ImportObjectCode
            return known in OBJECT_CODE_LABEL
                ? known
                : ("CUSTOMER" as ImportObjectCode)
        })
}

function mapScope(raw: string): ImportConfirmationView["scope"] {
    const u = raw.trim().toUpperCase()
    if (u.includes("SALES") || u.includes("销售")) return "SALES"
    if (u.includes("PROCURE") || u.includes("采购")) return "PROCUREMENT"
    if (u.includes("OPERAT") || u.includes("运营")) return "OPERATIONS"
    if (u.includes("WARE") || u.includes("仓储") || u.includes("仓"))
        return "WAREHOUSE"
    if (u.includes("FIN") || u.includes("财务")) return "FINANCE"
    return "OPERATIONS"
}

function mapConfirmResult(
    status: BackendConfirmation["status"],
): ImportConfirmationView["result"] {
    switch (status) {
        case "CONFIRMED":
            return "CONFIRMED"
        case "REJECTED":
            return "REJECTED"
        case "INVALIDATED":
            return "INVALIDATED"
        default:
            return "PENDING"
    }
}

export function mapIssueCode(code?: string | null): ImportIssueCode {
    if (!code) return "MAPPING_CONFLICT"
    const u = code.toUpperCase()
    const known: ImportIssueCode[] = [
        "CUSTOMER_NOT_FOUND",
        "AMOUNT_PRECISION",
        "BASELINE_DATE_MISMATCH",
        "HISTORY_FLOW_FORBIDDEN",
        "CARD_DRAFT_EXCLUDED",
        "MAPPING_CONFLICT",
        "QUALIFICATION_EXPIRED",
        "STOCK_QTY_INVALID",
    ]
    const hit = known.find((k) => u.includes(k) || k === u)
    return hit ?? "MAPPING_CONFLICT"
}

export function mapObjectType(raw: string): ImportObjectCode {
    const u = raw.trim().toUpperCase()
    if (u in OBJECT_CODE_LABEL) return u as ImportObjectCode
    if (u.includes("CUSTOMER") || u.includes("客户")) return "CUSTOMER"
    if (u.includes("CONTRACT") || u.includes("合同")) return "CONTRACT"
    if (u.includes("SUPPLIER") || u.includes("供应")) return "SUPPLIER"
    if (u.includes("WARE") || u.includes("仓")) return "WAREHOUSE"
    if (u.includes("STOCK") || u.includes("库存")) return "OPENING_STOCK"
    if (u.includes("SKU")) return "SKU"
    if (u.includes("CARD") && u.includes("CAT")) return "CARD_CATEGORY"
    if (u.includes("SALES")) return "CARD_SALES_ORDER"
    if (u.includes("AR") || u.includes("应收")) return "CARD_OPENING_AR"
    return "CUSTOMER"
}

export function mapRowStatus(row: BackendRow): IssueRowStatus | null {
    if (row.mapping_status === "conflict") return "CONFLICT"
    if (row.mapping_status === "pending_mapping") return "PENDING_MAPPING"
    if (row.import_status === "failed") return "FAILED"
    if (row.import_status === "skipped") return "SKIPPED"
    if (row.parse_status === "invalid") return "FAILED"
    return null
}

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
): ImportBatchView {
    const { status, stage } = mapBatchStatus(batch.status)
    const formal = status === "SUCCEEDED" || status === "PARTIAL_SUCCESS"
    const confViews: ImportConfirmationView[] = confirmations.map((c) => {
        const scope = mapScope(c.confirmation_scope)
        return {
            scope,
            result: mapConfirmResult(c.status),
            confirmedByLabel: c.decided_by ?? undefined,
            confirmedAt:
                c.decided_at != null ? instantToIso(c.decided_at) : undefined,
            trialVersion: String(c.trial_version),
            comment: c.comment ?? undefined,
            inViewerResponsibility: false,
        }
    })

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
        trialVersion: confViews[0]?.trialVersion ?? "0",
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
            allConfirmationsComplete:
                confViews.length > 0 &&
                confViews.every((c) => c.result === "CONFIRMED"),
            noBlockingIssues: batch.failed_rows === 0,
            trialVersionMatches: true,
            ruleVersionStable: true,
            workItemTypeRegistered: confViews.length > 0,
        },
        openingPolicyHints: [],
        allowedActions: [],
        actionBlockers: [],
        version: String(batch.version),
        updatedAt: instantToIso(batch.created_at),
        initiatorLabel: "—",
    }
}

/** 环境在后端批次上无字段；前端仍按 query.environment 标注视图（backend_gap）。 */
export function environmentFromQuery(
    env: ImportEnvironment,
): ImportEnvironment {
    return env
}
