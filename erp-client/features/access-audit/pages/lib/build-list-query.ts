import type {
    AccessListQuery,
    AccessView,
} from "@/features/access-audit/types"

export type AccessListQueryInput = {
    view: AccessView
    q: string
    status?: string
    org?: string
    risk?: string
    subjectType?: string
    subjectId?: string
    from?: string
    to?: string
    actorId?: string
    action?: string
    objectType?: string
    objectId?: string
    result?: string
    traceId?: string
}

/** 由 URL 参数组装列表查询；详情参数只驱动详情请求，eventId 不进列表查询。 */
export function buildAccessListQuery(
    input: AccessListQueryInput,
): AccessListQuery {
    return {
        view: input.view,
        q: input.q || undefined,
        status: input.status,
        org: input.org,
        risk: input.risk,
        // 详情参数只驱动详情请求：subjectId/subjectType 仅数据范围视图参与列表筛选，
        // eventId 不进列表查询（避免打开详情背后列表闪变）。
        subjectType: input.view === "scopes" ? input.subjectType : undefined,
        subjectId: input.view === "scopes" ? input.subjectId : undefined,
        from: input.from,
        to: input.to,
        actorId: input.actorId,
        action: input.action,
        objectType: input.objectType,
        objectId: input.objectId,
        result: input.result,
        traceId: input.traceId,
        eventId: undefined,
    }
}
