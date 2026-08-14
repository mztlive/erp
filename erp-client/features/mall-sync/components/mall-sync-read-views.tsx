"use client"

import type { ColumnDef, PaginationState } from "@tanstack/react-table"

import { MallSyncHistoryView } from "@/features/mall-sync/components/read-history-view"
import { MallSyncJobsView } from "@/features/mall-sync/components/read-jobs-view"
import { MallSyncOverviewView } from "@/features/mall-sync/components/read-overview-view"
import { MallSyncReconciliationView } from "@/features/mall-sync/components/read-reconciliation-view"
import { MallSyncSnapshotsView } from "@/features/mall-sync/components/read-snapshots-view"
import type {
    MallSnapshotRow,
    MallSyncJobRow,
    MallSyncPageView,
    MallSyncViewName,
    ReconciliationDifference,
} from "@/features/mall-sync/types"

export type PatchUrl = (
    patch: Record<string, string | null | undefined>,
    options?: { replace?: boolean },
) => void

type MallSyncReadViewsProps = {
    view: MallSyncViewName
    context: MallSyncPageView["context"] | undefined
    ownership: MallSyncPageView["context"]["ownership"] | undefined
    data: MallSyncPageView | undefined
    pageJobs: MallSyncJobRow[]
    jobColumns: ColumnDef<MallSyncJobRow>[]
    snapshotColumns: ColumnDef<MallSnapshotRow>[]
    diffColumns: ColumnDef<ReconciliationDifference>[]
    pagination: PaginationState
    onPaginationChange: (
        next: PaginationState | ((current: PaginationState) => PaginationState),
    ) => void
    retryPending: boolean
    onRetryJob: () => void
    patchUrl: PatchUrl
    firstPhase: boolean
    sealed: boolean
    onPullDifference: (externalOrderNo: string) => void
}

function MallSyncReadViews({
    view,
    context,
    ownership,
    data,
    pageJobs,
    jobColumns,
    snapshotColumns,
    diffColumns,
    pagination,
    onPaginationChange,
    retryPending,
    onRetryJob,
    patchUrl,
    firstPhase,
    sealed,
    onPullDifference,
}: MallSyncReadViewsProps) {
    switch (view) {
        case "overview":
            return (
                <MallSyncOverviewView
                    context={context}
                    ownership={ownership}
                    jobs={data?.jobs ?? []}
                    patchUrl={patchUrl}
                />
            )
        case "jobs":
            return (
                <MallSyncJobsView
                    selectedJob={data?.selectedJob}
                    totalJobs={data?.jobs.length ?? 0}
                    pageJobs={pageJobs}
                    jobColumns={jobColumns}
                    pagination={pagination}
                    onPaginationChange={onPaginationChange}
                    retryPending={retryPending}
                    onRetryJob={onRetryJob}
                />
            )
        case "snapshots":
            return (
                <MallSyncSnapshotsView
                    data={data}
                    snapshotColumns={snapshotColumns}
                />
            )
        case "reconciliation":
            return (
                <MallSyncReconciliationView
                    data={data}
                    diffColumns={diffColumns}
                    firstPhase={firstPhase}
                    onPullDifference={onPullDifference}
                />
            )
        case "history":
            return <MallSyncHistoryView data={data} sealed={sealed} />
        default:
            return null
    }
}

export { MallSyncReadViews }
