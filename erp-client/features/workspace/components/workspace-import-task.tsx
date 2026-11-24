"use client"

import { DetailWorkspaceMeta } from "@/components/business"
import { useWorkspaceInspectorMode } from "@/components/business/workspace-task-pane"
import { Badge } from "@/components/ui/badge"
import { useAccountProfileQuery } from "@/features/auth/queries"
import { BatchDetailView } from "@/features/import-opening/components/batch-detail-view"
import { useImportBatchDetailQuery } from "@/features/import-opening/hooks/queries"
import type { ImportOpeningUrlState } from "@/features/import-opening/lib/url-state"
import {
    CONFIRMATION_SCOPE_LABEL,
    type ConfirmationScope,
} from "@/features/import-opening/types"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { hasPermission } from "@/lib/permissions"

import { stripDocumentNumberPrefix } from "../lib/stable-number"
import type { WorkspaceWorkItem } from "../types"
import { WorkspaceProcessingPane } from "./workspace-processing-layout"
import { WorkspaceTaskIdentityHeader } from "./workspace-task-identity-header"

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

/** 工作台按批次、责任范围和任务办理导入确认。 */
export function WorkspaceImportTask({
    item,
    onTaskCompleted,
}: {
    item: WorkspaceWorkItem
    onTaskCompleted?: (workItemId: string) => void
}) {
    return (
        <WorkspaceImportTaskContent
            key={item.workItemId}
            item={item}
            onTaskCompleted={onTaskCompleted}
        />
    )
}

function WorkspaceImportTaskContent({
    item,
    onTaskCompleted,
}: {
    item: WorkspaceWorkItem
    onTaskCompleted?: (workItemId: string) => void
}) {
    const inspectorMode = useWorkspaceInspectorMode()
    const account = useAccountProfileQuery()
    const canDetail = hasPermission(
        account.data?.permissions,
        "legacy_import_batch:detail",
    )
    const forbidden = !account.isPending && !canDetail
    const context = {
        batchId: item.businessObjectId,
        workItemId: item.workItemId,
        confirmationScope: confirmationScopeOf(item),
        queueContextId: item.queueContextId,
    }
    const urlState: ImportOpeningUrlState = {
        ...context,
        environment: "PRODUCTION",
        section: "confirm",
        page: 1,
    }
    const idPrefix = `workspace-import-task-${toAutomationIdSegment(item.workItemId)}`
    const batch = useImportBatchDetailQuery(
        !account.isPending && canDetail ? context : undefined,
    ).data
    const scope = forbidden ? undefined : context.confirmationScope
    const headerTitle = forbidden ? "业务详情" : "导入业务确认"
    const rawBatchNo =
        batch?.batchNo || stripDocumentNumberPrefix(item.stableNumber)
    const batchNo =
        !forbidden && rawBatchNo && rawBatchNo !== headerTitle ? rawBatchNo : ""
    const headerItem = inspectorMode
        ? {
              ...item,
              allowedActions: item.allowedActions.filter(
                  (action) => action !== "REASSIGN" && action !== "CLOSE",
              ),
          }
        : item
    const deadline = item.dueAt ? item.dueAtLabel : undefined

    return (
        <WorkspaceProcessingPane
            archetype="action"
            className="@container/import-task"
            idPrefix={idPrefix}
            header={
                <WorkspaceTaskIdentityHeader
                    item={headerItem}
                    title={headerTitle}
                    compact
                    subtitle={
                        <DetailWorkspaceMeta
                            items={[
                                scope ? (
                                    <Badge key="scope" variant="info">
                                        {CONFIRMATION_SCOPE_LABEL[scope]}
                                    </Badge>
                                ) : null,
                                batchNo ? (
                                    <span
                                        key="batch-no"
                                        className="num select-all"
                                        data-workspace-document-number={batchNo}
                                    >
                                        {batchNo}
                                    </span>
                                ) : null,
                                deadline ? (
                                    <span key="deadline">
                                        截止{" "}
                                        <span className="num">{deadline}</span>
                                    </span>
                                ) : null,
                            ]}
                        />
                    }
                />
            }
            aria-label="当前导入业务确认任务"
        >
            <BatchDetailView
                batchId={item.businessObjectId}
                urlState={urlState}
                embedded
                idPrefix={idPrefix}
                onTaskCompleted={onTaskCompleted}
            />
        </WorkspaceProcessingPane>
    )
}
