"use client"

import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { IntegrationErrorsPage } from "@/features/integration-errors/pages/integration-errors-page"

import type { WorkspaceWorkItem } from "../types"

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
            <Alert variant="destructive">
                <AlertTitle>异常任务对象未注册</AlertTitle>
                <AlertDescription>
                    当前任务没有可验证的 W29 业务对象，已停止提供处理动作。
                </AlertDescription>
            </Alert>
        )
    }

    return (
        <section
            className="h-full min-h-0 overflow-auto"
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
        </section>
    )
}
