"use client"

import { useQueryClient } from "@tanstack/react-query"

import { WorkspaceTaskPane } from "@/components/business"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { toast } from "@/components/ui/toast"
import { AcceptanceWorkspace } from "@/features/sales-orders/components/acceptance-workspace"
import { workspaceHomeKeys } from "@/features/workspace/hooks/queries"

import {
    workspaceAcceptanceDescriptor,
    workspaceAcceptanceTaskIdentity,
} from "../lib/workspace-acceptance"
import type { WorkspaceWorkItem } from "../types"
import { WorkspaceTaskIdentityHeader } from "./workspace-task-identity-header"

type WorkspaceAcceptanceTaskProps = Readonly<{
    item: WorkspaceWorkItem
    onTaskCompleted?: (workItemId: string) => void
}>

/** W01 客户验收作业面：任务身份锁定一张销售单，登记验收不离开工作台。 */
export function WorkspaceAcceptanceTask({
    item,
    onTaskCompleted,
}: WorkspaceAcceptanceTaskProps) {
    const queryClient = useQueryClient()
    const descriptor = workspaceAcceptanceDescriptor(item)
    const executionAuthorized = item.allowedActions.includes("PROCESS")

    return (
        <WorkspaceTaskPane
            header={<WorkspaceTaskIdentityHeader item={item} />}
            aria-label="当前客户验收任务"
        >
            {!descriptor ? (
                <Alert variant="destructive">
                    <AlertTitle>任务责任与验收对象不一致</AlertTitle>
                    <AlertDescription>
                        请联系管理员核对责任人、销售单与任务原因后重试。
                    </AlertDescription>
                </Alert>
            ) : !executionAuthorized ? (
                <Alert variant="warning">
                    <AlertTitle>当前无法登记客户验收</AlertTitle>
                    <AlertDescription>
                        {item.actionBlockers[0]?.message ??
                            "当前账号没有处理此验收任务的资格。"}
                    </AlertDescription>
                </Alert>
            ) : (
                <AcceptanceWorkspace
                    key={item.workItemId}
                    salesOrderId={descriptor.salesOrderId}
                    ownerName={item.ownerUserLabel}
                    workItem={workspaceAcceptanceTaskIdentity(item)}
                    persistRegisterInUrl={false}
                    onPosted={(payload) => {
                        void queryClient.invalidateQueries({
                            queryKey: workspaceHomeKeys.all,
                        })
                        if (payload.remainingEligibleCount > 0) return
                        toast.add({
                            title: "客户验收已登记",
                            description: payload.acceptanceNo
                                ? `${payload.acceptanceNo} 已过账，本单待验已清零`
                                : "本单待验已清零",
                            type: "success",
                            timeout: 4000,
                        })
                        onTaskCompleted?.(item.workItemId)
                    }}
                />
            )}
        </WorkspaceTaskPane>
    )
}
