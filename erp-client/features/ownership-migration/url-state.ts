import type {
  BatchStatus,
  BlockerCode,
  ConfirmationFilter,
  MigrationWizardStage,
  OverviewViewMode,
  ViewerRoleDemo,
} from "@/features/ownership-migration/types"
import { WIZARD_ORDER } from "@/features/ownership-migration/types"

const BATCH_STATUSES = new Set<BatchStatus>([
  "PREPARING",
  "AWAITING_CONFIRMATION",
  "FROZEN",
  "BASELINE_CONFIRMED",
  "EXECUTING",
  "COMPLETED",
  "FAILED",
])

const CONFIRMATION_FILTERS = new Set<ConfirmationFilter>([
  "pending_sales",
  "pending_finance",
  "pending_baseline",
  "invalidated",
])

const BLOCKERS = new Set<BlockerCode>([
  "MAPPING",
  "SINGLE_LINE",
  "FINANCE",
  "CARD_BASELINE",
  "SYNC_WATERMARK",
  "SCOPE_DRIFT",
])

const ROLES = new Set<ViewerRoleDemo>([
  "SYSTEM_ADMIN",
  "CUTOVER_OWNER",
  "SALES_CONFIRMER",
  "FINANCE_CONFIRMER",
  "BUSINESS_USER",
  "NO_MODULE",
])

const STAGES = new Set<MigrationWizardStage>(WIZARD_ORDER)

export type OwnershipMigrationUrlState = {
  mall?: string
  customer?: string
  status?: BatchStatus | "open"
  confirmation?: ConfirmationFilter
  blocker?: BlockerCode
  view?: OverviewViewMode
  q?: string
  page: number
  /** overview | batch detail | cutover */
  panel: "overview" | "batch" | "cutover"
  batchId?: string
  stage?: MigrationWizardStage
  role?: ViewerRoleDemo
  workItemId?: string
}

export function parseOwnershipMigrationSearchParams(
  searchParams: URLSearchParams | { get(name: string): string | null }
): OwnershipMigrationUrlState {
  const mall = searchParams.get("mall") ?? undefined
  const customer =
    searchParams.get("customer") ?? searchParams.get("customerId") ?? undefined

  const statusRaw = searchParams.get("status")
  let status: BatchStatus | "open" | undefined
  if (statusRaw === "open") status = "open"
  else if (statusRaw && BATCH_STATUSES.has(statusRaw as BatchStatus)) {
    status = statusRaw as BatchStatus
  }

  const confRaw = searchParams.get("confirmation")
  const confirmation =
    confRaw && CONFIRMATION_FILTERS.has(confRaw as ConfirmationFilter)
      ? (confRaw as ConfirmationFilter)
      : undefined

  const blockerRaw = searchParams.get("blocker")
  const blocker =
    blockerRaw && BLOCKERS.has(blockerRaw as BlockerCode)
      ? (blockerRaw as BlockerCode)
      : undefined

  const viewRaw = searchParams.get("view")
  const view: OverviewViewMode | undefined =
    viewRaw === "my-customers" || viewRaw === "my_customers"
      ? "my_customers"
      : viewRaw === "all"
        ? "all"
        : undefined

  const q = searchParams.get("q") ?? searchParams.get("search") ?? undefined

  const pageRaw = Number(searchParams.get("page") ?? "1")
  const page =
    Number.isFinite(pageRaw) && pageRaw >= 1 ? Math.floor(pageRaw) : 1

  const batchId =
    searchParams.get("batchId") ?? searchParams.get("batch") ?? undefined

  const cutover =
    searchParams.get("panel") === "cutover" ||
    searchParams.get("view") === "cutover" ||
    searchParams.get("cutover") === "1"

  const stageRaw = searchParams.get("stage")
  const stage =
    stageRaw && STAGES.has(stageRaw as MigrationWizardStage)
      ? (stageRaw as MigrationWizardStage)
      : stageRaw === "scope"
        ? "SCOPE"
        : stageRaw === "confirmations"
          ? "CONFIRMATIONS"
          : stageRaw === "baseline"
            ? "BASELINE"
            : stageRaw === "execution"
              ? "EXECUTION"
              : undefined

  const roleRaw = searchParams.get("role")
  const role =
    roleRaw && ROLES.has(roleRaw as ViewerRoleDemo)
      ? (roleRaw as ViewerRoleDemo)
      : undefined

  const workItemId = searchParams.get("workItemId") ?? undefined

  let panel: OwnershipMigrationUrlState["panel"] = "overview"
  if (cutover) panel = "cutover"
  else if (batchId) panel = "batch"

  return {
    mall,
    customer,
    status,
    confirmation,
    blocker,
    view,
    q,
    page,
    panel,
    batchId,
    stage,
    role,
    workItemId,
  }
}

export function buildOwnershipMigrationSearchParams(
  state: OwnershipMigrationUrlState
): string {
  const params = new URLSearchParams()
  if (state.mall) params.set("mall", state.mall)
  if (state.customer) params.set("customer", state.customer)
  if (state.status) params.set("status", state.status)
  if (state.confirmation) params.set("confirmation", state.confirmation)
  if (state.blocker) params.set("blocker", state.blocker)
  if (state.view === "my_customers") params.set("view", "my-customers")
  else if (state.view === "all") params.set("view", "all")
  if (state.q) params.set("q", state.q)
  if (state.page > 1) params.set("page", String(state.page))
  if (state.panel === "cutover") params.set("panel", "cutover")
  if (state.batchId) params.set("batchId", state.batchId)
  if (state.batchId && state.stage && state.stage !== "SCOPE") {
    params.set("stage", state.stage)
  }
  if (state.role && state.role !== "SYSTEM_ADMIN") {
    params.set("role", state.role)
  }
  if (state.workItemId) params.set("workItemId", state.workItemId)
  const qs = params.toString()
  return qs ? `?${qs}` : ""
}
