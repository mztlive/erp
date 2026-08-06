/**
 * W01 今日工作台 — 真实 HTTP（D03 work_item + /account/profile）。
 * 工作台汇总投影 / as_of 属 E3；缺省时 freshness 取列表中最新 created_at（服务端字段），
 * 不使用客户端时钟冒充 as_of。
 */

import { apiGet } from "@/lib/api"
import type { Page } from "@/lib/api"
import type { WorkspaceId } from "@/lib/workspace-registry"

import type {
  TodayWorkspaceQuery,
  TodayWorkspaceView,
  WorkspaceFamilyFilter,
  WorkspaceMetric,
  WorkspaceTaskGroup,
  WorkspaceWorkItem,
} from "./types"

/** 后端 work_item 列表行（WorkItemView）。 */
type WorkItemDto = {
  id: string
  work_item_type: string
  business_object_type: string
  business_object_id: string
  subject_version?: string | null
  status: string
  owner_role?: string | null
  owner_user_id?: string | null
  priority: string
  due_at?: number | null
  reason_code?: string | null
  impact_summary?: string | null
  completion_action: string
  completed_at?: number | null
  completed_by?: string | null
  close_reason_code?: string | null
  close_reason_text?: string | null
  version: number
  created_at: number
}

type AccountProfileDto = {
  userid: string
  account: string
  name: string
  role_ids: string[]
}

const TEMPORARY_PREVIEW_LIMIT = 5

const FAMILY_META: Record<
  WorkspaceFamilyFilter,
  { label: string; defaultExpanded: boolean }
> = {
  approval: { label: "审批与确认", defaultExpanded: true },
  finance: { label: "票款与结算", defaultExpanded: true },
  fulfillment: { label: "履约与库存", defaultExpanded: false },
  exception: { label: "数据治理与异常", defaultExpanded: false },
  procurement: { label: "采购确认", defaultExpanded: true },
}

const TYPE_META: Record<
  string,
  {
    label: string
    family: WorkspaceFamilyFilter
    destination: WorkspaceId
  }
> = {
  PROCUREMENT_CONFIRMATION: {
    label: "采购二次确认",
    family: "procurement",
    destination: "W07",
  },
  LOW_MARGIN_MANAGER_CONFIRMATION: {
    label: "低毛利上级确认",
    family: "approval",
    destination: "W05",
  },
  PURCHASE_ORDER_REVIEW: {
    label: "采购单财务审核",
    family: "finance",
    destination: "W08",
  },
  CARD_FUNDS_REVIEW: {
    label: "卡券票款复核",
    family: "finance",
    destination: "W13",
  },
  CARD_FUNDS_DELTA_REVIEW: {
    label: "卡券票款差异复核",
    family: "finance",
    destination: "W13",
  },
  CARD_SALES_MANAGER_APPROVAL: {
    label: "卡券销售领导审批",
    family: "approval",
    destination: "W05",
  },
  CARD_SALES_OPERATION_APPROVAL: {
    label: "卡券运营审批",
    family: "approval",
    destination: "W05",
  },
  OWNERSHIP_MIGRATION_SALES_CONFIRMATION: {
    label: "归属迁移销售确认",
    family: "approval",
    destination: "W03",
  },
  OWNERSHIP_MIGRATION_FINANCE_CONFIRMATION: {
    label: "归属迁移财务确认",
    family: "finance",
    destination: "W11",
  },
  INVENTORY_ADJUSTMENT_REVIEW: {
    label: "库存调整复核",
    family: "fulfillment",
    destination: "W10",
  },
  FINANCE_CORRECTION_REVIEW: {
    label: "财务纠错复核",
    family: "finance",
    destination: "W11",
  },
  SUPPLIER_SETTLEMENT_REVIEW: {
    label: "供应商结算复核",
    family: "finance",
    destination: "W27",
  },
  IMPORT_BUSINESS_CONFIRMATION: {
    label: "导入业务确认",
    family: "exception",
    destination: "W18",
  },
  INTEGRATION_RESULT_UNKNOWN: {
    label: "集成结果未知",
    family: "exception",
    destination: "W29",
  },
  BUSINESS_EXCEPTION: {
    label: "业务异常",
    family: "exception",
    destination: "W29",
  },
}

