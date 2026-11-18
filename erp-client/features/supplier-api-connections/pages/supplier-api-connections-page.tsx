"use client"

import * as React from "react"
import { useAccountProfileQuery } from "@/features/auth/queries"
import { hasPermission } from "@/lib/permissions"
import { useRouter, useSearchParams } from "@/lib/navigation"

import { ConnectionList } from "@/features/supplier-api-connections/components/connection-list"
import {
    buildSupplierCatalogHref,
    buildSupplierConnectionSettingsHref,
    buildConnectionsSearchParams,
    parseConnectionsSearchParams,
    type ConnectionsUrlState,
} from "@/features/supplier-api-connections/lib/url-state"

export function SupplierApiConnectionsPage() {
    const router = useRouter()
    const searchParams = useSearchParams()
    const profile = useAccountProfileQuery()
    const canFilterSupplier = hasPermission(
        profile.data?.permissions,
        "supplier:list",
    )
    const hasUnauthorizedFilter =
        !canFilterSupplier && searchParams.has("supplierId")
    const urlState = React.useMemo(() => {
        const values = parseConnectionsSearchParams(searchParams)
        return hasUnauthorizedFilter
            ? { ...values, supplierId: undefined, page: 1 }
            : values
    }, [hasUnauthorizedFilter, searchParams])
    const patchUrl = React.useCallback(
        (patch: Partial<ConnectionsUrlState>) => {
            const query = buildConnectionsSearchParams({
                ...urlState,
                ...patch,
                ...(!canFilterSupplier ? { supplierId: undefined } : {}),
            })
            router.replace(`/supplier-api/connections${query}`, {
                scroll: false,
            })
        },
        [canFilterSupplier, router, urlState],
    )
    React.useEffect(() => {
        if (profile.data && hasUnauthorizedFilter) patchUrl({ page: 1 })
    }, [profile.data, hasUnauthorizedFilter, patchUrl])

    return (
        <ConnectionList
            urlState={urlState}
            patchUrl={patchUrl}
            onOpen={(id) =>
                router.push(buildSupplierConnectionSettingsHref(id))
            }
            onOpenCatalog={(id) => router.push(buildSupplierCatalogHref(id))}
        />
    )
}
