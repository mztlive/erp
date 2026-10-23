/** 工作台查询合同；所有匹配、候选搜索、计数和分页均由服务端执行。 */
export type WorkspaceFilters = {
    due?: "today" | "overdue"
    item_types?: string[]
    counterparties?: string[]
    handlers?: string[]
    sales_orders?: string[]
    purchase_orders?: string[]
    warehouses?: string[]
    business_types?: string[]
    statuses?: string[]
    date_field?: "created" | "due" | "completed"
    date_from?: string
    date_to?: string
    undated?: boolean
    blocked?: boolean
    option_query?: string
    include_options?: boolean
}
export type WorkspaceSearchOption = { value: string; label: string }
export type WorkspaceSearchOptions = Record<string, WorkspaceSearchOption[]>
export const WORKSPACE_FILTER_LABELS = {
    item_types: "事项类型",
    counterparties: "往来方",
    handlers: "处理人",
    sales_orders: "来源销售单",
    purchase_orders: "来源采购单",
    warehouses: "仓库",
    business_types: "业务类型",
    statuses: "办理状态",
} as const
export type WorkspaceChoiceKey = keyof typeof WORKSPACE_FILTER_LABELS

/** 只恢复本页有控件承载的条件；服务端继续执行完整输入校验。 */
export function parseWorkspaceFilters(
    value: string | null,
): WorkspaceFilters | undefined {
    if (!value) return undefined
    try {
        const parsed: unknown = JSON.parse(value)
        if (!parsed || typeof parsed !== "object" || Array.isArray(parsed))
            return undefined
        const raw = parsed as Record<string, unknown>
        const filters: WorkspaceFilters = {}
        for (const key of Object.keys(
            WORKSPACE_FILTER_LABELS,
        ) as WorkspaceChoiceKey[]) {
            const items = raw[key]
            if (
                Array.isArray(items) &&
                items.every((item) => typeof item === "string")
            )
                filters[key] = items
        }
        if (raw.due === "today" || raw.due === "overdue") filters.due = raw.due
        if (
            raw.date_field === "created" ||
            raw.date_field === "due" ||
            raw.date_field === "completed"
        )
            filters.date_field = raw.date_field
        if (typeof raw.date_from === "string") filters.date_from = raw.date_from
        if (typeof raw.date_to === "string") filters.date_to = raw.date_to
        if (raw.undated === true) filters.undated = true
        if (raw.blocked === true) filters.blocked = true
        return filters
    } catch {
        return undefined
    }
}

export function hasWorkspaceFilters(filters?: WorkspaceFilters) {
    return Boolean(
        filters &&
        (Object.keys(WORKSPACE_FILTER_LABELS).some(
            (key) => filters[key as WorkspaceChoiceKey]?.length,
        ) ||
            filters.date_from ||
            filters.date_to ||
            filters.undated ||
            filters.blocked ||
            filters.due),
    )
}
