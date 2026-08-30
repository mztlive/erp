"use client"

import * as React from "react"

import { BatchDetailView } from "@/features/import-opening/components/batch-detail-view"
import type { ImportOpeningUrlState } from "@/features/import-opening/lib/url-state"
import type { ConfirmationScope } from "@/features/import-opening/types"

import type { WorkspaceWorkItem } from "../types"

const CONFIRMATION_SCOPES = new Set<ConfirmationScope>([
    "SALES",
    "PROCUREMENT",
    "OPERATIONS",
    "WAREHOUSE",
    "FINANCE",
])

function confirmationScopeOf(item: WorkspaceWorkItem) {
    const scope = item.routeContext?.confirmationScope
    return scope && CONFIRMATION_SCOPES.has(scope as ConfirmationScope)
        ? (scope as ConfirmationScope)
        : undefined
}

/** W01 导入责任确认：锁定批次、范围与任务，仅在工作台内提交正式确认。 */
export function WorkspaceImportTask({
    item,
    onTaskCompleted,
}: {
    item: WorkspaceWorkItem
    onTaskCompleted?: (workItemId: string) => void
}) {
    const [urlState, setUrlState] = React.useState<ImportOpeningUrlState>({
        environment: "PRODUCTION",
        batchId: item.businessObjectId,
        workItemId: item.workItemId,
        confirmationScope: confirmationScopeOf(item),
        queueContextId: item.queueContextId,
        section: "confirm",
        page: 1,
    })
    const patchUrl = React.useCallback(
        (patch: Partial<ImportOpeningUrlState>) =>
            setUrlState((current) => ({ ...current, ...patch })),
        [],
    )
    const replaceUrl = React.useCallback(
        (next: ImportOpeningUrlState) => setUrlState(next),
        [],
    )

    return (
        <section
            className="h-full min-h-0 overflow-auto"
            aria-label="当前导入业务确认任务"
        >
            <BatchDetailView
                batchId={item.businessObjectId}
                urlState={urlState}
                patchUrl={patchUrl}
                replaceUrl={replaceUrl}
                embedded
                onTaskCompleted={onTaskCompleted}
            />
        </section>
    )
}
