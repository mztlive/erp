import type {
  CostBasis,
  HistoryBackfillEnvironment,
  HistoryBackfillProcessingStatus,
  HistoryBackfillReportReviewStatus,
  HistoryBackfillView,
  ItemResult,
  JobSection,
  MallOrderFactType,
  ViewerRoleDemo,
} from "@/features/history-backfill/types"

const VIEWS = new Set<HistoryBackfillView>([
  "active",
  "processing_completed",
  "report_pending",
  "all",
])

const PROCESSING = new Set<HistoryBackfillProcessingStatus>([
  "DRAFT",
  "VALIDATING",
  "READY",
  "RUNNING",
  "PARTIAL",
  "COMPLETED",
  "FAILED",
])

const REPORT_REVIEW = new Set<HistoryBackfillReportReviewStatus>([
  "NOT_READY",
  "POLICY_NOT_CONFIGURED",
  "PENDING",
  "CONFIRMED",
  "REJECTED",
])

const ENVIRONMENTS = new Set<HistoryBackfillEnvironment>([
  "production",
  "verification",
])

const RESULTS = new Set<ItemResult>([
  "INSERTED",
  "DEDUPLICATED",
  "UNATTRIBUTED",
  "FAILED",
])

const FACT_TYPES = new Set<MallOrderFactType>([
  "PAYMENT_SUCCEEDED",
  "ORDER_CANCELED",
  "REFUND_SUCCEEDED",
  "ORDER_COMPLETED",
  "CARD_BALANCE_RESTORED",
])

const COST_BASES = new Set<CostBasis>(["ACTUAL", "STANDARD", "NONE"])

const SECTIONS = new Set<JobSection>([
  "overview",
  "facts",
  "dedupe",
  "unattributed",
  "cost",
  "failures",
  "report",
])

const ROLES = new Set<ViewerRoleDemo>([
  "SYSTEM_ADMIN",
  "FINANCE",
  "OPERATIONS",
  "NO_MODULE",
])

export type HistoryBackfillUrlState = {
  view: HistoryBackfillView
  mallId?: string
  environment?: HistoryBackfillEnvironment
  processingStatus?: HistoryBackfillProcessingStatus
  reportReviewStatus?: HistoryBackfillReportReviewStatus
  basis?: CostBasis
  q?: string
  page: number
  /** 列表页内嵌详情时使用；路由 :jobId 优先 */
  jobId?: string
  section: JobSection
  result?: ItemResult
  factType?: MallOrderFactType
  costBasis?: CostBasis
  role?: ViewerRoleDemo
}

export function parseHistoryBackfillSearchParams(
  searchParams: URLSearchParams | { get(name: string): string | null }
): HistoryBackfillUrlState {
  const viewRaw = searchParams.get("view")
  const view: HistoryBackfillView =
    viewRaw && VIEWS.has(viewRaw as HistoryBackfillView)
      ? (viewRaw as HistoryBackfillView)
      : "active"

  const mallId =
    searchParams.get("mallId") ?? searchParams.get("mall") ?? undefined

  const envRaw = searchParams.get("environment")
  const environment =
    envRaw && ENVIRONMENTS.has(envRaw as HistoryBackfillEnvironment)
      ? (envRaw as HistoryBackfillEnvironment)
      : undefined

  const psRaw = searchParams.get("processingStatus")
  const processingStatus =
    psRaw && PROCESSING.has(psRaw as HistoryBackfillProcessingStatus)
      ? (psRaw as HistoryBackfillProcessingStatus)
      : undefined

  const rrRaw = searchParams.get("reportReviewStatus")
  const reportReviewStatus =
    rrRaw && REPORT_REVIEW.has(rrRaw as HistoryBackfillReportReviewStatus)
      ? (rrRaw as HistoryBackfillReportReviewStatus)
      : undefined

  const basisRaw = searchParams.get("basis")
  const basis =
    basisRaw && COST_BASES.has(basisRaw as CostBasis)
      ? (basisRaw as CostBasis)
      : undefined

  const q = searchParams.get("q") ?? undefined

  const pageRaw = Number(searchParams.get("page") ?? "1")
  const page =
    Number.isFinite(pageRaw) && pageRaw >= 1 ? Math.floor(pageRaw) : 1

  const jobId = searchParams.get("jobId") ?? undefined

  const sectionRaw = searchParams.get("section")
  const section: JobSection =
    sectionRaw && SECTIONS.has(sectionRaw as JobSection)
      ? (sectionRaw as JobSection)
      : "overview"

  const resultRaw = searchParams.get("result")
  const result =
    resultRaw && RESULTS.has(resultRaw as ItemResult)
      ? (resultRaw as ItemResult)
      : undefined

  const factRaw = searchParams.get("factType")
  const factType =
    factRaw && FACT_TYPES.has(factRaw as MallOrderFactType)
      ? (factRaw as MallOrderFactType)
      : undefined

  const costRaw = searchParams.get("costBasis")
  const costBasis =
    costRaw && COST_BASES.has(costRaw as CostBasis)
      ? (costRaw as CostBasis)
      : undefined

  const roleRaw = searchParams.get("role")
  const role =
    roleRaw && ROLES.has(roleRaw as ViewerRoleDemo)
      ? (roleRaw as ViewerRoleDemo)
      : undefined

  return {
    view,
    mallId,
    environment,
    processingStatus,
    reportReviewStatus,
    basis,
    q,
    page,
    jobId,
    section,
    result,
    factType,
    costBasis,
    role,
  }
}

export function buildHistoryBackfillSearchParams(
  state: HistoryBackfillUrlState,
  options?: { omitJobId?: boolean }
): string {
  const params = new URLSearchParams()
  if (state.view !== "active") params.set("view", state.view)
  if (state.mallId) params.set("mallId", state.mallId)
  if (state.environment) params.set("environment", state.environment)
  if (state.processingStatus)
    params.set("processingStatus", state.processingStatus)
  if (state.reportReviewStatus)
    params.set("reportReviewStatus", state.reportReviewStatus)
  if (state.basis) params.set("basis", state.basis)
  if (state.q) params.set("q", state.q)
  if (state.page > 1) params.set("page", String(state.page))
  if (state.jobId && !options?.omitJobId) params.set("jobId", state.jobId)
  if (state.section !== "overview") params.set("section", state.section)
  if (state.result) params.set("result", state.result)
  if (state.factType) params.set("factType", state.factType)
  if (state.costBasis) params.set("costBasis", state.costBasis)
  if (state.role && state.role !== "SYSTEM_ADMIN") {
    params.set("role", state.role)
  }
  const qs = params.toString()
  return qs ? `?${qs}` : ""
}
