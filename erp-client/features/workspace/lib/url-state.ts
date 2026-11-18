import {
    parseWorkspaceFilters,
    type WorkspaceFilters,
} from "@/lib/workspace-search"
import type {
    TodayWorkspaceQuery,
    WorkspaceDueFilter,
    WorkspaceMetricKey,
    WorkspaceSort,
    WorkspaceViewFilter,
} from "@/features/workspace/types"
import { createUrlStateCodec } from "@/lib/url-state"
import { hasPermission } from "@/lib/permissions"
import { OVERVIEW_VIEWS, type OverviewView } from "./overview"
import type { PeriodPreset } from "@/features/actual-profit-loss/types"

const DUE_VALUES = ["today", "overdue"] as const
const VIEW_VALUES = ["inbox", "started", "managed", "history"] as const
const SORT_VALUES = ["priority_due", "due_asc", "created_desc"] as const
export const DEFAULT_WORKSPACE_SORT = "created_desc" satisfies WorkspaceSort

export type WorkspaceUrlState = {
    overviewView?: OverviewView
    overviewPeriod?: PeriodPreset
    filters?: WorkspaceFilters
    scopeVersion?: string
    view: WorkspaceViewFilter
    due?: WorkspaceDueFilter
    blocked?: boolean
    workItemType?: string
    query?: string
    handlerUserIds?: string
    salesOrderIds?: string
    purchaseOrderIds?: string
    sort: WorkspaceSort
    queue?: boolean
    page?: number
}

const codec = createUrlStateCodec<WorkspaceUrlState>([
    {
        key: "overview",
        name: "overviewView",
        type: "enum",
        values: OVERVIEW_VIEWS,
    },
    {
        key: "period",
        name: "overviewPeriod",
        type: "enum",
        values: ["month-to-date", "last-month", "quarter-to-date"],
    },
    {
        key: "filters",
        type: "custom",
        parse: (get) => parseWorkspaceFilters(get("filters")),
        build: (value) => (value ? JSON.stringify(value) : undefined),
    },
    { key: "scopeVersion", type: "string" },
    {
        key: "view",
        type: "enum",
        values: VIEW_VALUES,
        defaultValue: "inbox",
    },
    { key: "due", type: "enum", values: DUE_VALUES },
    { key: "blocked", type: "boolean", defaultValue: false },
    { key: "type", name: "workItemType", type: "string", trim: true },
    { key: "q", name: "query", type: "string", trim: true },
    {
        key: "handlerUserIds",
        type: "custom",
        parse: (get) => parseIdListParam(get("handlerUserIds")),
        build: (value) => buildIdListParam(value),
    },
    {
        key: "salesOrderIds",
        type: "custom",
        parse: (get) => parseIdListParam(get("salesOrderIds")),
        build: (value) => buildIdListParam(value),
    },
    {
        key: "purchaseOrderIds",
        type: "custom",
        parse: (get) => parseIdListParam(get("purchaseOrderIds")),
        build: (value) => buildIdListParam(value),
    },
    {
        key: "sort",
        type: "enum",
        values: SORT_VALUES,
        defaultValue: DEFAULT_WORKSPACE_SORT,
    },
    { key: "queue", type: "boolean", defaultValue: false },
    { key: "page", type: "number", defaultValue: 1, min: 1 },
])

export const parseWorkspaceSearchParams: typeof codec.parse = (params) => {
    const state = codec.parse(params)
    const filters: WorkspaceFilters = { ...state.filters }
    if (state.workItemType)
        filters.item_types = [
            `${state.view === "started" ? "approval" : "task"}:${state.workItemType}`,
        ]
    if (state.handlerUserIds) filters.handlers = state.handlerUserIds.split(",")
    if (state.salesOrderIds)
        filters.sales_orders = state.salesOrderIds.split(",")
    if (state.purchaseOrderIds)
        filters.purchase_orders = state.purchaseOrderIds.split(",")
    if (state.blocked) filters.blocked = true
    if (state.due) filters.due = state.due
    return {
        ...state,
        filters,
        workItemType: undefined,
        handlerUserIds: undefined,
        salesOrderIds: undefined,
        purchaseOrderIds: undefined,
        blocked: undefined,
        due: undefined,
    }
}
export const buildWorkspaceSearchParams = codec.build

// 「我发起的审批」的候选来自审批队列自身，任务队列使用独立目录。
const TASK_FILTER_PERMISSIONS: Partial<Record<keyof WorkspaceFilters, string>> =
    {
        handlers: "business_person:list",
        sales_orders: "sales_order:list",
        purchase_orders: "purchase_order:list",
    }

