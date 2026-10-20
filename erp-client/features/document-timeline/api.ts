import { apiGet } from "@/lib/api"
import type {
    TaskTimeline,
    TimelineFilter,
    TimelinePage,
    TimelineResponse,
    TimelineTarget,
} from "./types"

export async function fetchTimeline(
    target: TimelineTarget,
    filter: TimelineFilter,
    cursor?: string,
): Promise<TimelineResponse> {
    const params = new URLSearchParams({
        page_size: String(filter.pageSize ?? 30),
        include_details: String(filter.includeDetails ?? false),
    })
    if (cursor) params.set("cursor", cursor)
    if (filter.category) params.set("category", filter.category)
    const id = encodeURIComponent(target.id)
    if (target.kind === "sales")
        return apiGet<TimelinePage>(
            `/admin/sales-orders/${id}/timeline?${params}`,
        )
    const path =
        target.kind === "approval" ? "approval-instances" : "work-items"
    const result = await apiGet<TaskTimeline>(
        `/admin/${path}/${id}/timeline?${params}`,
    )
    return {
        ...result.timeline,
        salesOrders: result.sales_orders,
        submittedAt: result.submitted_at,
    }
}
