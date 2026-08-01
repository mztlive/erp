import type { StatusTone } from "@/components/ui/status-badge"
import { sequentialText } from "@/lib/ui-text"
import type { WorkspaceId } from "@/lib/workspace-registry"

export type WorkspaceDueFilter = "today" | "overdue"
export type WorkspaceFamilyFilter =
  | "approval"
  | "finance"
  | "fulfillment"
  | "exception"

export type WorkspaceMetricKey =
  | "mine"
  | "due_today"
  | "overdue"
  | "exception"

export type WorkspaceActionCode = "VIEW" | "PROCESS"

export type WorkspaceWorkItem = Readonly<{
  workItemId: string
  workItemType: string
  workItemTypeLabel: string
  businessObjectType: string
  businessObjectId: string
  stableNumber: string
  objectTitle: string
  counterpartyName: string
  status: string
  statusLabel: string
  statusTone: StatusTone
  priority: number
  createdAt: string
  dueAt: string
  ownerRoleLabel: string
  ownerUserLabel?: string
  reasonLabel: string
  impactSummary: string
  allowedActions: readonly WorkspaceActionCode[]
  actionBlockers: readonly {
    action: WorkspaceActionCode
    code: string
    message: string
  }[]
  destinationWorkspaceId: WorkspaceId
  queueContextId: string
  enteredAtLabel: string
  dueAtLabel: string
  dueBucket: "today" | "overdue" | "later"
  family: WorkspaceFamilyFilter
}>

export type WorkspaceTaskGroup = Readonly<{
  family: WorkspaceFamilyFilter
  label: string
  total: number
  pagePreviewLimit?: number
  previewLimitSource?: "SERVER" | "TEMPORARY_FALLBACK"
  defaultExpanded: boolean
  items: readonly WorkspaceWorkItem[]
}>

export type WorkspaceMetric = Readonly<{
  key: WorkspaceMetricKey
  label: string
  count: number
  visible: boolean
  tone: StatusTone
  detail?: string
}>

export type WorkspaceWarning = Readonly<{
  warningId: string
  kind: string
  severity: "warning" | "destructive" | "info"
  title: string
  description: string
  occurredAt: string
  destinationWorkspaceId: WorkspaceId
  objectId?: string
}>

export type WorkspaceRecentItem = Readonly<{
  id: string
  label: string
  destinationWorkspaceId: WorkspaceId
  objectId?: string
  href: string
}>

export type TodayWorkspaceQuery = Readonly<{
  scope: "mine" | "role_pool"
  due?: WorkspaceDueFilter
  family?: WorkspaceFamilyFilter
  timezone: string
  scenario?: "forbidden" | "no_scope" | "empty"
}>

export type TodayWorkspaceView = Readonly<{
  access: "allowed" | "forbidden" | "no_data_scope"
  viewer: {
    userId: string
    displayName: string
    activeRoleLabel: string
    timezone: string
  }
  freshness: {
    workItemsUpdatedAt: string
    projectionUpdatedAt: string
    projectionState: "fresh" | "stale" | "failed" | "rebuilding" | "rebuilding"
  }
  metrics: readonly WorkspaceMetric[]
  groups: readonly WorkspaceTaskGroup[]
  warnings: readonly WorkspaceWarning[]
  recent: readonly WorkspaceRecentItem[]
  canOpenTaskQueue: boolean
  temporaryPreviewLimitFallback: number
}>

/** @deprecated Prefer WorkspaceWorkItem via buildTodayWorkspaceView */
export type WorkspaceTaskFilter = "all" | "today" | "overdue" | "sync"


