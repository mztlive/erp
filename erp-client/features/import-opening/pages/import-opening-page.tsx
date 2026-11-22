"use client"

import * as React from "react"

import { DetailWorkspaceSkeleton } from "@/components/business"
import { BatchListView } from "@/features/import-opening/components/batch-list-view"
import { useImportOpeningUrlState } from "@/features/import-opening/hooks/use-import-opening-url"
import { useRouter, useSearchParams } from "@/lib/navigation"

export function ImportOpeningPage() {
    const router = useRouter()
    const searchParams = useSearchParams()
    const { urlState, patchUrl } = useImportOpeningUrlState()

    const legacyBatchId =
        searchParams.get("batchId")?.trim() ||
        searchParams.get("id")?.trim() ||
        undefined

    const redirectHref = React.useMemo(() => {
        if (!legacyBatchId) return null
        const params = new URLSearchParams(searchParams.toString())
        params.delete("batchId")
        params.delete("id")
        const qs = params.toString()
        const base = `/governance/imports/${encodeURIComponent(legacyBatchId)}`
        return qs ? `${base}?${qs}` : base
    }, [legacyBatchId, searchParams])

    React.useEffect(() => {
        if (redirectHref) {
            router.replace(redirectHref)
        }
    }, [redirectHref, router])

    if (redirectHref) {
        return <DetailWorkspaceSkeleton title="导入批次详情" summary />
    }

    return <BatchListView urlState={urlState} patchUrl={patchUrl} />
}
