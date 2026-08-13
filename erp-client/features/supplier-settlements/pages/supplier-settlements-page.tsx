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
        return (
            <SettlementCenter
                statementId={urlState.statementId}
                urlState={urlState}
                patchUrl={patchUrl}
                returnTo={urlState.returnTo}
                onBack={() =>
                    patchUrl({
                        statementId: undefined,
                        section: "overview",
                        preview: undefined,
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
                })
            }
        />
    )
}
