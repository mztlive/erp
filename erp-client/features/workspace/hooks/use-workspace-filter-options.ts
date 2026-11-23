"use client"
import { useQuery } from "@tanstack/react-query"
import { useState } from "react"
import { listWorkItems } from "@/features/work-items/api"
import {
    listApprovalInstances,
    type ApprovalInstanceListView,
} from "@/features/approval-workflow/api"
import { useAccountProfileQuery } from "@/features/auth/queries"
import { useDebouncedSearch } from "@/features/entity-selectors/hooks/queries"
import {
    WORKSPACE_FILTER_LABELS,
    type WorkspaceFilters,
    type WorkspaceChoiceKey,
    type WorkspaceSearchOptions,
} from "@/lib/workspace-search"
import { getErrorPresentation } from "@/lib/api/errors"
import { readProblemCopy } from "@/lib/system-feedback"
import { taskOrderOptions } from "../api/filter-options"
import type { WorkspaceViewFilter } from "../types"

/** 候选搜索使用后端完整授权集；不对已加载选项做本地匹配。 */
export function useWorkspaceFilterOptions(
    view: WorkspaceViewFilter,
    filters?: WorkspaceFilters,
    name?: WorkspaceChoiceKey,
    approvalView: ApprovalInstanceListView = "started",
) {
    const [search, setSearch] = useState("")
    const queryText = useDebouncedSearch(search, 300)
    const profile = useAccountProfileQuery().data
    const orderDirectory =
        view !== "started" &&
        (name === "sales_orders" || name === "purchase_orders")
    const feedback = {
        subject: name
            ? `「${WORKSPACE_FILTER_LABELS[name]}」筛选选项`
            : "筛选选项",
        impact: "其他筛选条件仍可使用。",
    }
    const query = useQuery({
        queryKey: [
            "workspace-filter-options",
            view,
            view === "started" ? approvalView : undefined,
            name,
            queryText,
            filters,
            profile?.userid,
            profile?.role_ids,
            profile?.permissions,
        ],
        queryFn: async (): Promise<WorkspaceSearchOptions> => {
            if (orderDirectory && name) return taskOrderOptions(name, queryText)
            const options = {
                ...filters,
                include_options: true,
                option_query: queryText,
            }
            if (view === "started") {
                const page = await listApprovalInstances({
                    view: approvalView,
                    page: 1,
                    pageSize: 1,
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
        enabled: Boolean(profile) && (orderDirectory || Boolean(search.trim())),
        meta: { feedback },
    })
    const failure = query.isError
        ? getErrorPresentation(query.error)
        : undefined
    return {
        ...query,
        search,
        setSearch,
        failure: failure && readProblemCopy(failure, feedback),
        canRetry: failure?.retryable === true,
    }
}
