"use client"

import * as React from "react"
import { usePathname, useRouter, useSearchParams } from "next/navigation"

import { BatchDetailView } from "@/features/import-opening/components/batch-detail-view"
import { BatchListView } from "@/features/import-opening/components/batch-list-view"
import {
    buildImportOpeningSearchParams,
    parseImportOpeningSearchParams,
    type ImportOpeningUrlState,
} from "@/features/import-opening/lib/url-state"

export function ImportOpeningPage() {
    const router = useRouter()
    const pathname = usePathname()
    const searchParams = useSearchParams()

    const urlState = React.useMemo(
        () => parseImportOpeningSearchParams(searchParams),
        [searchParams],
    )

    const replaceUrl = React.useCallback(
        (next: ImportOpeningUrlState) => {
            const qs = buildImportOpeningSearchParams(next)
            router.replace(`${pathname}${qs}`, { scroll: false })
        },
        [pathname, router],
    )

    const patchUrl = React.useCallback(
        (patch: Partial<ImportOpeningUrlState>) => {
            replaceUrl({ ...urlState, ...patch })
        },
        [replaceUrl, urlState],
    )

    if (urlState.batchId) {
        return (
            <BatchDetailView
                batchId={urlState.batchId}
                urlState={urlState}
                patchUrl={patchUrl}
                replaceUrl={replaceUrl}
            />
        )
    }

    return <BatchListView urlState={urlState} patchUrl={patchUrl} />
}
