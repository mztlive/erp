"use client"

import * as React from "react"
import { usePathname, useRouter, useSearchParams } from "next/navigation"

import { SettlementCenter } from "@/features/supplier-settlements/components/settlement-center"
import { SettlementList } from "@/features/supplier-settlements/components/settlement-list"
import {
    buildSettlementsSearchParams,
    parseSettlementsSearchParams,
    type SettlementsUrlState,
} from "@/features/supplier-settlements/lib/url-state"

export function SupplierSettlementsPage() {
    const router = useRouter()
    const pathname = usePathname()
    const searchParams = useSearchParams()

    const pathMatch = pathname.match(/\/supplier-api\/settlements\/([^/]+)$/)
    const pathStatementId = pathMatch?.[1]

    const urlState = React.useMemo(() => {
        const parsed = parseSettlementsSearchParams(searchParams)
        if (pathStatementId && !parsed.statementId) {
            return { ...parsed, statementId: pathStatementId }
        }
        return parsed
    }, [searchParams, pathStatementId])

    const replaceUrl = React.useCallback(
        (next: SettlementsUrlState) => {
            if (pathStatementId && next.statementId === pathStatementId) {
                const base = `/supplier-api/settlements/${pathStatementId}`
                const params = new URLSearchParams()
                if (next.section !== "overview")
                    params.set("section", next.section)
                if (next.returnTo) params.set("returnTo", next.returnTo)
                if (next.workItemId) params.set("workItemId", next.workItemId)
                if (next.queueContextId)
                    params.set("queueContextId", next.queueContextId)
                if (next.from) params.set("from", next.from)
                const qs = params.toString()
                router.replace(qs ? `${base}?${qs}` : base, { scroll: false })
                return
            }
            const listPath = "/supplier-api/settlements"
            const qs = buildSettlementsSearchParams(next)
            router.replace(`${listPath}${qs}`, { scroll: false })
        },
        [pathStatementId, router],
    )

    const patchUrl = React.useCallback(
        (patch: Partial<SettlementsUrlState>) => {
            replaceUrl({ ...urlState, ...patch })
        },
        [replaceUrl, urlState],
    )

    if (urlState.statementId) {
        const taskReturnTo =
            urlState.from === "W02" &&
            urlState.workItemId &&
            urlState.queueContextId
                ? `/workspace/tasks?queueContextId=${encodeURIComponent(urlState.queueContextId)}&currentWorkItemId=${encodeURIComponent(urlState.workItemId)}`
                : undefined
        return (
            <SettlementCenter
                statementId={urlState.statementId}
                workItemId={urlState.workItemId}
                urlState={urlState}
                patchUrl={patchUrl}
                returnTo={urlState.returnTo ?? taskReturnTo}
                onBack={() =>
                    patchUrl({
                        statementId: undefined,
                        section: "overview",
                        preview: undefined,
                        workItemId: undefined,
                        queueContextId: undefined,
                        from: undefined,
                    })
                }
            />
        )
    }

    return (
        <SettlementList
            urlState={urlState}
            patchUrl={patchUrl}
            returnTo={urlState.returnTo}
            onOpen={(id) =>
                patchUrl({
                    statementId: id,
                    section: "overview",
                    preview: undefined,
                    workItemId: undefined,
                    queueContextId: undefined,
                    from: undefined,
                })
            }
        />
    )
}
