"use client"

import * as React from "react"
import { ArrowRightLeftIcon, CircleOffIcon } from "lucide-react"

import {
    WorkItemResponsibilityDialog,
    type WorkItemResponsibilityTarget,
} from "@/features/work-items"
import { Button } from "@/components/ui/button"
import { toAutomationIdSegment } from "@/lib/automation-id"
import type { WorkspaceWorkItem } from "../types"

type TaskCompletedHandler = (
    workItemId: string,
    preferredWorkItemId?: string,
) => void

const WorkspaceTaskCompletionContext = React.createContext<
    TaskCompletedHandler | undefined
>(undefined)

/** 为工作台作业面树提供统一任务完成/转交/关闭后的队列推进回调。 */
export function WorkspaceTaskCompletionProvider({
    onTaskCompleted,
    children,
}: {
    onTaskCompleted?: TaskCompletedHandler
    children: React.ReactNode
}) {
    return (
        <WorkspaceTaskCompletionContext.Provider value={onTaskCompleted}>
            {children}
        </WorkspaceTaskCompletionContext.Provider>
    )
}

/**
 * 统一工作台页头责任操作：严格由 `item.allowedActions` 驱动 `REASSIGN` 与 `CLOSE`，
 * 覆盖全部 14 类可转交事项及管理范围只读视图。
 */
export function WorkspaceTaskResponsibilityActions({
    item,
    onCompleted,
}: {
    item: WorkspaceWorkItem
    onCompleted?: (workItemId: string) => void
}) {
    const contextOnCompleted = React.useContext(WorkspaceTaskCompletionContext)
    const [mode, setMode] = React.useState<"reassign" | "close" | null>(null)

    const canReassign =
        item.status === "OPEN" && item.allowedActions.includes("REASSIGN")
    const canClose =
        item.status === "OPEN" && item.allowedActions.includes("CLOSE")

    const segment = toAutomationIdSegment(item.workItemId)
    const isFulfillment = item.workItemType === "FULFILLMENT_OPERATION"
    const target = React.useMemo<WorkItemResponsibilityTarget>(
        () => ({
            workItemId: item.workItemId,
            label:
                [item.counterpartyName, item.stableNumber]
                    .filter(Boolean)
                    .join(" · ") || item.objectTitle,
            expectedTaskVersion: item.taskVersion,
            currentOwnerLabel: item.ownerUserLabel,
        }),
        [
            item.counterpartyName,
            item.objectTitle,
            item.ownerUserLabel,
            item.stableNumber,
            item.taskVersion,
            item.workItemId,
        ],
    )

    if (!canReassign && !canClose) return null

    const handleCompleted = onCompleted ?? contextOnCompleted
    const reassignIdPrefix = isFulfillment
        ? "workspace-fulfillment-reassign"
        : "workspace-task-reassign"

    return (
        <>
            {canReassign ? (
                <Button
                    id={
                        isFulfillment
                            ? `workspace-fulfillment-reassign-trigger-${segment}`
                            : `workspace-task-reassign-trigger-${segment}`
                    }
                    type="button"
                    variant="outline"
                    size="sm"
                    onClick={() => setMode("reassign")}
                >
                    <ArrowRightLeftIcon
                        data-icon="inline-start"
                        aria-hidden="true"
                    />
                    转交他人
                </Button>
            ) : null}
            {canClose ? (
                <Button
                    id={`workspace-task-close-trigger-${segment}`}
                    type="button"
                    variant="outline"
                    size="sm"
                    onClick={() => setMode("close")}
                >
                    <CircleOffIcon
                        data-icon="inline-start"
                        aria-hidden="true"
                    />
                    关闭任务
                </Button>
            ) : null}
            {mode ? (
                <WorkItemResponsibilityDialog
                    mode={mode}
                    targets={[target]}
                    open
                    onOpenChange={(next) => {
                        if (!next) setMode(null)
                    }}
                    idPrefix={
                        mode === "reassign"
                            ? reassignIdPrefix
                            : "workspace-task-close"
                    }
                    title={
                        mode === "reassign" && isFulfillment
                            ? "转交履约待办"
                            : undefined
                    }
                    description={
                        mode === "reassign" && isFulfillment
                            ? `当前责任人：${item.ownerUserLabel}。采购单责任转交会同步更新该采购单全部待处理的交付任务；历史任务不变。`
                            : undefined
                    }
                    onCompleted={(summary) => {
                        if (summary.succeededIds.includes(item.workItemId)) {
                            handleCompleted?.(item.workItemId)
                        }
                    }}
                />
            ) : null}
        </>
    )
}
