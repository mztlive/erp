"use client"

import { useQueries } from "@tanstack/react-query"
import type { AccountProfile } from "@/features/auth/api"
import {
    listWorkItems,
    type WorkItemListParams,
} from "@/features/work-items/api"
import type { WorkItemDto } from "@/features/work-items/types"
import { queryKeyRoots } from "@/lib/query-key-roots"
import { overviewIdentity } from "./use-workspace-overview"

const URGENT_SOURCES = [
    { reason: "已超期", filter: { due: "overdue" } },
    { reason: "今日到期", filter: { due: "today" } },
    { reason: "紧急优先级", filter: { priorities: [1] } },
    { reason: "高优先级", filter: { priorities: [2] } },
] as const satisfies readonly {
    reason: string
    filter: Partial<WorkItemListParams>
}[]

export type UrgentWorkItem = {
    item: WorkItemDto
    reason: (typeof URGENT_SOURCES)[number]["reason"]
    rank: number
}

/** 从本人完整授权队列分别选取紧要候选，跨类型、跨岗位去重后最多展示五项。 */
export function useWorkbenchUrgent(profile: AccountProfile) {
    const identity = overviewIdentity(profile)
    const queries = useQueries({
        queries: URGENT_SOURCES.map((source) => ({
            queryKey: [
                ...queryKeyRoots.workspaceHome,
                "urgent-task-list",
                identity,
                source.filter,
            ],
            queryFn: () =>
                listWorkItems({
                    scope: "mine",
                    ...source.filter,
                    timezone: "Asia/Shanghai",
                    sort: "due_asc",
                    page: 1,
                    pageSize: 5,
                }),
            staleTime: 30_000,
            refetchInterval: 60_000,
            retry: false,
            meta: { roleWorkbench: true },
        })),
    })
    const candidates = new Map<string, UrgentWorkItem>()
    queries.forEach((query, rank) => {
        // 失败来源的缓存不冒充当前结果；缺失来源由界面明确提示。
        if (query.isError) return
        for (const item of query.data?.items ?? []) {
            if (!candidates.has(item.id))
                candidates.set(item.id, {
                    item,
                    reason: URGENT_SOURCES[rank]!.reason,
                    rank,
                })
        }
    })
    const items = [...candidates.values()]
        .sort((left, right) => {
            const category = left.rank - right.rank
            if (category) return category
            // 与服务端 due_asc 的空日期优先、截止时间、稳定 ID 顺序保持一致。
            const leftDue = left.item.due_at ?? -Infinity
            const rightDue = right.item.due_at ?? -Infinity
            if (leftDue !== rightDue) return leftDue < rightDue ? -1 : 1
            return left.item.id < right.item.id
                ? -1
                : left.item.id > right.item.id
                  ? 1
                  : 0
        })
        .slice(0, 5)
    return {
        items,
        isPending: queries.some((query) => query.isPending),
        isError: queries.some((query) => query.isError),
        isFetching: queries.some((query) => query.isFetching),
        refetch: () => Promise.all(queries.map((query) => query.refetch())),
    }
}
