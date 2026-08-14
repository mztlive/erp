"use client"

import { BusinessEmptyState, BusinessFailureState, PageScaffold } from "@/components/business"
import { Button } from "@/components/ui/button"
import { JobDetailContent } from "@/features/history-backfill/components/job-detail-content"
import { useHistoryBackfillDetailQuery } from "@/features/history-backfill/hooks/queries"
import type { HistoryBackfillUrlState } from "@/features/history-backfill/lib/url-state"
import type { ItemResult } from "@/features/history-backfill/types"

function JobDetailView({
    jobId,
    urlState,
    patchUrl,
    onBack,
}: {
    jobId: string
    urlState: HistoryBackfillUrlState
    patchUrl: (patch: Partial<HistoryBackfillUrlState>) => void
    onBack: () => void
    onOpenJob: (id: string) => void
}) {
    const section = urlState.section
    const results: ItemResult[] | undefined =
        section === "dedupe"
            ? ["DEDUPLICATED"]
            : section === "unattributed"
              ? ["UNATTRIBUTED"]
              : section === "failures"
                ? ["FAILED"]
                : urlState.result
                  ? [urlState.result]
                  : undefined
    const factTypes = urlState.factType ? [urlState.factType] : undefined
    const costBases = urlState.costBasis ? [urlState.costBasis] : undefined

    const detailQuery = useHistoryBackfillDetailQuery({
        jobId,
        results,
        factTypes,
        costBases,
        q: urlState.q,
        page: Math.max(1, urlState.page),
        pageSize: 20,
        section,
    })

    const view = detailQuery.data

    if (detailQuery.isPending) {
        return (
            <PageScaffold>
                <div className="h-10 w-48 animate-pulse rounded-lg bg-muted" />
                <div className="h-24 animate-pulse rounded-lg bg-muted" />
                <div className="h-40 animate-pulse rounded-lg bg-muted" />
            </PageScaffold>
        )
    }

    if (detailQuery.isError) {
        return (
            <PageScaffold>
                <BusinessFailureState
                    title="任务加载失败"
                    error={detailQuery.error}
                    onRetry={() => void detailQuery.refetch()}
                />
            </PageScaffold>
        )
    }

    if (!view?.job) {
        return (
            <PageScaffold>
                <BusinessEmptyState
                    kind="no-data"
                    title="无法打开该任务"
                    description="任务可能已结束或链接失效；也可返回任务列表重新选择。"
                    className="rounded-lg border-0 bg-transparent shadow-none ring-0"
                    action={
                        <div className="flex flex-wrap gap-2">
                            <Button
                                type="button"
                                variant="secondary"
                                className="rounded-lg shadow-none"
                                onClick={() => void detailQuery.refetch()}
                            >
                                重试
                            </Button>
                            <Button
                                type="button"
                                variant="secondary"
                                className="rounded-lg shadow-none"
                                onClick={onBack}
                            >
                                返回列表
                            </Button>
                        </div>
                    }
                />
            </PageScaffold>
        )
    }

    return (
        <JobDetailContent
            view={view}
            isFetching={detailQuery.isFetching}
            urlState={urlState}
            patchUrl={patchUrl}
            onBack={onBack}
            onRefresh={() => void detailQuery.refetch()}
        />
    )
}

export { JobDetailView }
