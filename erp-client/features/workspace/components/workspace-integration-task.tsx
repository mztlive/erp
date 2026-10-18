"use client"

import { WorkspaceTaskPane } from "@/components/business"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { IntegrationErrorsPage } from "@/features/integration-errors/pages/integration-errors-page"

import type { WorkspaceWorkItem } from "../types"
import { WorkspaceTaskIdentityHeader } from "./workspace-task-identity-header"

/** W01 集成异常作业面：按对象类型锁定 W29 详情并提交原有强类型处理命令。 */
export function WorkspaceIntegrationTask({
    item,
    onTaskCompleted,
}: {
    item: WorkspaceWorkItem
    onTaskCompleted?: (workItemId: string) => void
}) {
    if (
        item.businessObjectType !== "integration_error_task" &&
        item.businessObjectType !== "reconciliation_difference"
    ) {
        return (
            <WorkspaceTaskPane
                header={<WorkspaceTaskIdentityHeader item={item} />}
                aria-label="当前集成异常任务"
            >
                <Alert variant="destructive">
                    <AlertTitle>暂时无法打开本条异常待办</AlertTitle>
                    <AlertDescription>
                        请刷新待办重试；仍无法打开时，联系管理员核对关联的异常记录。
                    </AlertDescription>
                </Alert>
            </WorkspaceTaskPane>
        )
    }

    return (
        <WorkspaceTaskPane
            header={<WorkspaceTaskIdentityHeader item={item} />}
            aria-label="当前集成异常任务"
        >
            <IntegrationErrorsPage
                forcedTaskId={
                    item.businessObjectType === "integration_error_task"
                        ? item.businessObjectId
                        : undefined
                }
                forcedDifferenceId={
                    item.businessObjectType === "reconciliation_difference"
                        ? item.businessObjectId
                        : undefined
                }
                embedded
                onTaskCompleted={onTaskCompleted}
            />
        </WorkspaceTaskPane>
    )
}
