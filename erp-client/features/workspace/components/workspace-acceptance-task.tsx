"use client"

import { WorkspaceProcessingPane } from "./workspace-processing-layout"

import * as React from "react"
import { useQueryClient } from "@tanstack/react-query"
import { usePathname, useSearchParams } from "next/navigation"
import { ArrowUpRightIcon, FileTextIcon } from "lucide-react"

import { IconActionButton } from "@/components/business"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { toast } from "@/components/ui/toast"
import { AcceptanceWorkspace } from "@/features/sales-orders/components/acceptance-workspace"
import { workspaceReadActionLabel } from "@/features/workspace/api/work-item-meta"
import { workspaceHomeKeys } from "@/features/workspace/hooks/queries"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { PermissionGuard } from "@/features/auth/components/permission-guard"

import { sourceSalesOrderHref } from "../lib/source-sales-order"
import { stripDocumentNumberPrefix } from "../lib/stable-number"
import {
    workspaceAcceptanceDescriptor,
    workspaceAcceptanceTaskIdentity,
} from "../lib/workspace-acceptance"
import type { WorkspaceWorkItem } from "../types"
import {
    WorkspaceDocumentPaperDialog,
    type WorkspacePaperTarget,
} from "./workspace-document-paper-dialog"
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
    const pathname = usePathname()
    const searchParams = useSearchParams()
    const returnTo = `${pathname}${searchParams.toString() ? `?${searchParams}` : ""}`
    const [paper, setPaper] = React.useState<WorkspacePaperTarget | null>(null)
    const salesOrderId = descriptor?.salesOrderId
    const salesOrderNo = stripDocumentNumberPrefix(item.stableNumber)
    const readSalesLabel = workspaceReadActionLabel("sales_order")

    return (
        <WorkspaceProcessingPane
            header={
                <WorkspaceTaskIdentityHeader item={item}>
                    {salesOrderId ? (
                        <PermissionGuard permission="sales_order:detail">
                            <IconActionButton
                                id={`workspace-acceptance-preview-so-${toAutomationIdSegment(item.workItemId)}`}
                                label={readSalesLabel}
                                testId={`work-item-read-sales-order-${item.workItemId}`}
                                onClick={() =>
                                    setPaper({
                                        kind: "sales_order",
                                        objectId: salesOrderId,
                                        title: salesOrderNo,
                                    })
                                }
                            >
                                <FileTextIcon aria-hidden="true" />
                            </IconActionButton>
                            <IconActionButton
                                id={`workspace-acceptance-open-so-${toAutomationIdSegment(item.workItemId)}`}
                                label="打开销售单"
                                testId={`work-item-open-sales-order-${item.workItemId}`}
                                href={sourceSalesOrderHref(
                                    salesOrderId,
                                    returnTo,
                                )}
                            >
                                <ArrowUpRightIcon aria-hidden="true" />
                            </IconActionButton>
                        </PermissionGuard>
                    ) : null}
                </WorkspaceTaskIdentityHeader>
            }
            aria-label="当前客户验收任务"
        >
            {!descriptor ? (
                <Alert variant="destructive">
                    <AlertTitle>待办关联的单据信息异常</AlertTitle>
                    <AlertDescription>
                        当前待办与关联业务单据不匹配，请刷新页面后重试，或联系管理员处理。
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
            <WorkspaceDocumentPaperDialog
                target={paper}
                open={Boolean(paper)}
                onOpenChange={(open) => {
                    if (!open) setPaper(null)
                }}
            />
        </WorkspaceProcessingPane>
    )
}
