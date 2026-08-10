import type {
  CostBasis,
  HistoryBackfillEnvironment,
  HistoryBackfillProcessingStatus,
  HistoryBackfillReportReviewStatus,
  HistoryBackfillView,
  ItemResult,
  JobSection,
  MallOrderFactType,
} from "@/features/history-backfill/types"
import { createUrlStateCodec } from "@/lib/url-state"

const VIEW_VALUES = [
  "active",
  "processing_completed",
  "report_pending",
  "all",
] as const

const PROCESSING_VALUES = [
  "DRAFT",
  "VALIDATING",
  "READY",
  "RUNNING",
  "PARTIAL",
  "COMPLETED",
  "FAILED",
] as const

const REPORT_REVIEW_VALUES = [
  "NOT_READY",
  "POLICY_NOT_CONFIGURED",
  "PENDING",
  "CONFIRMED",
  "REJECTED",
] as const

const ENVIRONMENT_VALUES = ["production", "verification"] as const

const RESULT_VALUES = [
  "INSERTED",
  "DEDUPLICATED",
  "UNATTRIBUTED",
  "FAILED",
] as const

const FACT_TYPE_VALUES = [
  "PAYMENT_SUCCEEDED",
  "ORDER_CANCELED",
  "REFUND_SUCCEEDED",
  "ORDER_COMPLETED",
  "CARD_BALANCE_RESTORED",
] as const

const COST_BASIS_VALUES = ["ACTUAL", "STANDARD", "NONE"] as const

const SECTION_VALUES = [
  "overview",
  "facts",
  "dedupe",
  "unattributed",
  "cost",
  "failures",
  "report",
] as const

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
}

const codec = createUrlStateCodec<HistoryBackfillUrlState>([
  { key: "view", type: "enum", values: VIEW_VALUES, defaultValue: "active" },
  { key: "mallId", type: "string", aliases: ["mall"] },
  { key: "environment", type: "enum", values: ENVIRONMENT_VALUES },
  { key: "processingStatus", type: "enum", values: PROCESSING_VALUES },
  { key: "reportReviewStatus", type: "enum", values: REPORT_REVIEW_VALUES },
  { key: "basis", type: "enum", values: COST_BASIS_VALUES },
  { key: "q", type: "string" },
  { key: "page", type: "number", defaultValue: 1 },
  {
    key: "jobId",
    type: "custom",
    parse: (get) => get("jobId") ?? undefined,
    build: (value, options) =>
      value && !options?.omitJobId ? String(value) : undefined,
  },
  { key: "section", type: "enum", values: SECTION_VALUES, defaultValue: "overview" },
  { key: "result", type: "enum", values: RESULT_VALUES },
  { key: "factType", type: "enum", values: FACT_TYPE_VALUES },
  { key: "costBasis", type: "enum", values: COST_BASIS_VALUES },
])

export const parseHistoryBackfillSearchParams = codec.parse

export function buildHistoryBackfillSearchParams(
  state: HistoryBackfillUrlState,
  options?: { omitJobId?: boolean }
): string {
  return codec.build(state, options)
}
