import type { StatusTone } from "@/components/ui/status-badge"
import type { WorkspaceId, WorkspaceMode } from "@/lib/workspace-registry"

export type MetricDef = Readonly<{
  key: string
  label: string
  value: string | number
  detail?: string
}>

export type ListColumnDef = Readonly<{
  key: string
  header: string
  numeric?: boolean
  status?: boolean
}>

export type ListRow = Readonly<{
  id: string
  cells: Readonly<Record<string, string>>
  status?: { label: string; tone: StatusTone }
  href?: string
  /** Metric strip keys this row belongs to (besides the default “all” metric). */
  metricTags?: readonly string[]
  /** Toolbar filter labels this row belongs to (besides the default first label). */
  filterTags?: readonly string[]
}>

export type QueueTask = Readonly<{
  id: string
  taskType: string
  businessObject: string
  counterparty: string
  enteredAt: string
  enteredDateTime: string
  dueAt: string
  dueDateTime: string
  responsibleParty: string
  reason: string
  impact: string
  status: { label: string; tone: StatusTone }
  summaryFields: readonly { label: string; value: string; numeric?: boolean }[]
  checkItems?: readonly string[]
  actionLabel?: string
  /**
   * When set, the primary action navigates to a specialized handler workspace
   * instead of running the generic queue complete mutation.
   */
  handlerHref?: string
  /** Scope labels this task appears under (besides the default first scope). */
  scopeTags?: readonly string[]
}>

export type ObjectListItem = Readonly<{
  id: string
  title: string
  subtitle: string
  code: string
  status: { label: string; tone: StatusTone }
  metrics?: readonly { label: string; value: string }[]
  sections?: readonly {
    id: string
    title: string
    fields: readonly { label: string; value: string }[]
  }[]
  owner?: string
  updatedAt?: string
  /** Scope labels this object appears under (besides the default first scope). */
  scopeTags?: readonly string[]
}>

export type AnalyticsSeriesPoint = Readonly<{
  label: string
  value: number
}>

export type AnalyticsPayload = Readonly<{
  metrics: readonly MetricDef[]
  series: readonly AnalyticsSeriesPoint[]
  seriesTitle: string
  tableTitle: string
  columns: readonly ListColumnDef[]
  rows: readonly ListRow[]
  notes?: readonly string[]
}>

export type GovernanceStage = Readonly<{
  key: string
  label: string
  status: "pending" | "current" | "complete" | "failed"
}>

export type GovernanceIssue = Readonly<{
  id: string
  severity: "error" | "warning" | "info"
  message: string
  objectLabel?: string
  field?: string
}>

export type GovernancePayload = Readonly<{
  stages: readonly GovernanceStage[]
  metrics: readonly MetricDef[]
  batches: readonly ListRow[]
  batchColumns: readonly ListColumnDef[]
  issues: readonly GovernanceIssue[]
  diffEntries?: readonly {
    id: string
    field: string
    before: string
    after: string
  }[]
}>

export type ListWorkspacePayload = Readonly<{
  metrics: readonly MetricDef[]
  columns: readonly ListColumnDef[]
  rows: readonly ListRow[]
  searchPlaceholder: string
  filterLabels?: readonly string[]
  primaryActionLabel?: string
}>

export type QueueWorkspacePayload = Readonly<{
  scopeLabels: readonly string[]
  tasks: readonly QueueTask[]
}>

export type ObjectWorkspacePayload = Readonly<{
  scopeLabels?: readonly string[]
  items: readonly ObjectListItem[]
  searchPlaceholder: string
  primaryActionLabel?: string
}>

export type WorkspacePageDef = Readonly<{
  id: WorkspaceId
  title: string
  description: string
  mode: WorkspaceMode
  breadcrumbs: readonly { id: string; label: string; href?: string }[]
  shell:
    | { kind: "list"; payload: ListWorkspacePayload }
    | { kind: "queue"; payload: QueueWorkspacePayload }
    | { kind: "object"; payload: ObjectWorkspacePayload }
    | { kind: "analytics"; payload: AnalyticsPayload }
    | { kind: "governance"; payload: GovernancePayload }
}>
