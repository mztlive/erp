"use client"

import * as React from "react"
import { useRouter, useSearchParams } from "@/lib/navigation"

import { ConnectionList } from "@/features/supplier-api-connections/components/connection-list"
import {
    buildSupplierCatalogHref,
    buildConnectionsSearchParams,
    parseConnectionsSearchParams,
    type ConnectionsUrlState,
} from "@/features/supplier-api-connections/lib/url-state"

export function SupplierApiConnectionsPage() {
    const router = useRouter()
    const searchParams = useSearchParams()
    const urlState = React.useMemo(
        () => parseConnectionsSearchParams(searchParams),
        [searchParams],
    )
    const patchUrl = React.useCallback(
        (patch: Partial<ConnectionsUrlState>) => {
            const query = buildConnectionsSearchParams({
                ...urlState,
                ...patch,
            })
            router.replace(`/supplier-api/connections${query}`, {
                scroll: false,
            })
        },
        [router, urlState],
    )

    return (
        <ConnectionList
            urlState={urlState}
            patchUrl={patchUrl}
            onOpen={(id) => router.push(buildSupplierCatalogHref(id))}
        />
    )
}
