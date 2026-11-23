"use client"

import { useMemo } from "react"
import { useQuery } from "@tanstack/react-query"
import type { AccountProfile } from "@/features/auth/api"
import { getWorkItemStats, type WorkItemStats } from "@/features/work-items/api"
import { getErrorPresentation } from "@/lib/api/errors"
import { hasPermission } from "@/lib/permissions"
import { readProblemCopy } from "@/lib/system-feedback"
import type { WorkspaceFilters } from "@/lib/workspace-search"
import {
    EXCEPTION_WORK_ITEM_TYPES,
    EXECUTION_WORK_ITEM_TYPES,
} from "../components/workspace-type-nav"
import type { WorkspaceUrlState } from "../lib/url-state"
import { workspaceHomeKeys } from "./queries"

const feedback = {
    subject: "筛选项的事项数量",
    impact: "筛选和事项列表仍可使用。",
}

function sumValues(
    record: Readonly<Record<string, number>> | undefined,
): number {
    if (!record) return 0
    return Object.values(record).reduce((acc, value) => acc + value, 0)
}

function deriveWorkItemCounts(
    stats: WorkItemStats | undefined,
    due: WorkspaceFilters["due"],
    selectedItemTypes: readonly string[] | undefined,
    fallbackTotal: number,
): Record<string, number | undefined> {
    if (!stats) return {}

    const typeCount = (workItemType: string): number => {
        if (due === "overdue") {
            return stats.work_item_type_timing?.[workItemType]?.overdue ?? 0
        }
        if (due === "today") {
            return stats.work_item_type_timing?.[workItemType]?.due_today ?? 0
        }
        return stats.work_item_type_counts?.[workItemType] ?? 0
    }

    const counts: Record<string, number | undefined> = {}

    const approvalCount = typeCount("DOCUMENT_APPROVAL")
    const reworkCount = typeCount("APPROVAL_REWORK")
    const executionCount = EXECUTION_WORK_ITEM_TYPES.reduce(
        (acc, type) => acc + typeCount(type),
        0,
    )
    const exceptionCount = EXCEPTION_WORK_ITEM_TYPES.reduce(
        (acc, type) => acc + typeCount(type),
        0,
    )
    const allCount = due
        ? Object.values(stats.work_item_type_timing ?? {}).reduce(
              (acc, entry) =>
                  acc + (due === "overdue" ? entry.overdue : entry.due_today),
              0,
          )
        : stats.work_item_type_counts
          ? sumValues(stats.work_item_type_counts)
          : (stats.total ??
            (!selectedItemTypes?.length ? fallbackTotal : undefined))

    counts["bucket:all"] = allCount
    counts["types-all"] = allCount
    counts["bucket:approval"] = approvalCount
    counts["bucket:execution"] = executionCount
    counts["bucket:exception"] = exceptionCount
    counts["bucket:rework"] = reworkCount
    counts["task:DOCUMENT_APPROVAL"] = approvalCount
    counts["task:APPROVAL_REWORK"] = reworkCount

    for (const type of EXECUTION_WORK_ITEM_TYPES) {
        counts[`task:${type}`] = typeCount(type)
    }
    for (const type of EXCEPTION_WORK_ITEM_TYPES) {
        counts[`task:${type}`] = typeCount(type)
    }

    if (!due) {
        for (const [key, count] of Object.entries(
            stats.approval_document_type_counts ?? {},
        )) {
            counts[key.startsWith("approval:") ? key : `approval:${key}`] =
                count
        }
        for (const [key, count] of Object.entries(
            stats.item_type_counts ?? {},
        )) {
            if (counts[key] === undefined) {
                counts[key] = count
            }
        }
    }

    counts["due-all"] = stats.total ?? (!due ? fallbackTotal : undefined)
    counts["overdue"] = stats.overdue
    counts["today"] = stats.due_today

    return counts
}

function deriveApprovalCounts(
    documentTypeCounts: Readonly<Record<string, number>> | undefined,
    selectedItemTypes: readonly string[] | undefined,
    fallbackTotal: number,
): Record<string, number | undefined> {
    const counts: Record<string, number | undefined> = {}
    if (!documentTypeCounts) {
        if (!selectedItemTypes?.length) {
            counts["types-all"] = fallbackTotal
        }
        return counts
    }
    let total = 0
    for (const [docType, count] of Object.entries(documentTypeCounts)) {
        const key = docType.startsWith("approval:")
            ? docType
            : `approval:${docType}`
        counts[key] = count
        total += count
    }
    counts["types-all"] = !selectedItemTypes?.length ? fallbackTotal : total
    return counts
}

/**
 * 队列聚合与筛选计数：
 * - 任务队列（`inbox` / `managed` / `history`）：单次 `GET /admin/work-items/stats` 返回全部桶、类型与期限计数，彻底淘汰 8 路并发请求；
 * - 审批视图（`started` / `mine` / `blocked` / `managed`）：直接消费审批列表单次响应的 `document_type_counts`，零额外请求。
 */
export function useWorkspaceQueueCounts(
    state: WorkspaceUrlState,
    total: number,
    profile?: AccountProfile,
    approvalDocumentTypeCounts?: Readonly<Record<string, number>>,
) {
    const isApprovalQueue = state.view === "started"
    const dueFilter = state.filters?.due
    const selectedItemTypes = state.filters?.item_types
    // 统计请求省略 due，使单次 stats 同时返回 due-all / overdue / today 及按类型 timing 分布。
    const statsFilters = useMemo<WorkspaceFilters>(
        () => ({
            ...state.filters,
            due: undefined,
            include_options: undefined,
            option_query: undefined,
        }),
        [state.filters],
    )
    const scope = state.view === "inbox" ? "mine" : state.view
    const canQueryStats =
        !isApprovalQueue &&
        Boolean(profile) &&
        hasPermission(profile?.permissions, "work_item:list")

    const statsQuery = useQuery({
        queryKey: [
            ...workspaceHomeKeys.all,
            "queue-stats",
            scope,
            state.query,
            statsFilters,
            state.counterpartyIds,
            profile?.userid,
            profile?.policy_version,
            profile?.organization_version,
            profile?.role_ids,
            profile?.permissions,
        ],
        queryFn: () =>
            getWorkItemStats({
                scope: scope as "mine" | "managed" | "history",
                query: state.query,
                filters: statsFilters,
                counterpartyIds: state.counterpartyIds,
                timezone: "Asia/Shanghai",
            }),
        meta: { feedback },
        enabled: canQueryStats,
        staleTime: 60_000,
        refetchInterval: 60_000,
    })

    const counts = useMemo(
        () =>
            isApprovalQueue
                ? deriveApprovalCounts(
                      approvalDocumentTypeCounts,
                      selectedItemTypes,
                      total,
                  )
                : deriveWorkItemCounts(
                      statsQuery.data,
                      dueFilter,
                      selectedItemTypes,
                      total,
                  ),
        [
            approvalDocumentTypeCounts,
            dueFilter,
            isApprovalQueue,
            selectedItemTypes,
            statsQuery.data,
            total,
        ],
    )

    const presentation =
        !isApprovalQueue && statsQuery.isError
            ? getErrorPresentation(statsQuery.error)
            : undefined

    return {
        counts,
        failure: presentation && readProblemCopy(presentation, feedback),
        retry: presentation?.retryable
            ? () => {
                  void statsQuery.refetch()
              }
            : undefined,
    }
}