export function canUseWorkspaceFilter(
    view: WorkspaceViewFilter,
    key: keyof WorkspaceFilters,
    permissions?: readonly string[],
): boolean {
    const permission = TASK_FILTER_PERMISSIONS[key]
    return (
        view === "started" ||
        !permission ||
        hasPermission(permissions, permission)
    )
}

/** 已隐藏的筛选不能继续影响队列、分页或连续办理。 */
export function restrictWorkspaceFilters(
    state: WorkspaceUrlState,
    permissions?: readonly string[],
): WorkspaceUrlState {
    const unavailable = (
        Object.keys(TASK_FILTER_PERMISSIONS) as (keyof WorkspaceFilters)[]
    ).filter(
        (key) =>
            state.filters?.[key] &&
            !canUseWorkspaceFilter(state.view, key, permissions),
    )
    if (!unavailable.length) return state

    const filters = { ...state.filters }
    for (const key of unavailable) delete filters[key]
    return { ...state, filters, page: 1, scopeVersion: undefined }
}

/**
 * 处理人／来源订单筛选：逗号分隔的稳定 ID，只收窄授权结果。
 * 去重排序并上限 100 项，与服务端校验对齐；非法输入回未筛选。
 */
function parseIdListParam(raw: string | null): string | undefined {
    if (!raw) return undefined
    const ids = [...new Set(raw.split(",").map((part) => part.trim()))]
        .filter(Boolean)
        .sort()
        .slice(0, 100)
    return ids.length > 0 ? ids.join(",") : undefined
}

function buildIdListParam(value: unknown): string | undefined {
    if (typeof value !== "string") return undefined
    return parseIdListParam(value)
}

/**
 * 由 URL 推导当前选中的指标。
 */
export function metricKeyFromUrlState(
    state: Pick<WorkspaceUrlState, "view" | "due" | "blocked" | "filters">,
): WorkspaceMetricKey {
    if (state.view === "history") return "history"
    if (state.view === "started") return "started"
    if (state.view === "managed") return "managed"
    if (state.blocked || state.filters?.blocked) return "blocked"
    if (state.due === "overdue" || state.filters?.due === "overdue")
        return "overdue"
    return "inbox"
}

/**
 * 指标点击写入 URL，不跳页。
 */
export function urlStateFromMetricKey(
    key: WorkspaceMetricKey,
    current: WorkspaceUrlState,
): WorkspaceUrlState {
    switch (key) {
        case "overdue":
            return {
                ...current,
                view: "inbox",
                due: "overdue",
                blocked: false,
            }
        case "blocked":
            return {
                ...current,
                view: "inbox",
                due: undefined,
                blocked: true,
            }
        case "history":
            return {
                ...current,
                view: "history",
                due: undefined,
                blocked: false,
            }
        case "started":
            return {
                ...current,
                view: "started",
                due: undefined,
                blocked: false,
            }
        case "managed":
            return {
                ...current,
                view: "managed",
                due: undefined,
                blocked: false,
            }
        case "inbox":
        default:
            return {
                ...current,
                view: "inbox",
                due: undefined,
                blocked: false,
            }
    }
}

/**
 * URL 状态转列表查询。timezone 必须来自当前工作角色所属组织。
 * 详情身份由独立路由承载，不进入队列查询。
 */
export function toTodayWorkspaceQuery(
    state: WorkspaceUrlState,
    timezone: string,
): TodayWorkspaceQuery {
    return {
        view: state.view,
        filters: state.filters,
        scopeVersion: state.scopeVersion,
        due: state.due,
        blocked: state.blocked || undefined,
        workItemType: state.workItemType,
        query: state.query,
        handlerUserIds: state.handlerUserIds,
        salesOrderIds: state.salesOrderIds,
        purchaseOrderIds: state.purchaseOrderIds,
        sort: state.sort,
        page: state.page,
        timezone,
    }
}

/**
 * 左列标题。不得出现「团队待处理」。
 */
export function filterSummaryFor(key: WorkspaceMetricKey): string {
    switch (key) {
        case "overdue":
            return "已超期"
        case "blocked":
            return "受阻"
        case "history":
            return "处理记录"
        case "started":
            return "我发起的审批"
        case "managed":
            return "范围内待办"
        case "inbox":
        default:
            return "待我处理"
    }
}
