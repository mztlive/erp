"use client"

import * as React from "react"
import { usePathname, useRouter, useSearchParams } from "next/navigation"

import { ConnectionCenter } from "@/features/supplier-api-connections/components/connection-center"
import { ConnectionList } from "@/features/supplier-api-connections/components/connection-list"
import {
    buildConnectionsSearchParams,
    parseConnectionsSearchParams,
    type ConnectionsUrlState,
} from "@/features/supplier-api-connections/lib/url-state"

export function SupplierApiConnectionsPage() {
    const router = useRouter()
    const pathname = usePathname()
    const searchParams = useSearchParams()

    // Support path-based /connections/:id
    const pathMatch = pathname.match(/\/supplier-api\/connections\/([^/]+)$/)
    const pathConnectionId = pathMatch?.[1]

    const urlState = React.useMemo(() => {
        const parsed = parseConnectionsSearchParams(searchParams)
        if (pathConnectionId && !parsed.connectionId) {
            return { ...parsed, connectionId: pathConnectionId }
        }
        return parsed
    }, [searchParams, pathConnectionId])

    const replaceUrl = React.useCallback(
        (next: ConnectionsUrlState) => {
            // Prefer query-param center on list route for SPA tab identity;
            // path route keeps path when already on [connectionId].
            if (pathConnectionId && next.connectionId === pathConnectionId) {
                const base = `/supplier-api/connections/${pathConnectionId}`
                const params = new URLSearchParams()
                if (next.section !== "overview")
                    params.set("section", next.section)
                const qs = params.toString()
                router.replace(qs ? `${base}?${qs}` : base, { scroll: false })
                return
            }
            const listPath = "/supplier-api/connections"
            const qs = buildConnectionsSearchParams(next)
            router.replace(`${listPath}${qs}`, { scroll: false })
        },
        [pathConnectionId, router],
    )

    const patchUrl = React.useCallback(
        (patch: Partial<ConnectionsUrlState>) => {
            replaceUrl({ ...urlState, ...patch })
        },
        [replaceUrl, urlState],
    )

    if (urlState.connectionId) {
        return (
            <ConnectionCenter
                connectionId={urlState.connectionId}
                urlState={urlState}
                patchUrl={patchUrl}
                onBack={() =>
                    patchUrl({ connectionId: undefined, section: "overview" })
                }
            />
        )
    }

    return (
        <ConnectionList
            urlState={urlState}
            patchUrl={patchUrl}
            onOpen={(id) => patchUrl({ connectionId: id, section: "overview" })}
        />
    )
}
