"use client"

import { JobDetailView } from "@/features/history-backfill/pages/job-detail-view"
import { JobListView } from "@/features/history-backfill/pages/job-list-view"
import { useHistoryBackfillUrlState } from "@/features/history-backfill/hooks/use-history-backfill-url-state"

export function HistoryBackfillPage({
    routeJobId,
}: {
    /** 来自 `/governance/history-backfill/:jobId` */
    routeJobId?: string
}) {
    const { urlState, jobId, patchUrl, backToList, openJob } =
        useHistoryBackfillUrlState(routeJobId)

    if (jobId) {
        return (
            <JobDetailView
                jobId={jobId}
                urlState={urlState}
                patchUrl={patchUrl}
                onBack={backToList}
                onOpenJob={openJob}
            />
        )
    }

    return (
        <JobListView
            urlState={urlState}
            patchUrl={patchUrl}
            onOpenJob={openJob}
        />
    )
}
