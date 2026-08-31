"use client"

import * as React from "react"
import { usePathname, useRouter, useSearchParams } from "next/navigation"

import type { IntegrationView } from "../../types"
import {
    buildIntegrationSearchParams,
    parseIntegrationSearchParams,
    toResolutionQuery,
} from "../../lib/url-state"

export function useIntegrationPageUrl({
    forcedTaskId,
    forcedDifferenceId,
}: {
    forcedTaskId?: string
    forcedDifferenceId?: string
}) {
    const router = useRouter()
    const pathname = usePathname()
    const searchParams = useSearchParams()

    const urlState = React.useMemo(
        () => parseIntegrationSearchParams(searchParams),
        [searchParams],
    )

    const currentTaskId = forcedTaskId ?? urlState.currentTaskId
    const currentDifferenceId =
        forcedDifferenceId ?? urlState.currentDifferenceId
    const focusMode = Boolean(forcedTaskId || forcedDifferenceId)

    const query = React.useMemo(
        () =>
            toResolutionQuery({
                ...urlState,
                currentTaskId,
                currentDifferenceId,
            }),
        [urlState, currentTaskId, currentDifferenceId],
    )

    const replaceUrl = React.useCallback(
        (patch: Record<string, string | null | undefined>) => {
            if (focusMode && (forcedTaskId || forcedDifferenceId)) {
                // detail routes keep path; still allow query prefs
                const params = new URLSearchParams(searchParams.toString())
                for (const [k, v] of Object.entries(patch)) {
                    if (
                        k === "taskId" ||
                        k === "differenceId" ||
                        k === "currentTaskId" ||
                        k === "currentDifferenceId"
                    )
                        continue
                    if (v == null || v === "") params.delete(k)
                    else params.set(k, v)
                }
                const qs = params.toString()
                router.replace(qs ? `${pathname}?${qs}` : pathname, {
                    scroll: false,
                })
                return
            }
            const base = parseIntegrationSearchParams(searchParams)
            const next = {
                ...base,
                view: (patch.view as IntegrationView | undefined) ?? base.view,
                mode: (patch.mode as typeof base.mode | undefined) ?? base.mode,
                environment:
                    (patch.environment as
                        | typeof base.environment
                        | undefined) ?? base.environment,
                errorClass:
                    patch.errorClass === null
                        ? undefined
                        : (patch.errorClass ?? base.errorClass),
                owner:
                    (patch.owner as typeof base.owner | undefined) ??
                    base.owner,
                q: patch.q === null ? undefined : (patch.q ?? base.q),
                queueContextId: patch.queueContextId ?? base.queueContextId,
                resolveWorkItemId:
                    patch.resolveWorkItemId === null
                        ? undefined
                        : (patch.resolveWorkItemId ?? base.resolveWorkItemId),
                currentTaskId:
                    patch.taskId === null
                        ? undefined
                        : (patch.taskId ?? base.currentTaskId),
                currentDifferenceId:
                    patch.differenceId === null
                        ? undefined
                        : (patch.differenceId ?? base.currentDifferenceId),
                autoNext:
                    patch.autoNext === "0"
                        ? false
                        : patch.autoNext === "1"
                          ? true
                          : base.autoNext,
            }
            const params = buildIntegrationSearchParams(next)
            // clear resolve after apply
            if (patch.resolveWorkItemId === null) {
                params.delete("resolveWorkItemId")
            }
            const qs = params.toString()
            router.replace(
                qs
                    ? `/governance/integration-errors?${qs}`
                    : "/governance/integration-errors",
                { scroll: false },
            )
        },
        [
            focusMode,
            forcedDifferenceId,
            forcedTaskId,
            pathname,
            router,
            searchParams,
        ],
    )

    const autoNext = urlState.autoNext

    const hasQueueFilters = Boolean(
        urlState.mode !== "all" ||
        urlState.environment !== "production" ||
        urlState.errorClass ||
        urlState.owner !== "me" ||
        urlState.q,
    )

    return {
        urlState,
        currentTaskId,
        currentDifferenceId,
        focusMode,
        query,
        replaceUrl,
        autoNext,
        hasQueueFilters,
    }
}
