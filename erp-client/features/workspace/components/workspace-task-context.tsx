"use client"

import type { ReactNode } from "react"

import {
    WorkspaceDescriptionText,
    WorkspaceHotkeyCheatSheetButton,
    WorkspaceTaskContextHelp,
    WorkspaceTaskHeaderActions as BaseWorkspaceTaskHeaderActions,
    WorkspaceTaskInstructions,
    isBlockedWorkspaceTask,
    type WorkspaceHotkeyCheatSheetButtonProps,
    type WorkspaceHotkeyCheatSheetScope,
    type WorkspaceTaskActionBlocker,
    type WorkspaceTaskApprovalContext,
    type WorkspaceTaskContextHelpProps,
    type WorkspaceTaskContextItem,
    type WorkspaceTaskHeaderActionsProps as BaseWorkspaceTaskHeaderActionsProps,
    type WorkspaceTaskInstructionsItem,
    type WorkspaceTaskProcessingState,
} from "@/components/business/task/workspace-task-context"
import { TaskTimeline } from "@/features/document-timeline/components/task-timeline"
import { timelineDocumentTypes } from "@/features/document-timeline/types"

import type { WorkspaceWorkItem } from "../types"

export {
    WorkspaceDescriptionText,
    WorkspaceHotkeyCheatSheetButton,
    WorkspaceTaskContextHelp,
    WorkspaceTaskInstructions,
    isBlockedWorkspaceTask,
    type WorkspaceHotkeyCheatSheetButtonProps,
    type WorkspaceHotkeyCheatSheetScope,
    type WorkspaceTaskActionBlocker,
    type WorkspaceTaskApprovalContext,
    type WorkspaceTaskContextHelpProps,
    type WorkspaceTaskContextItem,
    type WorkspaceTaskInstructionsItem,
    type WorkspaceTaskProcessingState,
}

export type WorkspaceTaskHeaderActionsProps = Omit<
    BaseWorkspaceTaskHeaderActionsProps,
    "item"
> & {
    item: WorkspaceWorkItem
}

/**
 * 工作台域适配壳：复用共享 `WorkspaceTaskHeaderActions`，并按单据类型自动挂载 `TaskTimeline` 入口。
 */
export function WorkspaceTaskHeaderActions({
    item,
    timelineAction,
    ...props
}: WorkspaceTaskHeaderActionsProps) {
    const resolvedTimelineAction: ReactNode =
        timelineAction !== undefined ? (
            timelineAction
        ) : timelineDocumentTypes.has(item.businessObjectType) ? (
            <TaskTimeline
                key={item.workItemId}
                taskId={item.workItemId}
                approval={Boolean(item.approval)}
                instance={item.workItemType === "APPROVAL_INSTANCE"}
                title={item.counterpartyName || item.objectTitle}
                document={{
                    type: item.businessObjectType,
                    id: item.businessObjectId,
                    number: item.stableNumber,
                }}
            />
        ) : null

    return (
        <BaseWorkspaceTaskHeaderActions
            {...props}
            item={item}
            timelineAction={resolvedTimelineAction}
        />
    )
}