const PRIORITY_RANK: Record<string, number> = {
  urgent: 1,
  high: 2,
  normal: 3,
  low: 4,
}

const STATUS_LABEL: Record<string, { label: string; tone: WorkspaceWorkItem["statusTone"] }> = {
  UNCLAIMED: { label: "待领取", tone: "warning" },
  IN_PROGRESS: { label: "处理中", tone: "info" },
  COMPLETED: { label: "已完成", tone: "success" },
  CLOSED: { label: "已关闭", tone: "neutral" },
}

function unixToIso(secs?: number | null): string {
  if (secs == null || secs <= 0) return ""
  return new Date(secs * 1000).toISOString()
}

function formatRelativeLabel(iso: string, timezone: string): string {
  if (!iso) return "—"
  try {
    return new Intl.DateTimeFormat("zh-CN", {
      timeZone: timezone,
      month: "numeric",
      day: "numeric",
      hour: "2-digit",
      minute: "2-digit",
      hour12: false,
    }).format(new Date(iso))
  } catch {
    return iso
  }
}

function dueBucket(
  dueAtIso: string,
  timezone: string
): WorkspaceWorkItem["dueBucket"] {
  if (!dueAtIso) return "later"
  const due = new Date(dueAtIso).getTime()
  // 比较用服务端 due_at；无 as_of 时以 due 与本地日历日粗分桶仅供展示分组，
  // 不作为 projection as_of。
  const now = Date.now()
  if (due < now) return "overdue"
  try {
    const fmt = new Intl.DateTimeFormat("en-CA", {
      timeZone: timezone,
      year: "numeric",
      month: "2-digit",
      day: "2-digit",
    })
    const dueDay = fmt.format(new Date(dueAtIso))
    const todayDay = fmt.format(new Date(now))
    if (dueDay === todayDay) return "today"
  } catch {
    /* ignore */
  }
  return "later"
}

function mapWorkItem(
  dto: WorkItemDto,
  timezone: string,
  scope: TodayWorkspaceQuery["scope"]
): WorkspaceWorkItem {
  const meta = TYPE_META[dto.work_item_type] ?? {
    label: dto.work_item_type,
    family: "exception" as WorkspaceFamilyFilter,
    destination: "W02" as WorkspaceId,
  }
  const statusMeta = STATUS_LABEL[dto.status] ?? {
    label: dto.status,
    tone: "neutral" as const,
  }
  const createdAt = unixToIso(dto.created_at)
  const dueAt = unixToIso(dto.due_at)
  const bucket = dueBucket(dueAt, timezone)
  const processable =
    dto.status === "UNCLAIMED" || dto.status === "IN_PROGRESS"

  return {
    workItemId: dto.id,
    workItemType: dto.work_item_type,
    workItemTypeLabel: meta.label,
    businessObjectType: dto.business_object_type,
    businessObjectId: dto.business_object_id,
    stableNumber: dto.business_object_id,
    objectTitle: `${meta.label} · ${dto.business_object_id}`,
    counterpartyName: dto.business_object_type,
    status: dto.status,
    statusLabel: statusMeta.label,
    statusTone: statusMeta.tone,
    priority: PRIORITY_RANK[dto.priority] ?? 3,
    createdAt,
    dueAt,
    ownerRoleLabel: dto.owner_role ?? "未分配角色",
    ownerUserLabel: dto.owner_user_id ?? undefined,
    reasonLabel: dto.reason_code ?? "—",
    impactSummary: dto.impact_summary ?? "—",
    allowedActions: processable ? (["VIEW", "PROCESS"] as const) : (["VIEW"] as const),
    actionBlockers: processable
      ? []
      : [
          {
            action: "PROCESS",
            code: "TERMINAL",
            message: "任务已结束，不能继续处理。",
          },
        ],
    destinationWorkspaceId: meta.destination,
    queueContextId: `queue:W01:${scope}`,
    enteredAtLabel: formatRelativeLabel(createdAt, timezone),
    dueAtLabel:
      bucket === "overdue"
        ? "已超期"
        : formatRelativeLabel(dueAt, timezone),
    dueBucket: bucket,
    family: meta.family,
  }
}

