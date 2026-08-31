"use client"

import * as React from "react"
import { useRouter, useSearchParams } from "next/navigation"

import type { IntegrationUrlState } from "../../lib/url-state"
import type {
    IntegrationQueueView,
    IntegrationResolutionItemView,
} from "../../types"
import { buildIntegrationSearchParams } from "../../lib/url-state"

export function useIntegrationPageSync({
    focusMode,
    view,
    queuePending,
    urlState,
    item,
    itemCount,
    autoNext,
}: {
    focusMode: boolean
    view: IntegrationQueueView | undefined
    queuePending: boolean
    urlState: IntegrationUrlState
    item: IntegrationResolutionItemView | undefined
    itemCount: number
    autoNext: boolean
}) {
    const router = useRouter()
    const searchParams = useSearchParams()

    // resolveWorkItemId → domain detail replace
    const resolvedEntry = view?.resolvedEntry
    React.useEffect(() => {
        if (!resolvedEntry) return
        if (!urlState.resolveWorkItemId) return
        const entry = resolvedEntry
        if (entry.itemType === "ERROR_TASK") {
            router.replace(
                `/governance/integration-errors/errors/${entry.id}?queueContextId=${encodeURIComponent(urlState.queueContextId)}&view=${urlState.view}&autoNext=${autoNext ? "1" : "0"}`,
            )
        } else {
            router.replace(
                `/governance/integration-errors/differences/${entry.id}?queueContextId=${encodeURIComponent(urlState.queueContextId)}&view=${urlState.view}&autoNext=${autoNext ? "1" : "0"}`,
            )
        }
    }, [
        resolvedEntry,
        urlState.resolveWorkItemId,
        urlState.queueContextId,
        urlState.view,
        autoNext,
        router,
    ])

    // URL defaults for current item
    React.useEffect(() => {
        if (queuePending || !view || focusMode) return
        if (urlState.resolveWorkItemId) return
        const hasTask = searchParams.has("taskId")
        const hasDiff = searchParams.has("differenceId")
        const hasView = searchParams.has("view")
        const hasCtx = searchParams.has("queueContextId")
        if (hasView && hasCtx && (hasTask || hasDiff || itemCount === 0)) return
        const params = buildIntegrationSearchParams({
            ...urlState,
            currentTaskId:
                item?.identity.itemType === "ERROR_TASK"
                    ? item.identity.id
                    : urlState.currentTaskId,
            currentDifferenceId:
                item?.identity.itemType === "RECONCILIATION_DIFFERENCE"
                    ? item.identity.id
                    : urlState.currentDifferenceId,
        })
        router.replace(`/governance/integration-errors?${params.toString()}`, {
            scroll: false,
        })
    }, [
        queuePending,
        view,
        focusMode,
        urlState,
        item,
        itemCount,
        searchParams,
        router,
    ])
}
