"use client"

import { useParams, useRouter, useSearchParams } from "@/lib/navigation"

import { ConnectionCenter } from "@/features/supplier-api-connections/components/connection-center"
import {
    buildConnectionDetailHref,
    buildConnectionsSearchParams,
    parseConnectionDetailSearchParams,
    parseConnectionsSearchParams,
} from "@/features/supplier-api-connections/lib/url-state"

export function SupplierApiConnectionDetailPage() {
    const { connectionId } = useParams<{ connectionId: string }>()
    const router = useRouter()
    const searchParams = useSearchParams()
    const listState = parseConnectionsSearchParams(searchParams)
    const { section } = parseConnectionDetailSearchParams(searchParams)

    return (
        <ConnectionCenter
            key={connectionId}
            connectionId={connectionId}
            section={section}
            onSectionChange={(next) =>
                router.replace(
                    buildConnectionDetailHref(connectionId, listState, next),
                    {
                        scroll: false,
                    },
                )
            }
            onBack={() =>
                router.push(
                    `/supplier-api/connections${buildConnectionsSearchParams(listState)}`,
                )
            }
        />
    )
}