function buildMetrics(items: readonly WorkspaceWorkItem[]): WorkspaceMetric[] {
  const mine = items.length
  const dueToday = items.filter((i) => i.dueBucket === "today").length
  const overdue = items.filter((i) => i.dueBucket === "overdue").length
  const exception = items.filter((i) => i.family === "exception").length
  return [
    {
      key: "mine",
      label: "待我处理",
      count: mine,
      visible: true,
      tone: "info",
    },
    {
      key: "due_today",
      label: "今日到期",
      count: dueToday,
      visible: true,
      tone: "warning",
    },
    {
      key: "overdue",
      label: "已超期",
      count: overdue,
      visible: true,
      tone: "destructive",
    },
    {
      key: "exception",
      label: "异常待处理",
      count: exception,
      visible: true,
      tone: "warning",
    },
  ]
}

function buildGroups(
  items: readonly WorkspaceWorkItem[],
  familyFilter?: WorkspaceFamilyFilter
): WorkspaceTaskGroup[] {
  const families = (
    Object.keys(FAMILY_META) as WorkspaceFamilyFilter[]
  ).filter((f) => !familyFilter || f === familyFilter)

  return families
    .map((family) => {
      const familyItems = items.filter((i) => i.family === family)
      const meta = FAMILY_META[family]
      return {
        family,
        label: meta.label,
        total: familyItems.length,
        pagePreviewLimit: TEMPORARY_PREVIEW_LIMIT,
        previewLimitSource: "TEMPORARY_FALLBACK" as const,
        defaultExpanded: meta.defaultExpanded,
        items: familyItems.slice(0, TEMPORARY_PREVIEW_LIMIT),
      }
    })
    .filter((g) => g.total > 0 || Boolean(familyFilter))
}

/**
 * 拉取今日工作台视图：任务列表来自 `/admin/work-items`，观众身份来自 `/account/profile`。
 */
export async function fetchWorkspaceDashboard(
  query: TodayWorkspaceQuery
): Promise<TodayWorkspaceView> {
  const profile = await apiGet<AccountProfileDto>("/account/profile")

  const listQuery: Record<string, unknown> = {
    page: 1,
    page_size: 100,
    sort_by: "due_at",
    sort_dir: "asc",
  }

  if (query.scope === "mine") {
    listQuery.owner_user_id = profile.userid
    listQuery.status = "IN_PROGRESS"
  } else {
    listQuery.status = "UNCLAIMED"
  }

  const page = await apiGet<Page<WorkItemDto>>("/admin/work-items", listQuery)

  let items = page.items.map((dto) =>
    mapWorkItem(dto, query.timezone, query.scope)
  )

  if (query.due === "today") {
    items = items.filter((i) => i.dueBucket === "today")
  } else if (query.due === "overdue") {
    items = items.filter((i) => i.dueBucket === "overdue")
  }
  if (query.family) {
    items = items.filter((i) => i.family === query.family)
  }

  // 新鲜度：取服务端 created_at 最大值；无 E3 工作台投影 as_of 时 projection 同源，不造客户端时间。
  const maxCreated = page.items.reduce(
    (max, row) => Math.max(max, row.created_at ?? 0),
    0
  )
  const updatedAt = maxCreated > 0 ? unixToIso(maxCreated) : ""

  return {
    access: "allowed",
    viewer: {
      userId: profile.userid,
      displayName: profile.name || profile.account,
      activeRoleLabel:
        profile.role_ids.length > 0 ? profile.role_ids.join("、") : "已登录",
      timezone: query.timezone,
    },
    freshness: {
      workItemsUpdatedAt: updatedAt,
      projectionUpdatedAt: updatedAt,
      projectionState: updatedAt ? "fresh" : "stale",
    },
    metrics: buildMetrics(items),
    groups: buildGroups(items, query.family),
    warnings: [],
    recent: [],
    canOpenTaskQueue: true,
    temporaryPreviewLimitFallback: TEMPORARY_PREVIEW_LIMIT,
  }
}
