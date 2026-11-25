"use client"

import type { ReactNode } from "react"

import {
    WorkspaceCompactTaskHeader,
    WorkspaceTaskIdentityHeader as BaseWorkspaceTaskIdentityHeader,
    type WorkspaceCompactTaskHeaderProps,
    type WorkspaceTaskIdentityHeaderProps as BaseWorkspaceTaskIdentityHeaderProps,
    type WorkspaceTaskIdentityItem,
} from "@/components/business/task/workspace-task-identity-header"

import type { WorkspaceWorkItem } from "../types"
import { WorkspaceTaskHeaderActions } from "./workspace-task-context"
import { WorkspaceTaskResponsibilityActions } from "./workspace-task-responsibility-actions"

export {
    WorkspaceCompactTaskHeader,
    type WorkspaceCompactTaskHeaderProps,
    type WorkspaceTaskIdentityItem,
}

export type WorkspaceTaskIdentityHeaderProps = Omit<
    BaseWorkspaceTaskIdentityHeaderProps,
    "item"
> & {
    item: WorkspaceWorkItem
}

/**
 * 工作台域适配壳：复用共享 `WorkspaceTaskIdentityHeader`，并自动注入责任流转与时间线头部动作。
 */
export function WorkspaceTaskIdentityHeader({
    item,
    actions,
    leadingActions,
    timelineAction,
    children,
    ...props
}: WorkspaceTaskIdentityHeaderProps) {
    const resolvedActions: ReactNode =
        actions !== undefined ? (
            actions
        ) : (
            <WorkspaceTaskHeaderActions
                item={item}
                timelineAction={timelineAction}
            >
                <WorkspaceTaskResponsibilityActions item={item} />
                {leadingActions}
                {children}
            </WorkspaceTaskHeaderActions>
        )

    return (
        <BaseWorkspaceTaskIdentityHeader
            {...props}
            item={item}
            actions={resolvedActions}
        />
    )
}
