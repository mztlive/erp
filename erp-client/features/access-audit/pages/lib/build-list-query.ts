import type { AccessListQuery, AccessView } from "@/features/access-audit/types"

export type AccessListQueryInput = {
    view: AccessView
    q: string
    subjectType?: string
    subjectId?: string
    from?: string
    to?: string
    actorId?: string
    action?: string
    objectId?: string
    result?: string
    traceId?: string
}

/** 由 URL 参数组装列表查询；详情参数（主体、事件）只驱动详情请求。 */
export function buildAccessListQuery(
    input: AccessListQueryInput,
): AccessListQuery {
    return {
        view: input.view,
        q: input.q || undefined,
        // 详情参数只驱动详情请求：数据范围收进主体详情后，
        // subjectId/subjectType 与 eventId 都不再进列表查询（打开详情不改动背后列表）。
        subjectType: undefined,
        subjectId: undefined,
        from: input.from,
        to: input.to,
        actorId: input.actorId,
        action: input.action,
        objectId: input.objectId,
        result: input.result,
        traceId: input.traceId,
        eventId: undefined,
    }
}
