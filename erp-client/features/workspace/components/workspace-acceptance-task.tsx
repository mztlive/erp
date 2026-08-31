"use client"

import { useQueryClient } from "@tanstack/react-query"

import { workspaceTaskSurfacePadClassName } from "@/components/business"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { toast } from "@/components/ui/toast"
import { AcceptanceWorkspace } from "@/features/sales-orders/components/acceptance-workspace"
import { workspaceHomeKeys } from "@/features/workspace/hooks/queries"
import { cn } from "@/lib/utils"

import {
    workspaceAcceptanceDescriptor,
    workspaceAcceptanceTaskIdentity,
} from "../lib/workspace-acceptance"
import type { WorkspaceWorkItem } from "../types"
import { WorkspaceDocumentBadge } from "./workspace-document-badge"
import { WorkspaceTaskHeaderActions } from "./workspace-task-context"

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
        <section
            className="flex h-full min-h-0 flex-col"
            aria-label="当前客户验收任务"
        >
            <header
                className={cn(
                    workspaceTaskSurfacePadClassName,
                    "flex shrink-0 items-start justify-between gap-3 border-b border-grid py-5",
                )}
            >
                <div className="flex min-w-0 flex-col gap-2">
                    <WorkspaceDocumentBadge item={item} />
                    <h2 className="text-xl font-semibold tracking-tight">
                        {item.objectTitle}
                    </h2>
                    <p className="text-sm text-muted-foreground">
                        {[
                            `${item.ownerRoleLabel} · ${item.ownerUserLabel}`,
                            item.counterpartyName,
                        ]
                            .filter(Boolean)
                            .join(" · ")}
                    </p>
                </div>
                <WorkspaceTaskHeaderActions item={item} />
            </header>

            <div className="min-h-0 flex-1 overflow-auto [&>[data-slot=alert]]:mx-5 [&>[data-slot=alert]]:my-5">
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
            </div>
        </section>
    )
}
