"use client"

import * as React from "react"

import { usePathname, useRouter, useSearchParams } from "@/lib/navigation"
import { buildWorkspaceDetailHref } from "@/lib/workspace-detail-href"
import { isWorkspaceFrame, workspaceHost } from "@/lib/workspace-tabs"

import { SettlementCenter } from "@/features/supplier-settlements/components/settlement-center"
import {
    parseSettlementsSearchParams,
    type SettlementsUrlState,
} from "@/features/supplier-settlements/lib/url-state"

const LIST_HREF = "/supplier-api/settlements"

function safeDecodeSegment(raw: string): string {
    try {
        return decodeURIComponent(raw)
    } catch {
        return raw
    }
}

function safeInternalReturnTo(raw?: string): string | undefined {
    const value = raw?.trim()
    if (!value || !value.startsWith("/") || value.startsWith("//")) {
        return undefined
    }
    return value
}

/** 独立结算单详情页：承载 /supplier-api/settlements/[statementId]，不与列表页混用路由分支。 */
export function SupplierSettlementDetailPage({
    statementId: rawStatementId,
}: {
    statementId: string
}) {
    const router = useRouter()
    const pathname = usePathname()
    const searchParams = useSearchParams()

    const statementId = React.useMemo(
        () => safeDecodeSegment(rawStatementId),
        [rawStatementId],
    )

    const urlState = React.useMemo<SettlementsUrlState>(() => {
        const parsed = parseSettlementsSearchParams(searchParams)
        return {
            ...parsed,
            statementId,
        }
    }, [searchParams, statementId])

    const patchUrl = React.useCallback(
        (patch: Partial<SettlementsUrlState>) => {
            const next: SettlementsUrlState = {
                ...urlState,
                ...patch,
                statementId,
            }
            const base = `/supplier-api/settlements/${encodeURIComponent(statementId)}`
            const params = new URLSearchParams()
            if (next.section !== "overview") params.set("section", next.section)
            if (next.diff) params.set("diff", next.diff)
            if (next.returnTo) params.set("returnTo", next.returnTo)
            if (next.workItemId) params.set("workItemId", next.workItemId)
            if (next.queueContextId) {
                params.set("queueContextId", next.queueContextId)
            }
            if (next.from) params.set("from", next.from)
            const qs = params.toString()
            const target = qs ? `${base}?${qs}` : base
            const currentQs = searchParams.toString()
            const current = currentQs ? `${pathname}?${currentQs}` : pathname
            if (target !== current) {
                router.replace(target, { scroll: false })
            }
        },
        [pathname, router, searchParams, statementId, urlState],
    )

    const taskReturnTo = urlState.workItemId
        ? buildWorkspaceDetailHref("tasks", urlState.workItemId)
        : undefined
    const resolvedReturnTo = safeInternalReturnTo(
        urlState.returnTo ?? taskReturnTo,
    )

    const handleBack = React.useCallback(() => {
        if (resolvedReturnTo) {
            router.push(resolvedReturnTo)
            return
        }
        const host = workspaceHost()
        if (host) {
            host.closeCurrent(isWorkspaceFrame() ? window : null, LIST_HREF)
        } else {
            router.push(LIST_HREF)
        }
    }, [resolvedReturnTo, router])

    return (
        <SettlementCenter
            statementId={statementId}
            workItemId={urlState.workItemId}
            urlState={urlState}
            patchUrl={patchUrl}
            returnTo={resolvedReturnTo}
            onBack={handleBack}
        />
    )
}
