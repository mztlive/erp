"use client"

import { hashKey, useQueries } from "@tanstack/react-query"
import type { AccountProfile } from "@/features/auth/api"
import type {
    WorkspaceFilters,
    WorkspaceSearchOption,
} from "@/lib/workspace-search"
import { fetchQueueCount } from "../api/queue-count"
import type { WorkspaceUrlState } from "../lib/url-state"
import { workspaceHomeKeys } from "./queries"

/** 类型数量忽略当前类型；期限数量保留当前类型，二者均保留关键词与其他业务条件。 */
export function useWorkspaceQueueCounts(
    state: WorkspaceUrlState,
    types: readonly WorkspaceSearchOption[],
    total: number,
    profile?: AccountProfile,
) {
    const filters = state.filters ?? {}
    const typeFilters = { ...filters, item_types: undefined }
    const dueFilters = { ...filters, due: undefined }
    const requests: { key: string; filters: WorkspaceFilters }[] = [
        { key: "types-all", filters: typeFilters },
        ...types.map((type) => ({
            key: type.value,
            filters: { ...typeFilters, item_types: [type.value] },
        })),
        { key: "due-all", filters: dueFilters },
        ...(state.view === "inbox" || state.view === "managed"
            ? (["overdue", "today"] as const).map((due) => ({
                  key: due,
                  filters: {
                      ...dueFilters,
                      due,
                      date_field: undefined,
                      date_from: undefined,
                      date_to: undefined,
                      undated: undefined,
                  },
              }))
            : []),
    ]
    // 相同条件只请求一次，当前列表的计数直接复用其总数。
    const currentKey = hashKey([filters])
    const unique = [
        ...new Map(
            requests
                .filter((request) => hashKey([request.filters]) !== currentKey)
                .map((request) => [hashKey([request.filters]), request]),
        ).values(),
    ]
    const results = useQueries({
        queries: unique.map((request) => ({
            queryKey: [
                ...workspaceHomeKeys.all,
                "queue-count",
                state.view,
                state.query,
                request.filters,
                profile?.userid,
                profile?.policy_version,
                profile?.organization_version,
                profile?.role_ids,
                profile?.permissions,
            ],
            queryFn: () =>
                fetchQueueCount(state.view, state.query, request.filters),
            enabled: Boolean(profile),
            staleTime: 60_000,
            refetchInterval: 60_000,
        })),
    })
    return {
        counts: Object.fromEntries(
            requests.map((request) => {
                const key = hashKey([request.filters])
                const result =
                    results[
                        unique.findIndex(
                            (entry) => hashKey([entry.filters]) === key,
                        )
                    ]
                return [
                    request.key,
                    key === currentKey
                        ? total
                        : result?.isError
                          ? undefined
                          : result?.data,
                ]
            }),
        ) as Record<string, number | undefined>,
        failed: results.some((result) => result.isError),
        retry: () => {
            for (const result of results) void result.refetch()
        },
    }
}
