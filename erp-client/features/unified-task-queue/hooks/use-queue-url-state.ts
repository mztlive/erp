"use client"

import * as React from "react"
import { useRouter, useSearchParams } from "next/navigation"

import {
    buildW02SearchParams,
    parseDue,
    parseFamily,
    parsePriorities,
    parseScopeSlug,
    parseSort,
    readW02FocusId,
    writeW02FocusId,
} from "../lib/queue-url"
import type { QueueScopeSlug, WorkItemFamily } from "../types"

export type QueueUrlOverrides = Readonly<{
    scope?: QueueScopeSlug
    family?: WorkItemFamily | null
    due?: "today" | "overdue" | null
    priorities?: readonly number[] | null
    sort?: "priority_due" | "due_asc" | "created_desc"
    historyStatus?: "COMPLETED" | "CLOSED"
    query?: string | null
    currentWorkItemId?: string | null
    approvalBlockers?: boolean
}>

/** 队列页唯一 URL 状态入口：读取筛选参数并生成下一次导航地址。 */
export function useQueueUrlState() {
    const router = useRouter()
    const searchParams = useSearchParams()

    const approvalBlockers = searchParams.get("view") === "approval-blockers"
    const scope = parseScopeSlug(searchParams.get("scope"))
    const family = parseFamily(searchParams.get("family"))
    const due = parseDue(searchParams.get("due"))
    const priorities = parsePriorities(searchParams.get("priority"))
    const sort = parseSort(searchParams.get("sort"))
    const historyStatus: "COMPLETED" | "CLOSED" | undefined =
        scope === "history" && searchParams.get("status") === "closed"
            ? "CLOSED"
            : scope === "history"
              ? "COMPLETED"
              : undefined
    const workItemType = searchParams.get("type") ?? undefined
    const queryText = searchParams.get("q") ?? ""
    const queueContextId = searchParams.get("queueContextId") ?? undefined
    const currentWorkItemId =
        searchParams.get("currentWorkItemId") ?? readW02FocusId() ?? undefined

    const replaceUrl = React.useCallback(
        (overrides: QueueUrlOverrides) => {
            const nextId =
                overrides.currentWorkItemId === undefined
                    ? currentWorkItemId
                    : overrides.currentWorkItemId
            writeW02FocusId(nextId ?? null)
            router.replace(
                `/workspace/tasks${buildW02SearchParams({
                    scope: overrides.scope ?? scope,
                    family:
                        overrides.family === null
                            ? null
                            : (overrides.family ?? family),
                    workItemType,
                    due: overrides.due === null ? null : (overrides.due ?? due),
                    priorities:
                        overrides.priorities === null
                            ? null
                            : (overrides.priorities ?? priorities),
                    sort: overrides.sort ?? sort,
                    historyStatus: overrides.historyStatus ?? historyStatus,
                    q:
                        overrides.query === null
                            ? null
                            : (overrides.query ?? queryText),
                    queueContextId,
                    currentWorkItemId: nextId,
                    approvalBlockers: overrides.approvalBlockers,
                })}`,
                { scroll: false },
            )
        },
        [
            currentWorkItemId,
            due,
            family,
            historyStatus,
            priorities,
            queryText,
            queueContextId,
            router,
            scope,
            sort,
            workItemType,
        ],
    )

    return {
        approvalBlockers,
        scope,
        family,
        due,
        priorities,
        sort,
        historyStatus,
        workItemType,
        queryText,
        queueContextId,
        currentWorkItemId,
        replaceUrl,
    }
}
