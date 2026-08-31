"use client"

import { WorkspaceTaskPane } from "@/components/business"
import { CardFundsReviewPage } from "@/features/card-funds-review/pages/card-funds-review-page"

import type { WorkspaceWorkItem } from "../types"
import { WorkspaceTaskIdentityHeader } from "./workspace-task-identity-header"

/** W01 卡券票款复核：固定当前 W13 任务，复用完整票款事实与强类型决定面。 */
export function WorkspaceCardFundsTask({
    item,
    onTaskCompleted,
}: {
    item: WorkspaceWorkItem
    onTaskCompleted?: (workItemId: string, preferredWorkItemId?: string) => void
}) {
    return (
        <WorkspaceTaskPane
            header={<WorkspaceTaskIdentityHeader item={item} />}
            aria-label="当前卡券票款复核任务"
        >
            <CardFundsReviewPage
                forcedWorkItemId={item.workItemId}
                embedded
                onTaskCompleted={onTaskCompleted}
            />
        </WorkspaceTaskPane>
    )
}
