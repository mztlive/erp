/**
 * W18 导入与期初 · 真实 HTTP API（P4 F8）。
 * 后端域：legacy_import（/admin/legacy-import-*）。
 * 导出签名保持与 queries.ts 一致；Page 形状在本文件内适配为 feature view。
 */

import { apiGet } from "@/lib/api"
import type { Page } from "@/lib/api/paging"
import type {
  ImportBatchListQuery,
  ImportBatchListView,
  ImportBatchStatus,
  ImportBatchView,
  ImportConfirmationView,
  ImportEnvironment,
  ImportIssueCode,
  ImportIssuePage,
  ImportIssueQuery,
  ImportObjectCode,
  ImportPipelineStage,
  IssueRowStatus,
} from "@/features/import-opening/types"
import {
  BATCH_STATUS_LABEL,
  OBJECT_CODE_LABEL,
} from "@/features/import-opening/types"

// ─── Backend DTOs ────────────────────────────────────────────────────────────

type BackendBatchListItem = {
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

type BackendBatchDetail = BackendBatchListItem & {
  successful_sanitized_file_asset_id?: string | null
  success_manifest_file_asset_id?: string | null
  failure_diagnostic_file_asset_id?: string | null
  source_file_hmac?: string | null
  background_job_id?: string | null
}

type BackendRow = {
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

type BackendConfirmation = {
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

function instantToIso(secs: number | null | undefined): string {
  if (secs == null || !Number.isFinite(secs)) return ""
  return new Date(secs * 1000).toISOString()
}

function mapBatchStatus(
  s: BackendBatchListItem["status"]
): { status: ImportBatchStatus; stage: ImportPipelineStage } {
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
function toBackendStatusFilter(
  status?: string
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
      return known in OBJECT_CODE_LABEL ? known : ("CUSTOMER" as ImportObjectCode)
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
  status: BackendConfirmation["status"]
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

function mapIssueCode(code?: string | null): ImportIssueCode {
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

function mapObjectType(raw: string): ImportObjectCode {
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

function mapRowStatus(row: BackendRow): IssueRowStatus | null {
  if (row.mapping_status === "conflict") return "CONFLICT"
  if (row.mapping_status === "pending_mapping") return "PENDING_MAPPING"
  if (row.import_status === "failed") return "FAILED"
  if (row.import_status === "skipped") return "SKIPPED"
  if (row.parse_status === "invalid") return "FAILED"
  return null
}

function toListItem(
  batch: BackendBatchListItem,
  env: ImportEnvironment
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

function buildBatchView(
  batch: BackendBatchDetail,
  confirmations: BackendConfirmation[],
  env: ImportEnvironment
): ImportBatchView {
  const { status, stage } = mapBatchStatus(batch.status)
  const formal =
    status === "SUCCEEDED" || status === "PARTIAL_SUCCESS"
  const confViews: ImportConfirmationView[] = confirmations.map((c) => {
    const scope = mapScope(c.confirmation_scope)
    return {
      scope,
      result: mapConfirmResult(c.status),
      confirmedByLabel: c.decided_by ?? undefined,
      confirmedAt: c.decided_at != null ? instantToIso(c.decided_at) : undefined,
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
        batch.total_rows - batch.success_rows - batch.failed_rows
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
function environmentFromQuery(env: ImportEnvironment): ImportEnvironment {
  return env
}

// ─── API ─────────────────────────────────────────────────────────────────────

export async function fetchImportBatchList(
  query: ImportBatchListQuery
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
    }
  )

  let rows = page.items.map((b) => toListItem(b, env))
  if (query.objectType && query.objectType !== "all") {
    const ot = query.objectType
    rows = rows.filter((r) => r.sourceObjectSet.includes(ot))
  }

  // 指标：再拉一页较大集合做计数（后端无 metrics 聚合端点）
  const metricsSource = await apiGet<Page<BackendBatchListItem>>(
    "/admin/legacy-import-batches",
    { page: 1, page_size: 100 }
  )
  const all = metricsSource.items
  const metrics = {
    pendingValidate: all.filter((b) =>
      ["pending_validation", "validating"].includes(b.status)
    ).length,
    pendingConfirm: all.filter((b) => b.status === "pending_confirmation")
      .length,
    applying: all.filter((b) => b.status === "importing").length,
    failedOrPartial: all.filter((b) =>
      ["partial_failed", "failed"].includes(b.status)
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

export async function fetchImportBatchDetail(input: {
  batchId: string
}): Promise<ImportBatchView | null> {
  let batch: BackendBatchDetail
  try {
    batch = await apiGet<BackendBatchDetail>(
      `/admin/legacy-import-batches/${input.batchId}`
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
    }
  )

  // environment 后端无字段 — 默认 PRODUCTION 展示（缺口见 evidence）
  return buildBatchView(batch, confPage.items, "PRODUCTION")
}

export async function fetchImportIssues(
  query: ImportIssueQuery
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
    }
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

export function formatObjectSet(codes: readonly string[]): string {
  return codes
    .map((c) => OBJECT_CODE_LABEL[c as keyof typeof OBJECT_CODE_LABEL] ?? c)
    .join("、")
}
