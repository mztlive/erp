"use client"
import { useQuery } from "@tanstack/react-query"
import { useState } from "react"
import { listWorkItems } from "@/features/work-items/api"
import { listApprovalInstances } from "@/features/approval-workflow/api"
import { useAccountProfileQuery } from "@/features/auth/queries"
import { useDebouncedSearch } from "@/features/entity-selectors/hooks/queries"
import type {
    WorkspaceFilters,
    WorkspaceSearchOptions,
} from "@/lib/workspace-search"
import type { WorkspaceViewFilter } from "../types"

/** 候选搜索使用后端完整授权集；不对已加载选项做本地匹配。 */
export function useWorkspaceFilterOptions(
    view: WorkspaceViewFilter,
    filters?: WorkspaceFilters,
) {
    const [search, setSearch] = useState("")
    const queryText = useDebouncedSearch(search, 300)
    const profile = useAccountProfileQuery().data
    const query = useQuery({
        queryKey: [
            "workspace-filter-options",
            view,
            queryText,
            filters,
            profile?.userid,
            profile?.role_ids,
            profile?.permissions,
        ],
        queryFn: async (): Promise<WorkspaceSearchOptions> => {
            const options = {
                ...filters,
                include_options: true,
                option_query: queryText,
            }
            if (view === "started") {
                const page = await listApprovalInstances({
                    view: "started",
                    page: 1,
                    limit: 1,
                    filters: options,
                })
                return page.filterOptions ?? {}
            }
            const page = await listWorkItems({
                scope: view === "inbox" ? "mine" : view,
                filters: options,
                page: 1,
                pageSize: 1,
                timezone: "Asia/Shanghai",
            })
            return page.filter_options ?? {}
        },
        enabled: Boolean(profile) && Boolean(search.trim()),
    })
    return { ...query, search, setSearch }
}
