import type {
  BatchSection,
  ImportEnvironment,
  ImportIssueCode,
  ImportObjectCode,
  IssueRowStatus,
  ViewerRoleDemo,
} from "@/features/import-opening/types"

const SECTIONS = new Set<BatchSection>([
  "overview",
  "files",
  "trial",
  "confirm",
  "progress",
  "result",
  "audit",
])

const ISSUE_CODES = new Set<ImportIssueCode>([
  "CUSTOMER_NOT_FOUND",
  "AMOUNT_PRECISION",
  "BASELINE_DATE_MISMATCH",
  "HISTORY_FLOW_FORBIDDEN",
  "CARD_DRAFT_EXCLUDED",
  "MAPPING_CONFLICT",
  "QUALIFICATION_EXPIRED",
  "STOCK_QTY_INVALID",
])

const OBJECT_CODES = new Set<ImportObjectCode>([
  "CUSTOMER",
  "CONTRACT",
  "SUPPLIER",
  "WAREHOUSE",
  "OPENING_STOCK",
  "SKU",
  "CARD_CATEGORY",
  "CARD_SALES_ORDER",
  "CARD_OPENING_AR",
])

const ROW_STATUSES = new Set<IssueRowStatus>([
  "PENDING_MAPPING",
  "CONFLICT",
  "FAILED",
  "SKIPPED",
])

export type ImportOpeningUrlState = {
  environment: ImportEnvironment
  status?: string
  objectType?: ImportObjectCode
  q?: string
  batchId?: string
  section: BatchSection
  issueCode?: ImportIssueCode
  issueObjectType?: ImportObjectCode
  rowStatus?: IssueRowStatus
  page: number
  /** Mock 角色演示，不写入业务记录 */
  role?: ViewerRoleDemo
}

export function parseImportOpeningSearchParams(
  searchParams: URLSearchParams | { get(name: string): string | null }
): ImportOpeningUrlState {
  const envRaw = searchParams.get("environment")
  const environment: ImportEnvironment =
    envRaw === "PRODUCTION" || envRaw === "production"
      ? "PRODUCTION"
      : "VALIDATION"

  const status = searchParams.get("status") ?? undefined
  const objectRaw = searchParams.get("objectType")
  const objectType =
    objectRaw && OBJECT_CODES.has(objectRaw as ImportObjectCode)
      ? (objectRaw as ImportObjectCode)
      : undefined
  const q = searchParams.get("q") ?? undefined
  const batchId =
    searchParams.get("batchId") ?? searchParams.get("id") ?? undefined

  const sectionRaw = searchParams.get("section")
  const section: BatchSection =
    sectionRaw && SECTIONS.has(sectionRaw as BatchSection)
      ? (sectionRaw as BatchSection)
      : "overview"

  const issueRaw = searchParams.get("issueCode")
  const issueCode =
    issueRaw && ISSUE_CODES.has(issueRaw as ImportIssueCode)
      ? (issueRaw as ImportIssueCode)
      : undefined

  const issueObjectRaw = searchParams.get("issueObject")
  const issueObjectType =
    issueObjectRaw && OBJECT_CODES.has(issueObjectRaw as ImportObjectCode)
      ? (issueObjectRaw as ImportObjectCode)
      : undefined

  const rowRaw = searchParams.get("rowStatus")
  const rowStatus =
    rowRaw && ROW_STATUSES.has(rowRaw as IssueRowStatus)
      ? (rowRaw as IssueRowStatus)
      : undefined

  const pageRaw = Number(searchParams.get("page") ?? "1")
  const page = Number.isFinite(pageRaw) && pageRaw >= 1 ? Math.floor(pageRaw) : 1

  const roleRaw = searchParams.get("role")
  const role: ViewerRoleDemo | undefined =
    roleRaw === "WAREHOUSE_CONFIRMER" ||
    roleRaw === "FINANCE_CONFIRMER" ||
    roleRaw === "SYSTEM_ADMIN"
      ? roleRaw
      : undefined

  return {
    environment,
    status,
    objectType,
    q,
    batchId,
    section,
    issueCode,
    issueObjectType,
    rowStatus,
    page,
    role,
  }
}

export function buildImportOpeningSearchParams(
  state: ImportOpeningUrlState
): string {
  const params = new URLSearchParams()
  if (state.environment !== "VALIDATION") {
    params.set("environment", state.environment)
  }
  if (state.status) params.set("status", state.status)
  if (state.objectType) params.set("objectType", state.objectType)
  if (state.q) params.set("q", state.q)
  if (state.batchId) params.set("batchId", state.batchId)
  if (state.batchId && state.section !== "overview") {
    params.set("section", state.section)
  }
  if (state.issueCode) params.set("issueCode", state.issueCode)
  if (state.issueObjectType) params.set("issueObject", state.issueObjectType)
  if (state.rowStatus) params.set("rowStatus", state.rowStatus)
  if (state.page > 1) params.set("page", String(state.page))
  if (state.role && state.role !== "SYSTEM_ADMIN") {
    params.set("role", state.role)
  }
  const qs = params.toString()
  return qs ? `?${qs}` : ""
}
