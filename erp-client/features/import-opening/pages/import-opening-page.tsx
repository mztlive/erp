"use client"

import { BatchDetailView } from "@/features/import-opening/components/batch-detail-view"
import { BatchListView } from "@/features/import-opening/components/batch-list-view"
import { useImportOpeningUrlState } from "@/features/import-opening/hooks/use-import-opening-url"

export function ImportOpeningPage() {
    const { urlState, replaceUrl, patchUrl } = useImportOpeningUrlState()

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
