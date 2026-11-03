import { listApprovalInstances } from "@/features/approval-workflow/api"
import { listWorkItems } from "@/features/work-items/api"
import type { WorkspaceFilters } from "@/lib/workspace-search"
import type { WorkspaceViewFilter } from "../types"

/** 复用服务端查询总数；不下载全量事项，也不从当前页推算分类数量。 */
export async function fetchQueueCount(
    view: WorkspaceViewFilter,
    query: string | undefined,
    filters: WorkspaceFilters,
) {
    const countFilters = { ...filters, include_options: false }
    if (view === "started") {
        const page = await listApprovalInstances({
            view: "started",
            query,
            filters: countFilters,
            page: 1,
            limit: 1,
        })
        return page.total
    }
    const page = await listWorkItems({
        scope: view === "inbox" ? "mine" : view,
        query,
        filters: countFilters,
        page: 1,
        pageSize: 1,
        timezone: "Asia/Shanghai",
    })
    return page.total
}
