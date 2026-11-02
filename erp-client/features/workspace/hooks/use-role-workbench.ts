"use client"

import { useQueries, useQuery } from "@tanstack/react-query"
import type { AccountProfile } from "@/features/auth/api"
import { getWorkItemStats, listWorkItems } from "@/features/work-items/api"
import { queryKeyRoots } from "@/lib/query-key-roots"
import { roleWorkbench } from "@/lib/role-workbenches"
import { fetchOverviewData } from "../api/overview"
import { overviewIdentity } from "./use-workspace-overview"
import type { OverviewMetric } from "../lib/overview"

export function stageMetrics(id: string): OverviewMetric[] {
    if (id === "WB-04")
        return [
            {
                key: "receipt-balance",
                label: "回款待分配",
                detail: "当前未分配余额 · 含税 · 元",
                permission: "customer_receipt:list",
                source: { kind: "receipt" },
                field: "balance",
            },
            {
                key: "payment-balance",
                label: "付款待分配",
                detail: "当前未分配余额 · 含税 · 元",
                permission: "supplier_payment:list",
                source: { kind: "payment" },
                field: "balance",
            },
        ]
    if (id === "WB-01")
        return (
            [
                ["draft", "草稿"],
                ["pending_review", "审核中"],
                ["effective", "已生效"],
                ["voided", "已作废"],
            ] as const
        ).map(([status, label]) => ({
            key: `stage-${status}`,
            label,
            detail: "截至当前 · 单据数",
            permission: "sales_order:list",
            source: { kind: "sales", query: { commercialStatus: status } },
            field: "count",
        }))
    return (
        [
            ["DRAFT", "草稿"],
            ["PENDING_REVIEW", "审核中"],
            ["EFFECTIVE", "已生效"],
            ["PARTIAL", "部分执行"],
        ] as const
    ).map(([status, label]) => ({
        key: `stage-${status}`,
        label,
        detail: "截至当前 · 单据数",
        permission: "purchase_order:list",
        source: { kind: "purchase", query: { status } },
        field: "count",
    }))
}

export function useRoleWorkbench(profile: AccountProfile, id: string) {
    const template = roleWorkbench(id)!
    const identity = overviewIdentity(profile)
    const metrics = stageMetrics(id).map((metric): OverviewMetric => {
        if (metric.source.kind === "sales")
            return {
                ...metric,
                detail: `本人负责 · ${metric.detail}`,
                source: {
                    kind: "sales",
                    query: {
                        ...metric.source.query,
                        ownerUserIds: profile.userid,
                    },
                },
            }
        if (metric.source.kind === "purchase")
            return {
                ...metric,
                detail: `本人负责 · ${metric.detail}`,
                source: {
                    kind: "purchase",
                    query: {
                        ...metric.source.query,
                        ownerUserIds: profile.userid,
                    },
                },
            }
        return metric
    })
    const stages = useQueries({
        queries: metrics.map((metric) => ({
            queryKey: [
                ...queryKeyRoots.workspaceHome,
                "overview",
                identity,
                metric.source,
            ],
            queryFn: () => fetchOverviewData(metric.source, 1, undefined, 1),
            staleTime: 60_000,
            refetchInterval: 300_000,
            retry: false,
            meta: { roleWorkbench: true },
        })),
    })
    const stats = useQuery({
        queryKey: [...queryKeyRoots.workspaceHome, "role-task-stats", identity],
        queryFn: () =>
            getWorkItemStats({ scope: "mine", timezone: "Asia/Shanghai" }),
        staleTime: 30_000,
        refetchInterval: 60_000,
        retry: false,
        meta: { roleWorkbench: true },
    })
    const pages = useQueries({
        queries: template.tasks.map((type) => ({
            queryKey: [
                ...queryKeyRoots.workspaceHome,
                "role-task-list",
                identity,
                type,
            ],
            queryFn: () =>
                listWorkItems({
                    scope: "mine",
                    workItemType: type,
                    timezone: "Asia/Shanghai",
                    page: 1,
                    pageSize: 5,
                    sort: "priority_due",
                }),
            staleTime: 30_000,
            refetchInterval: 60_000,
            retry: false,
            meta: { roleWorkbench: true },
        })),
    })
    const tasks = pages.map((query, index) => {
        const type = template.tasks[index]!
        const timing = stats.data?.work_item_type_timing
        return {
            ...query,
            statsError: stats.isError,
            statsPending: stats.isPending,
            refetch: () => Promise.all([query.refetch(), stats.refetch()]),
            data: query.data ? { page: query.data } : undefined,
            stats: stats.data
                ? {
                      assigned: stats.data.work_item_type_counts
                          ? (stats.data.work_item_type_counts[type] ?? 0)
                          : undefined,
                      due_today: timing
                          ? (timing[type]?.due_today ?? 0)
                          : undefined,
                      overdue: timing
                          ? (timing[type]?.overdue ?? 0)
                          : undefined,
                      as_of: stats.data.as_of,
                  }
                : undefined,
        }
    })
    return { template, metrics, stages, tasks }
}
