"use client"

import * as React from "react"

import { BatchDetailView } from "@/features/import-opening/components/batch-detail-view"
import { useImportBatchDetailUrlState } from "@/features/import-opening/hooks/use-import-opening-url"
import type { BatchSection } from "@/features/import-opening/types"
import { useRouter } from "@/lib/navigation"

export function ImportBatchDetailPage({ batchId }: { batchId: string }) {
    const router = useRouter()
    const { urlState, returnTo, replaceUrl, patchUrl } =
        useImportBatchDetailUrlState(batchId)

    const backHref = React.useMemo(() => {
        if (returnTo) return returnTo
        return urlState.environment === "PRODUCTION"
            ? "/governance/imports?environment=PRODUCTION"
            : "/governance/imports"
    }, [returnTo, urlState.environment])

    const openBatch = React.useCallback(
        (targetBatchId: string, nextSection?: BatchSection) => {
            const params = new URLSearchParams()
            if (urlState.environment === "PRODUCTION") {
                params.set("environment", "PRODUCTION")
            }
            if (nextSection && nextSection !== "overview") {
                params.set("section", nextSection)
            }
            const qs = params.toString()
            const base = `/governance/imports/${encodeURIComponent(targetBatchId)}`
            router.push(qs ? `${base}?${qs}` : base)
        },
        [router, urlState.environment],
    )

    return (
        <BatchDetailView
            batchId={batchId}
            urlState={urlState}
            patchUrl={patchUrl}
            replaceUrl={replaceUrl}
            backHref={backHref}
            backAlways={Boolean(returnTo)}
            onOpenBatch={openBatch}
        />
    )
}
