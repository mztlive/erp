import type {
  BatchSection,
  ImportEnvironment,
  ImportIssueCode,
  ImportObjectCode,
  IssueRowStatus,
  ViewerRoleDemo,
} from "@/features/import-opening/types"
import { createUrlStateCodec } from "@/lib/url-state"

const SECTION_VALUES = [
  "overview",
  "files",
  "trial",
  "confirm",
  "progress",
  "result",
  "audit",
] as const

const ISSUE_CODE_VALUES = [
  "CUSTOMER_NOT_FOUND",
  "AMOUNT_PRECISION",
  "BASELINE_DATE_MISMATCH",
  "HISTORY_FLOW_FORBIDDEN",
  "CARD_DRAFT_EXCLUDED",
  "MAPPING_CONFLICT",
  "QUALIFICATION_EXPIRED",
  "STOCK_QTY_INVALID",
] as const

const OBJECT_CODE_VALUES = [
  "CUSTOMER",
  "CONTRACT",
  "SUPPLIER",
  "WAREHOUSE",
  "OPENING_STOCK",
  "SKU",
  "CARD_CATEGORY",
  "CARD_SALES_ORDER",
  "CARD_OPENING_AR",
] as const

const ROW_STATUS_VALUES = [
  "PENDING_MAPPING",
  "CONFLICT",
  "FAILED",
  "SKIPPED",
] as const

const ROLE_VALUES = [
  "WAREHOUSE_CONFIRMER",
  "FINANCE_CONFIRMER",
  "SYSTEM_ADMIN",
] as const

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

const codec = createUrlStateCodec<ImportOpeningUrlState>([
  {
    key: "environment",
    type: "custom",
    parse: (get) => {
      const raw = get("environment")
      return raw === "PRODUCTION" || raw === "production"
        ? "PRODUCTION"
        : "VALIDATION"
    },
    build: (value) => (value === "VALIDATION" ? undefined : String(value)),
  },
  { key: "status", type: "string" },
  { key: "objectType", type: "enum", values: OBJECT_CODE_VALUES },
  { key: "q", type: "string" },
  { key: "batchId", type: "string", aliases: ["id"] },
  {
    key: "section",
    type: "enum",
    values: SECTION_VALUES,
    defaultValue: "overview",
    buildWhen: (value, state) =>
      value !== "overview" && Boolean(state.batchId),
  },
  { key: "issueCode", type: "enum", values: ISSUE_CODE_VALUES },
  {
    key: "issueObject",
    name: "issueObjectType",
    type: "enum",
    values: OBJECT_CODE_VALUES,
  },
  { key: "rowStatus", type: "enum", values: ROW_STATUS_VALUES },
  { key: "page", type: "number", defaultValue: 1 },
  {
    key: "role",
    type: "enum",
    values: ROLE_VALUES,
    buildWhen: (value) => Boolean(value) && value !== "SYSTEM_ADMIN",
  },
])

export const parseImportOpeningSearchParams = codec.parse
export const buildImportOpeningSearchParams = codec.build
