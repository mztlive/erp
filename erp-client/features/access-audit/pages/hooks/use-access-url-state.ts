"use client"

import { usePathname, useRouter, useSearchParams } from "next/navigation"

import { patchUrl as patchSearchParams } from "@/lib/patch-search-params"
import { parseView } from "@/features/access-audit/lib/url-state"
import type { AccessView } from "@/features/access-audit/types"

/**
 * 权限与审计页面的 URL 查询参数解析与修补。
 * 所有参数与界面控件一一对应（见 AGENTS.md 界面契约）。
 */
function useAccessUrlState() {
    const router = useRouter()
    const pathname = usePathname()
    const searchParams = useSearchParams()

    const view: AccessView = parseView(searchParams.get("view"))
    const qParam = searchParams.get("q") ?? ""
    const status = searchParams.get("status") ?? undefined
    const org = searchParams.get("org") ?? undefined
    const risk = searchParams.get("risk") ?? undefined
    const subjectTypeParam = searchParams.get("subjectType") ?? undefined
    const subjectIdParam = searchParams.get("subjectId") ?? undefined
    const eventIdParam = searchParams.get("eventId") ?? undefined
    const fromParam = searchParams.get("from") ?? undefined
    const toParam = searchParams.get("to") ?? undefined
    const actorId = searchParams.get("actorId") ?? undefined
    const action = searchParams.get("action") ?? undefined
    const objectType = searchParams.get("objectType") ?? undefined
    const objectId = searchParams.get("objectId") ?? undefined
    const resultFilter = searchParams.get("result") ?? undefined
    const traceId = searchParams.get("traceId") ?? undefined
    const rejectedWorkItemId = searchParams.get("workItemId") ?? undefined

    function patchUrl(
        patch: Record<string, string | null | undefined>,
        options?: { replace?: boolean },
    ) {
        patchSearchParams(
            { router, pathname, searchParams, view },
            patch,
            options,
        )
    }

    return {
        router,
        pathname,
        searchParams,
        view,
        qParam,
        status,
        org,
        risk,
        subjectTypeParam,
        subjectIdParam,
        eventIdParam,
        fromParam,
        toParam,
        actorId,
        action,
        objectType,
        objectId,
        resultFilter,
        traceId,
        rejectedWorkItemId,
        patchUrl,
    }
}

export { useAccessUrlState }
