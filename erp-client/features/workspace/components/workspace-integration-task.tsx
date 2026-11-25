"use client"

import {
    WorkspaceProcessingPane,
    WorkspaceCompactTaskHeader,
} from "@/components/business"
import { useWorkspaceInspectorMode } from "@/components/business/workspace-task-pane"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { IntegrationErrorsPage } from "@/features/integration-errors/pages/integration-errors-page"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { displayName } from "@/lib/display-name"
import { dispatchWorkspaceHotkeyAction } from "@/lib/workspace-hotkeys"

import { useWorkspaceHotkeys } from "@/hooks/use-workspace-hotkeys"
import { stripDocumentNumberPrefix } from "../lib/stable-number"
import type { WorkspaceWorkItem } from "../types"
import { WorkspaceTaskIdentityHeader } from "./workspace-task-identity-header"

export function WorkspaceIntegrationTask({
    item,
    onTaskCompleted,
}: {
    item: WorkspaceWorkItem
    onTaskCompleted?: (workItemId: string) => void
}) {
    const inspectorMode = useWorkspaceInspectorMode()
    const idPrefix = `workspace-investigation-${toAutomationIdSegment(item.workItemId)}`
    const errorTask = item.businessObjectType === "integration_error_task"
    const difference = item.businessObjectType === "reconciliation_difference"

    useWorkspaceHotkeys({
        enabled: inspectorMode && (errorTask || difference),
        enableCheatSheet: false,
        onToggleRail: () => dispatchWorkspaceHotkeyAction("toggle-rail"),
        onHistory: () => dispatchWorkspaceHotkeyAction("history"),
    })
    const title = difference
        ? "对账差异"
        : item.workItemType === "INTEGRATION_RESULT_UNKNOWN"
          ? "外部同步待核实"
          : "接口差错"
    const restrictedHeader = <WorkspaceCompactTaskHeader title="业务详情" />

    if (!errorTask && !difference) {
        return (
            <WorkspaceProcessingPane
                archetype="investigation"
                idPrefix={idPrefix}
                header={restrictedHeader}
                aria-label="当前集成异常任务"
            >
                <Alert variant="destructive">
                    <AlertTitle>暂时无法打开本条异常待办</AlertTitle>
                    <AlertDescription>
                        请刷新待办并核对关联的异常记录。
                    </AlertDescription>
                </Alert>
            </WorkspaceProcessingPane>
        )
    }

    return (
        <IntegrationErrorsPage
            key={item.workItemId}
            forcedTaskId={errorTask ? item.businessObjectId : undefined}
            forcedDifferenceId={difference ? item.businessObjectId : undefined}
            embedded
            onTaskCompleted={onTaskCompleted}
            investigation={{
                idPrefix,
                inspectorMode,
                workItemId: item.workItemId,
                title,
                number: displayName(
                    stripDocumentNumberPrefix(item.stableNumber),
                    item.businessObjectId,
                    item.workItemId,
                ),
                amount: item.amountSummary,
                deadline: item.dueAt
                    ? { date: item.dueAt, label: item.dueAtLabel }
                    : undefined,
                renderWorkbench: ({
                    canRead,
                    canReassign,
                    contextRail,
                    subtitle,
                    children,
                }) => (
                    <WorkspaceProcessingPane
                        archetype="investigation"
                        idPrefix={idPrefix}
                        className={
                            inspectorMode ? "@container/workspace" : undefined
                        }
                        contextRail={contextRail}
                        header={
                            canRead ? (
                                <WorkspaceTaskIdentityHeader
                                    item={{
                                        ...item,
                                        allowedActions:
                                            !inspectorMode && canReassign
                                                ? ["REASSIGN"]
                                                : [],
                                    }}
                                    title={title}
                                    compact
                                    subtitle={subtitle ?? null}
                                />
                            ) : (
                                restrictedHeader
                            )
                        }
                        aria-label="当前集成异常任务"
                    >
                        {children}
                    </WorkspaceProcessingPane>
                ),
            }}
        />
    )
}
