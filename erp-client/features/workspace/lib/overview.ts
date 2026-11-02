import { hasPermission } from "@/lib/permissions"
import type { AccountProfile } from "@/features/auth/api"
import type { SalesOrdersListQuery } from "@/features/sales-orders/api/contracts"
import type { PurchaseOrderListQuery } from "@/features/purchase-orders/api/purchase-orders-contract"
import type { PeriodPreset } from "@/features/actual-profit-loss/types"
import { resolvePeriod } from "@/features/actual-profit-loss/lib/url-state"

export const OVERVIEW_VIEWS = [
    "sales",
    "procurement",
    "finance",
    "fulfillment",
    "management",
] as const
export type OverviewView = (typeof OVERVIEW_VIEWS)[number]
export const OVERVIEW_LABELS: Record<OverviewView, string> = {
    sales: "销售业务",
    procurement: "采购业务",
    finance: "资金往来",
    fulfillment: "仓储与履约",
    management: "经营概览",
}
export type OverviewSource =
    | { kind: "sales"; query?: Partial<SalesOrdersListQuery> }
    | { kind: "purchase"; query?: PurchaseOrderListQuery }
    | { kind: "receivable" | "payable" | "receipt" | "payment" }
    | {
          kind: "inquiry"
          procurement?: boolean
          status?: string
          taskStatus?: "open"
      }
    | { kind: "fulfillment"; operationType: string; due?: "overdue" }
    | { kind: "profit"; from: string; to: string }

export type OverviewMetric = {
    key: string
    label: string
    detail: string
    permission: string
    source: OverviewSource
    field:
        | "count"
        | "balance"
        | "overdue"
        | "revenue"
        | "cost"
        | "profit"
        | "fulfillmentCost"
    warning?: boolean
}
export type OverviewRow = {
    id: string
    title: string
    description: string
    status?: string
    amount?: string | null
    amounts?: Partial<Record<OverviewMetric["field"], string | null>>
    href?: string
    taskId?: string
}
export type OverviewData = {
    count: number
    balance?: string | null
    overdue?: string | null
    revenue?: string | null
    cost?: string | null
    profit?: string | null
    fulfillmentCost?: string | null
    rows: OverviewRow[]
    scopeVersion?: string
    scopeSummary?: string
    asOf?: string
    noScope?: boolean
    limited?: boolean
    note?: string
    trend?: { label: string; value: string | null }[]
}

/** 模板按入口权限提供；服务端仍负责组织范围与金额字段裁剪。 */
export function availableOverviewViews(
    profile?: AccountProfile,
): OverviewView[] {
    if (!profile) return []
    const can = (permission: string) =>
        hasPermission(profile.permissions, permission)
    return OVERVIEW_VIEWS.filter((view) => {
        if (view === "sales")
            return can("sales_order:list") || can("sales_inquiry:list")
        if (view === "procurement")
            return can("purchase_order:list") || can("procurement_inquiry:list")
        if (view === "finance")
            return [
                "receivable_account:list",
                "payable_account:list",
                "customer_receipt:list",
                "supplier_payment:list",
            ].some(can)
        if (view === "management")
            return can("cost_entry:list") && can("sales_order:list")
        return (
            can("work_item:list") &&
            [
                "purchase_receipt:list",
                "delivery:list",
                "electronic_delivery:list",
                "service_fulfillment:list",
            ].some(can)
        )
    })
}

export function overviewPeriod(
    preset: PeriodPreset,
    custom?: { from: string; to: string },
) {
    const period = custom ?? resolvePeriod(preset)
    return {
        ...period,
        createdFrom: Date.parse(`${period.from}T00:00:00+08:00`) / 1000,
        createdTo: Date.parse(`${period.to}T23:59:59+08:00`) / 1000,
    }
}

