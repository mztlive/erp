"use client"

import { workspaceTaskSurfaceClassName } from "@/components/business"
import { cn } from "@/lib/utils"

import { WorkspaceTaskSurfaceBoundary } from "./workspace-task-surface-boundary"
import {
    WorkspaceTaskSurface,
    type WorkspaceTaskDetailProps,
} from "./workspace-task-surface"

/**
 * 工作台作业面。金额、单据字段、明细全部展开，按区块分层。
 * 任务说明收在标题栏问号里，不单独占一块说明区。
 * 履约、付款、开票与客户验收在本页提交正式命令；审批通过、驳回留在底栏。
 */
export function WorkspaceTaskDetail(props: WorkspaceTaskDetailProps) {
    return (
        <section
            data-slot="workspace-task-surface"
            className={cn(
                workspaceTaskSurfaceClassName,
                "relative flex h-full min-h-0 flex-1 flex-col",
            )}
            aria-label="当前工作台任务"
        >
            <div className="min-h-0 flex-1">
                <WorkspaceTaskSurfaceBoundary
                    workItemId={props.item.workItemId}
                >
                    <WorkspaceTaskSurface {...props} />
                </WorkspaceTaskSurfaceBoundary>
            </div>
        </section>
    )
}
