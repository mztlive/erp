"use client"

import * as React from "react"
import { usePathname, useRouter, useSearchParams } from "next/navigation"

import {
    buildHistoryBackfillSearchParams,
    parseHistoryBackfillSearchParams,
    type HistoryBackfillUrlState,
} from "@/features/history-backfill/lib/url-state"

/**
 * 历史回填页 URL 状态：解析当前查询参数，并把 patch 以 router.replace
 * 写回列表 / 任务详情地址（不产生历史记录）。
 */
export function useHistoryBackfillUrlState(routeJobId?: string) {
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

    const backToList = React.useCallback(() => {
        replaceListUrl({
            ...urlState,
            jobId: undefined,
            section: "overview",
        })
    }, [replaceListUrl, urlState])

    const openJob = React.useCallback(
        (id: string) =>
            replaceDetailUrl(id, {
                ...urlState,
                section: "overview",
                page: 1,
                result: undefined,
                factType: undefined,
                costBasis: undefined,
                q: undefined,
            }),
        [replaceDetailUrl, urlState],
    )

    return { urlState, jobId, pathname, patchUrl, backToList, openJob }
}