export function overviewMetrics(
    view: OverviewView,
    preset: PeriodPreset,
    profile: AccountProfile,
    custom?: { from: string; to: string },
): OverviewMetric[] {
    const period = overviewPeriod(preset, custom)
    const sales: OverviewMetric[] = [
        {
            key: "sales-created",
            label: "新增销售单",
            detail: "按创建日期 · 含草稿",
            permission: "sales_order:list",
            source: { kind: "sales", query: period },
            field: "count",
        },
        {
            key: "sales-review",
            label: "审批中销售单",
            detail: "当前存量 · 全部期间",
            permission: "sales_order:list",
            source: {
                kind: "sales",
                query: { commercialStatus: "pending_review" },
            },
            field: "count",
        },
        {
            key: "sales-feedback",
            label: "待销售处理询价",
            detail: "当前存量 · 全部期间",
            permission: "sales_inquiry:list",
            source: { kind: "inquiry", status: "awaiting_sales" },
            field: "count",
        },
        {
            key: "receivable-balance",
            label: "应收未收",
            detail: "当前余额 · 含税",
            permission: "receivable_account:list",
            source: { kind: "receivable" },
            field: "balance",
        },
    ]
    const procurement: OverviewMetric[] = [
        {
            key: "purchase-inquiries",
            label: "待处理采购询价",
            detail: "当前开放询价任务",
            permission: "procurement_inquiry:list",
            source: { kind: "inquiry", procurement: true, taskStatus: "open" },
            field: "count",
        },
        {
            key: "purchase-review",
            label: "审批中采购单",
            detail: "当前存量 · 全部期间",
            permission: "purchase_order:list",
            source: { kind: "purchase", query: { metric: "review" } },
            field: "count",
        },
        {
            key: "purchase-fulfill",
            label: "已生效采购单",
            detail: "当前存量 · 全部期间",
            permission: "purchase_order:list",
            source: { kind: "purchase", query: { status: "EFFECTIVE" } },
            field: "count",
        },
        {
            key: "purchase-partial",
            label: "部分执行采购单",
            detail: "当前存量 · 全部期间",
            permission: "purchase_order:list",
            source: { kind: "purchase", query: { status: "PARTIAL" } },
            field: "count",
        },
    ]
    const finance: OverviewMetric[] = [
        sales[3]!,
        {
            key: "receivable-overdue",
            label: "逾期应收",
            detail: "当前逾期余额 · 含税",
            permission: "receivable_account:list",
            source: { kind: "receivable" },
            field: "overdue",
            warning: true,
        },
        {
            key: "payable-balance",
            label: "应付未付",
            detail: "当前余额 · 含税",
            permission: "payable_account:list",
            source: { kind: "payable" },
            field: "balance",
        },
        {
            key: "receipt-balance",
            label: "回款待分配",
            detail: "当前未分配余额 · 含税",
            permission: "customer_receipt:list",
            source: { kind: "receipt" },
            field: "balance",
        },
        {
            key: "payment-balance",
            label: "付款待分配",
            detail: "当前未分配余额 · 含税",
            permission: "supplier_payment:list",
            source: { kind: "payment" },
            field: "balance",
        },
    ]
    const fulfillment = [
        ["RECEIPT", "待收货", "purchase_receipt:list"],
        ["WAREHOUSE_SHIP", "待仓库发货", "delivery:list"],
        ["SUPPLIER_DIRECT", "待供应商直发", "delivery:list"],
        ["ELECTRONIC", "待电子交付", "electronic_delivery:list"],
        ["SERVICE", "待服务交付", "service_fulfillment:list"],
    ].map(([operationType, label, permission]): OverviewMetric => ({
        key: `fulfillment-${operationType}`,
        label: label!,
        permission: permission!,
        detail: "我的履约责任 · 当前未完成",
        source: { kind: "fulfillment", operationType: operationType! },
        field: "count",
    }))
    const management = (
        [
            ["revenue", "不含税销售收入"],
            ["cost", "实际采购成本"],
            ["fulfillmentCost", "实际履约费用"],
            ["profit", "实际经营盈亏"],
        ] as const
    ).map(([field, label]): OverviewMetric => ({
        key: `management-${field}`,
        label,
        permission: "cost_entry:list",
        detail: "非卡券 · 不含税 · 所选期间",
        source: { kind: "profit", from: period.from, to: period.to },
        field,
    }))
    const templates = { sales, procurement, finance, fulfillment, management }
    return templates[view]
        .filter((metric) =>
            hasPermission(profile.permissions, metric.permission),
        )
        .map((metric) => {
            if (view === "sales" && metric.source.kind === "sales")
                return {
                    ...metric,
                    detail: `本人负责 · ${metric.detail}`,
                    source: {
                        ...metric.source,
                        query: {
                            ...metric.source.query,
                            ownerUserIds: profile.userid,
                        },
                    },
                }
            if (view === "procurement" && metric.source.kind === "purchase")
                return {
                    ...metric,
                    detail: `本人负责 · ${metric.detail}`,
                    source: {
                        ...metric.source,
                        query: {
                            ...metric.source.query,
                            ownerUserIds: profile.userid,
                        },
                    },
                }
            return metric
        })
        .slice(0, 6)
}
