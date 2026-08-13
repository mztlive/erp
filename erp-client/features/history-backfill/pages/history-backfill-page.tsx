"use client"

import * as React from "react"
import { usePathname, useRouter, useSearchParams } from "next/navigation"

import { JobDetailView } from "@/features/history-backfill/pages/job-detail-view"
import { JobListView } from "@/features/history-backfill/pages/job-list-view"
import {
    buildHistoryBackfillSearchParams,
    parseHistoryBackfillSearchParams,
    type HistoryBackfillUrlState,
} from "@/features/history-backfill/lib/url-state"

export function HistoryBackfillPage({
    routeJobId,
}: {
    /** 来自 `/governance/history-backfill/:jobId` */
    routeJobId?: string
}) {
    const router = useRouter()
    const pathname = usePathname()
    const searchParams = useSearchParams()

    const urlState = React.useMemo(
        () => parseHistoryBackfillSearchParams(searchParams),
        [searchParams],
    )

    const jobId = routeJobId ?? urlState.jobId

    const listPath = "/governance/history-backfill"

    const replaceListUrl = React.useCallback(
        (next: HistoryBackfillUrlState) => {
            const qs = buildHistoryBackfillSearchParams(
                { ...next, jobId: undefined },
                { omitJobId: true },
            )
            router.replace(`${listPath}${qs}`, { scroll: false })
        },
        [router],
    )

    const replaceDetailUrl = React.useCallback(
        (id: string, next: HistoryBackfillUrlState) => {
            const qs = buildHistoryBackfillSearchParams(
                { ...next, jobId: undefined },
                { omitJobId: true },
            )
            router.replace(`${listPath}/${id}${qs}`, { scroll: false })
        },
        [router],
    )

    const patchUrl = React.useCallback(
        (patch: Partial<HistoryBackfillUrlState>) => {
            const next = { ...urlState, ...patch }
            if (jobId) replaceDetailUrl(jobId, next)
            else replaceListUrl(next)
        },
        [urlState, jobId, replaceDetailUrl, replaceListUrl],
    )

    if (jobId) {
        return (
            <JobDetailView
                jobId={jobId}
                urlState={urlState}
                patchUrl={patchUrl}
                onBack={() => {
                    replaceListUrl({
                        ...urlState,
                        jobId: undefined,
                        section: "overview",
                    })
                }}
                onOpenJob={(id) =>
                    replaceDetailUrl(id, {
                        ...urlState,
                        section: "overview",
                        page: 1,
                        result: undefined,
                        factType: undefined,
                        costBasis: undefined,
                        q: undefined,
                    })
                }
            />
        )
    }

    return (
        <JobListView
            urlState={urlState}
            patchUrl={patchUrl}
            onOpenJob={(id) =>
                replaceDetailUrl(id, {
                    ...urlState,
                    section: "overview",
                    page: 1,
                    result: undefined,
                    factType: undefined,
                    costBasis: undefined,
                    q: undefined,
                })
            }
            pathname={pathname}
        />
    )
}
