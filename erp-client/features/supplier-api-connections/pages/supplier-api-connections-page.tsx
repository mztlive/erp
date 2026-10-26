"use client"

import * as React from "react"
import { useRouter, useSearchParams } from "@/lib/navigation"

import { ConnectionList } from "@/features/supplier-api-connections/components/connection-list"
import {
    buildConnectionDetailHref,
    buildConnectionsSearchParams,
    parseConnectionDetailSearchParams,
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
    const legacyId = searchParams.get("connectionId") || searchParams.get("id")
    const legacyHref = legacyId
        ? buildConnectionDetailHref(
              legacyId,
              urlState,
              parseConnectionDetailSearchParams(searchParams).section,
          )
        : null

    React.useEffect(() => {
        if (legacyHref) router.replace(legacyHref, { scroll: false })
    }, [legacyHref, router])

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

    if (legacyHref) return null

    return (
        <ConnectionList
            urlState={urlState}
            patchUrl={patchUrl}
            onOpen={(id) =>
                router.push(buildConnectionDetailHref(id, urlState))
            }
        />
    )
}