const TEMPORARY_PREVIEW_LIMIT = 5
const FAMILY_META: Record<WorkspaceFamilyFilter, { label: string; defaultExpanded: boolean }> = {
    approval: {
        label: "审批与确认",
        defaultExpanded: true
    },
    finance: {
        label: "票款与结算",
        defaultExpanded: true
    },
    fulfillment: {
        label: "履约与库存",
        defaultExpanded: false
    },
    exception: {
        label: "数据治理与异常",
        defaultExpanded: false
    }
};
/**
 * Mock formal work items. `workItemType` is the fixed server type code;
 * family is assigned only for display grouping (W01 §4.3 / §8.2).
 */ const ALL_WORK_ITEMS: readonly WorkspaceWorkItem[] = [
    {
        workItemId: "wi_pc_01",
        workItemType: "PROCUREMENT_CONFIRMATION",
        workItemTypeLabel: "采购二次确认",
        businessObjectType: "SALES_ORDER",
        businessObjectId: "so_1001",
        stableNumber: "XS20260328001",
        objectTitle: "销售单 · XS20260328001",
        counterpartyName: "星河制造股份有限公司",
        status: "OPEN",
        statusLabel: "待处理",
        statusTone: "warning",
        priority: 80,
        createdAt: "2026-08-01T08:42:00+08:00",
        dueAt: "2026-08-01T11:30:00+08:00",
        ownerRoleLabel: "采购部",
        ownerUserLabel: "王敏",
        reasonLabel: "销售单已提交，供应商与成本信息待确认",
        impactSummary: "确认后生成采购执行任务并锁定成本口径",
        allowedActions: [
            "VIEW",
            "PROCESS"
        ],
        actionBlockers: [],
        destinationWorkspaceId: "W07",
        queueContextId: "queue:W07:mine",
        enteredAtLabel: "今天 08:42",
        dueAtLabel: "今天 11:30",
        dueBucket: "today",
        family: "approval"
    },
    {
        workItemId: "wi_pc_02",
        workItemType: "PROCUREMENT_CONFIRMATION",
        workItemTypeLabel: "采购二次确认",
        businessObjectType: "SALES_ORDER",
        businessObjectId: "so_1002",
        stableNumber: "XS20260327012",
        objectTitle: "销售单 · XS20260327012",
        counterpartyName: "北辰能源集团",
        status: "OPEN",
        statusLabel: "已超期",
        statusTone: "destructive",
        priority: 95,
        createdAt: "2026-07-31T16:18:00+08:00",
        dueAt: "2026-08-01T09:00:00+08:00",
        ownerRoleLabel: "采购部",
        ownerUserLabel: "王敏",
        reasonLabel: "客户履约日期提前，采购计划尚未确认",
        impactSummary: "可能影响 8 月 3 日首批交付",
        allowedActions: [
            "VIEW",
            "PROCESS"
        ],
        actionBlockers: [],
        destinationWorkspaceId: "W07",
        queueContextId: "queue:W07:mine",
        enteredAtLabel: "昨天 16:18",
        dueAtLabel: "已超期 42 分钟",
        dueBucket: "overdue",
        family: "approval"
    },
    {
        workItemId: "wi_margin_01",
        workItemType: "LOW_MARGIN_MANAGER_CONFIRMATION",
        workItemTypeLabel: "低毛利经理确认",
        businessObjectType: "SALES_ORDER",
        businessObjectId: "so_1003",
        stableNumber: "XS20260328005",
        objectTitle: "销售单 · XS20260328005",
        counterpartyName: "星河制造股份有限公司",
        status: "OPEN",
        statusLabel: "待处理",
        statusTone: "warning",
        priority: 70,
        createdAt: "2026-08-01T09:05:00+08:00",
        dueAt: "2026-08-01T17:00:00+08:00",
        ownerRoleLabel: "销售领导",
        ownerUserLabel: "王敏",
        reasonLabel: "毛利低于组织阈值，需经理确认后继续",
        impactSummary: "未确认前销售单不可进入履约",
        allowedActions: [
            "VIEW",
            "PROCESS"
        ],
        actionBlockers: [],
        destinationWorkspaceId: "W05",
        queueContextId: "queue:W05:mine",
        enteredAtLabel: "今天 09:05",
        dueAtLabel: "今天 17:00",
        dueBucket: "today",
        family: "approval"
    },
    {
        workItemId: "wi_pc_03",
        workItemType: "PROCUREMENT_CONFIRMATION",
        workItemTypeLabel: "采购二次确认",
        businessObjectType: "SALES_ORDER",
        businessObjectId: "so_1007",
        stableNumber: "XS20260328011",
        objectTitle: "销售单 · XS20260328011",
        counterpartyName: "启航传媒有限公司",
        status: "OPEN",
        statusLabel: "待处理",
        statusTone: "warning",
        priority: 55,
        createdAt: "2026-08-01T09:40:00+08:00",
        dueAt: "2026-08-01T15:00:00+08:00",
        ownerRoleLabel: "采购部",
        ownerUserLabel: "王敏",
        reasonLabel: "供应商交期待二次确认",
        impactSummary: "未确认前不可下推采购执行",
        allowedActions: [
            "VIEW",
            "PROCESS"
        ],
        actionBlockers: [],
        destinationWorkspaceId: "W07",
        queueContextId: "queue:W07:mine",
        enteredAtLabel: "今天 09:40",
        dueAtLabel: "今天 15:00",
        dueBucket: "today",
        family: "approval"
    },
    {
        workItemId: "wi_pc_04",
        workItemType: "PROCUREMENT_CONFIRMATION",
        workItemTypeLabel: "采购二次确认",
        businessObjectType: "SALES_ORDER",
        businessObjectId: "so_1008",
        stableNumber: "XS20260328014",
        objectTitle: "销售单 · XS20260328014",
        counterpartyName: "远景科技股份",
        status: "OPEN",
        statusLabel: "待处理",
        statusTone: "warning",
        priority: 50,
        createdAt: "2026-08-01T09:45:00+08:00",
        dueAt: "2026-08-01T16:30:00+08:00",
        ownerRoleLabel: "采购部",
        ownerUserLabel: "王敏",
        reasonLabel: "成本拆分明细不完整",
        impactSummary: "确认后锁定采购成本口径",
        allowedActions: [
            "VIEW",
            "PROCESS"
        ],
        actionBlockers: [],
        destinationWorkspaceId: "W07",
        queueContextId: "queue:W07:mine",
        enteredAtLabel: "今天 09:45",
        dueAtLabel: "今天 16:30",
        dueBucket: "today",
        family: "approval"
    },
    {
        workItemId: "wi_pc_05",
        workItemType: "PROCUREMENT_CONFIRMATION",
        workItemTypeLabel: "采购二次确认",
        businessObjectType: "SALES_ORDER",
        businessObjectId: "so_1009",
        stableNumber: "XS20260328018",
        objectTitle: "销售单 · XS20260328018",
        counterpartyName: "宏图实业",
        status: "OPEN",
        statusLabel: "待处理",
        statusTone: "info",
        priority: 45,
        createdAt: "2026-08-01T09:50:00+08:00",
        dueAt: "2026-08-01T17:30:00+08:00",
        ownerRoleLabel: "采购部",
        ownerUserLabel: "王敏",
        reasonLabel: "替代料方案待确认",
        impactSummary: "可能影响交付批次排程",
        allowedActions: [
            "VIEW",
            "PROCESS"
        ],
        actionBlockers: [],
        destinationWorkspaceId: "W07",
        queueContextId: "queue:W07:mine",
        enteredAtLabel: "今天 09:50",
        dueAtLabel: "今天 17:30",
        dueBucket: "today",
        family: "approval"
    },
    {
        workItemId: "wi_pc_06",
        workItemType: "PROCUREMENT_CONFIRMATION",
        workItemTypeLabel: "采购二次确认",
        businessObjectType: "SALES_ORDER",
        businessObjectId: "so_1010",
        stableNumber: "XS20260328021",
        objectTitle: "销售单 · XS20260328021",
        counterpartyName: "南岭贸易",
        status: "OPEN",
        statusLabel: "待处理",
        statusTone: "info",
        priority: 40,
        createdAt: "2026-08-01T09:55:00+08:00",
        dueAt: "2026-08-01T18:00:00+08:00",
        ownerRoleLabel: "采购部",
        ownerUserLabel: "王敏",
        reasonLabel: "包装规格与客户要求不一致",
        impactSummary: "确认后方可生成履约任务",
        allowedActions: [
            "VIEW",
            "PROCESS"
        ],
        actionBlockers: [],
        destinationWorkspaceId: "W07",
        queueContextId: "queue:W07:mine",
        enteredAtLabel: "今天 09:55",
        dueAtLabel: "今天 18:00",
        dueBucket: "today",
        family: "approval"
    },
    {
        workItemId: "wi_card_01",
        workItemType: "CARD_FUNDS_REVIEW",
        workItemTypeLabel: "卡券票款复核",
        businessObjectType: "SALES_ORDER",
        businessObjectId: "so_1004",
        stableNumber: "XS20260325008",
        objectTitle: "销售单 · XS20260325008",
        counterpartyName: "蓝湾集团",
        status: "OPEN",
        statusLabel: "待领取",
        statusTone: "info",
        priority: 90,
        createdAt: "2026-07-31T15:20:00+08:00",
        dueAt: "2026-08-01T09:00:00+08:00",
        ownerRoleLabel: "财务部",
        reasonLabel: "商城回款与开票金额待与 ERP 应收对齐",
        impactSummary: "未复核前票款数据不可作为经营结果",
        allowedActions: [
            "VIEW",
            "PROCESS"
        ],
        actionBlockers: [],
        destinationWorkspaceId: "W13",
        queueContextId: "queue:W13:mine",
        enteredAtLabel: "昨天 15:20",
        dueAtLabel: "已超期 1 小时",
        dueBucket: "overdue",
        family: "finance"
    },
    {
        workItemId: "wi_receipt_01",
        workItemType: "CUSTOMER_RECEIPT_REVIEW",
        workItemTypeLabel: "回款核对",
        businessObjectType: "SALES_ORDER",
        businessObjectId: "so_1005",
        stableNumber: "XS20260326009",
        objectTitle: "销售单 · XS20260326009",
        counterpartyName: "海纳教育科技有限公司",
        status: "OPEN",
        statusLabel: "待复核",
        statusTone: "warning",
        priority: 60,
        createdAt: "2026-08-01T09:34:00+08:00",
        dueAt: "2026-08-01T16:00:00+08:00",
        ownerRoleLabel: "财务部",
        ownerUserLabel: "陈琳",
        reasonLabel: "银行流水与客户结算主体名称存在简称差异",
        impactSummary: "复核前不计入已回款金额",
        allowedActions: [
            "VIEW"
        ],
        actionBlockers: [
            {
                action: "PROCESS",
                code: "ROLE_NOT_ASSIGNED",
                message: "当前角色可查看，处理权归财务复核岗"
            }
        ],
        destinationWorkspaceId: "W11",
        queueContextId: "queue:W11:mine",
        enteredAtLabel: "今天 09:34",
        dueAtLabel: "今天 16:00",
        dueBucket: "today",
        family: "finance"
    },
    {
        workItemId: "wi_fulfill_01",
        workItemType: "BUSINESS_EXCEPTION",
        workItemTypeLabel: "履约异常",
        businessObjectType: "SALES_ORDER",
        businessObjectId: "so_1006",
        stableNumber: "XS20260312008",
        objectTitle: "销售单 · XS20260312008",
        counterpartyName: "云帆物流集团",
        status: "IN_PROGRESS",
        statusLabel: "已超期",
        statusTone: "destructive",
        priority: 85,
        createdAt: "2026-07-31T14:06:00+08:00",
        dueAt: "2026-08-01T08:30:00+08:00",
        ownerRoleLabel: "履约组",
        ownerUserLabel: "刘青",
        reasonLabel: "电子交付批次尚未回传接收结果",
        impactSummary: "200 份夜班餐补包状态待确认",
        allowedActions: [
            "VIEW",
            "PROCESS"
        ],
        actionBlockers: [],
        destinationWorkspaceId: "W09",
        queueContextId: "queue:W09:mine",
        enteredAtLabel: "昨天 14:06",
        dueAtLabel: "已超期 1 小时",
        dueBucket: "overdue",
        family: "fulfillment"
    },
    {
        workItemId: "wi_sync_01",
        workItemType: "INTEGRATION_RESULT_UNKNOWN",
        workItemTypeLabel: "同步结果未知",
        businessObjectType: "MASTER_MAPPING_TASK",
        businessObjectId: "sync_017",
        stableNumber: "SYNC-20260801-017",
        objectTitle: "同步批次 · SYNC-20260801-017",
        counterpartyName: "商城销售单数据",
        status: "OPEN",
        statusLabel: "同步异常",
        statusTone: "destructive",
        priority: 75,
        createdAt: "2026-08-01T09:21:00+08:00",
        dueAt: "2026-08-01T10:21:00+08:00",
        ownerRoleLabel: "系统管理员",
        reasonLabel: "2 张卡券销售单的客户映射未命中",
        impactSummary: "ERP 数据更新延迟，商城原单不受影响",
        allowedActions: [
            "VIEW",
            "PROCESS"
        ],
        actionBlockers: [],
        destinationWorkspaceId: "W17",
        queueContextId: "queue:W17:mine",
        enteredAtLabel: "今天 09:21",
        dueAtLabel: "今天 10:21",
        dueBucket: "today",
        family: "exception"
    },
    {
        workItemId: "wi_map_01",
        workItemType: "BUSINESS_EXCEPTION",
        workItemTypeLabel: "映射异常处理",
        businessObjectType: "MASTER_MAPPING_TASK",
        businessObjectId: "map_044",
        stableNumber: "MAP-20260801-044",
        objectTitle: "映射任务 · MAP-20260801-044",
        counterpartyName: "华东商城",
        status: "CLAIMED",
        statusLabel: "处理中",
        statusTone: "info",
        priority: 65,
        createdAt: "2026-08-01T09:10:00+08:00",
        dueAt: "2026-08-01T18:00:00+08:00",
        ownerRoleLabel: "运营",
        ownerUserLabel: "李倩",
        reasonLabel: "外部商品缺少可销售项目映射",
        impactSummary: "阻断 12 条消费订单入账",
        allowedActions: [
            "VIEW",
            "PROCESS"
        ],
        actionBlockers: [],
        destinationWorkspaceId: "W21",
        queueContextId: "queue:W21:mine",
        enteredAtLabel: "今天 09:10",
        dueAtLabel: "今天 18:00",
        dueBucket: "today",
        family: "exception"
    }
];
const RECENT_WORK: readonly WorkspaceRecentItem[] = [
    {
        id: "recent_1",
        label: "XS20260328001 · 星河制造",
        destinationWorkspaceId: "W05",
        objectId: "so_1001",
        href: "/sales/orders/so_1001"
    },
    {
        id: "recent_2",
        label: "采购二次确认 · 待处理队列",
        destinationWorkspaceId: "W07",
        href: "/procurement/confirm"
    },
    {
        id: "recent_3",
        label: "XS20260312008 · 云帆物流",
        destinationWorkspaceId: "W05",
        objectId: "so_1006",
        href: "/sales/orders/so_1006"
    }
];
function sortWorkItems(items: readonly WorkspaceWorkItem[]): WorkspaceWorkItem[] {
    return [
        ...items
    ].sort((left, right) => {
        const leftOverdue = left.dueBucket === "overdue" ? 0 : 1;
        const rightOverdue = right.dueBucket === "overdue" ? 0 : 1;
        if (leftOverdue !== rightOverdue) return leftOverdue - rightOverdue;
        if (right.priority !== left.priority) return right.priority - left.priority;
        const dueLeft = left.dueAt ?? "9999";
        const dueRight = right.dueAt ?? "9999";
        if (dueLeft !== dueRight) return dueLeft.localeCompare(dueRight);
        return left.createdAt.localeCompare(right.createdAt);
    });
}
function filterItems(items: readonly WorkspaceWorkItem[], query: TodayWorkspaceQuery): WorkspaceWorkItem[] {
    return items.filter((item) => {
        if (query.due === "today" && item.dueBucket !== "today") return false;
        if (query.due === "overdue" && item.dueBucket !== "overdue") return false;
        if (query.family && item.family !== query.family) return false;
        return true;
    });
}
function filterByResponsibilityScope(
    items: readonly WorkspaceWorkItem[],
    query: TodayWorkspaceQuery
): WorkspaceWorkItem[] {
    // Mock the server-side responsibility boundary. Unassigned items belong to
    // a claimable role pool; assigned items are visible in `mine` only when the
    // assignee is the current viewer.
    return items.filter((item) =>
        query.scope === "mine"
            ? item.ownerUserLabel === "王敏"
            : item.ownerUserLabel == null
    )
}
function buildMetrics(
    items: readonly WorkspaceWorkItem[],
    scope: TodayWorkspaceQuery["scope"]
): WorkspaceMetric[] {
    // Metrics are server-side aggregates over the permission snapshot, not the
    // currently loaded preview page. Mock uses the full personal task set.
    const mine = items.length;
    const dueToday = items.filter((item) => item.dueBucket === "today").length;
    const overdue = items.filter((item) => item.dueBucket === "overdue").length;
    const exception = items.filter((item) => item.family === "exception").length;
    return [
        {
            key: "mine",
            label:
              scope === "mine"
                ? sequentialText.minePending
                : sequentialText.teamUnclaimed,
            count: mine,
            visible: true,
            tone: "neutral",
            detail: scope === "mine" ? "仅当前用户" : "当前角色可领取"
        },
        {
            key: "due_today",
            label: "今日到期",
            count: dueToday,
            visible: true,
            tone: "info",
            detail: "今天 18:00 前"
        },
        {
            key: "overdue",
            label: "已超期",
            count: overdue,
            visible: true,
            tone: overdue > 0 ? "destructive" : "neutral",
            detail: "需要优先处理"
        },
        {
            key: "exception",
            label: "同步异常",
            count: exception,
            visible: true,
            tone: exception > 0 ? "warning" : "neutral",
            detail: "影响数据更新时间"
        }
    ];
}
function buildGroups(items: readonly WorkspaceWorkItem[]): WorkspaceTaskGroup[] {
    const families: WorkspaceFamilyFilter[] = [
        "approval",
        "finance",
        "fulfillment",
        "exception",
    ]
    const limit = TEMPORARY_PREVIEW_LIMIT
    const groups: WorkspaceTaskGroup[] = []
    for (const family of families) {
        const familyItems = sortWorkItems(items.filter((item) => item.family === family))
        if (familyItems.length === 0) continue
        const meta = FAMILY_META[family]
        const hasOverdue = familyItems.some((item) => item.dueBucket === "overdue")
        groups.push({
            family,
            label: meta.label,
            total: familyItems.length,
            pagePreviewLimit: limit,
            previewLimitSource: "TEMPORARY_FALLBACK",
            defaultExpanded:
                family === "approval" ||
                (family === "finance" && familyItems.length > 0) ||
                (family === "fulfillment" && hasOverdue) ||
                (family === "exception" && hasOverdue),
            // Server returns only the preview page; total remains full count.
            items: familyItems.slice(0, limit),
        })
    }
    return groups
}
function buildWarnings(items: readonly WorkspaceWorkItem[]): WorkspaceWarning[] {
    const overdue = items.filter((item) => item.dueBucket === "overdue").length
    const sync = items.filter((item) => item.family === "exception")
    const warnings: WorkspaceWarning[] = []
    if (sync.length > 0) {
        warnings.push({
            warningId: "alert_sync",
            kind: "PROJECTION_DELAY",
            severity: "warning",
            title: "商城数据同步延迟",
            description: `最近成功同步于 09:18，${sync.length} 项映射/同步任务待处理。`,
            occurredAt: "2026-08-01T09:18:00+08:00",
            destinationWorkspaceId: "W17",
        })
    }
    if (overdue > 0) {
        warnings.push({
            warningId: "alert_due",
            kind: "OVERDUE_TASKS",
            severity: "destructive",
            title: `${overdue} 项任务已超期`,
            description: "最早超期约 1 小时，建议优先处理采购确认与履约异常。",
            occurredAt: "2026-08-01T09:00:00+08:00",
            destinationWorkspaceId: "W02",
        })
    }
    return warnings.slice(0, 5)
}
export function buildTodayWorkspaceView(query: TodayWorkspaceQuery): TodayWorkspaceView {
    if (query.scenario === "forbidden") {
        return {
            access: "forbidden",
            viewer: {
                userId: "user_wangmin",
                displayName: "王敏",
                activeRoleLabel: "销售",
                timezone: query.timezone
            },
            freshness: {
                workItemsUpdatedAt: new Date().toISOString(),
                projectionUpdatedAt: new Date().toISOString(),
                projectionState: "failed"
            },
            metrics: [],
            groups: [],
            warnings: [],
            recent: [],
            canOpenTaskQueue: false,
            temporaryPreviewLimitFallback: TEMPORARY_PREVIEW_LIMIT
        };
    }
    if (query.scenario === "no_scope") {
        return {
            access: "no_data_scope",
            viewer: {
                userId: "user_wangmin",
                displayName: "王敏",
                activeRoleLabel: "销售",
                timezone: query.timezone
            },
            freshness: {
                workItemsUpdatedAt: new Date().toISOString(),
                projectionUpdatedAt: new Date().toISOString(),
                projectionState: "fresh"
            },
            // No fake zero metrics when the role has no data scope (W01 §2.2 / §9).
            metrics: [],
            groups: [],
            warnings: [],
            recent: [],
            canOpenTaskQueue: false,
            temporaryPreviewLimitFallback: TEMPORARY_PREVIEW_LIMIT
        };
    }
    const permissionSnapshot = query.scenario === "empty" ? [] : ALL_WORK_ITEMS;
    const sourceItems = filterByResponsibilityScope(permissionSnapshot, query);
    const metrics = buildMetrics(sourceItems, query.scope);
    const filtered = filterItems(sourceItems, query);
    const groups = buildGroups(filtered);
    // Projection can lag formal work items by ≤1 min. Mock: projection is 90s
    // behind work items so the UI must mark it stale and not claim realtime.
    const workItemsUpdatedAt = new Date().toISOString();
    const projectionUpdatedAt = new Date(Date.now() - 90_000).toISOString();
    return {
        access: "allowed",
        viewer: {
            userId: "user_wangmin",
            displayName: "王敏",
            activeRoleLabel: "销售与采购协同",
            timezone: query.timezone
        },
        freshness: {
            workItemsUpdatedAt,
            projectionUpdatedAt,
            projectionState: "stale"
        },
        metrics,
        groups,
        warnings: buildWarnings(sourceItems),
        recent: query.scenario === "empty" ? RECENT_WORK : RECENT_WORK,
        canOpenTaskQueue: true,
        temporaryPreviewLimitFallback: TEMPORARY_PREVIEW_LIMIT
    };
}
