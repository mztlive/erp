"use client"

import { useEffect, useRef, useState } from "react"
import Link from "next/link"
import { usePathname, useSearchParams } from "next/navigation"
import { FileTextIcon } from "lucide-react"

import { WorkspaceTaskPane } from "@/components/business"
import { Button } from "@/components/ui/button"
import {
    Tooltip,
    TooltipContent,
    TooltipTrigger,
} from "@/components/ui/tooltip"
import { PurchaseSourceUpdateWorkspace } from "@/features/purchase-orders/components/purchase-source-update-workspace"
import { useWorkItemDetailQuery } from "@/features/work-items/queries"
import { toAutomationIdSegment } from "@/lib/automation-id"
import type { WorkspaceWorkItem } from "../types"
import { stripDocumentNumberPrefix } from "../lib/stable-number"
import { WorkspaceTaskIdentityHeader } from "./workspace-task-identity-header"
import { WorkspaceDocumentPaperDialog } from "./workspace-document-paper-dialog"

/** 采购依据更新：原地办理，只有服务端确认任务完成才推进队列。 */
export function WorkspacePurchaseSourceUpdateTask({
    item,
    onTaskCompleted,
}: {
    item: WorkspaceWorkItem
    onTaskCompleted?: (workItemId: string) => void
}) {
    const [previewOpen, setPreviewOpen] = useState(false)
    const pathname = usePathname()
    const searchParams = useSearchParams()
    const returnParams = new URLSearchParams(searchParams.toString())
    returnParams.set("currentWorkItemId", item.workItemId)
    const returnTo = `${pathname}?${returnParams}`
    const purchaseHref = `/procurement/orders/${encodeURIComponent(item.businessObjectId)}?${new URLSearchParams({ from: "workspace", returnTo })}`
    const taskQuery = useWorkItemDetailQuery(item.workItemId)
    const notified = useRef(false)
    useEffect(() => {
        if (
            taskQuery.isError ||
            taskQuery.isFetching ||
            taskQuery.data?.status !== "COMPLETED" ||
            notified.current
        )
            return
        notified.current = true
        onTaskCompleted?.(item.workItemId)
    }, [
        taskQuery.data?.status,
        taskQuery.isError,
        taskQuery.isFetching,
        item.workItemId,
        onTaskCompleted,
    ])

    return (
        <WorkspaceTaskPane
            header={
                <WorkspaceTaskIdentityHeader
                    item={item}
                    title="处理销售变更影响"
                    subtitle={
                        <span className="flex flex-wrap gap-x-3 gap-y-1">
                            <span>{item.counterpartyName}</span>
                            <Link
                                id={`workspace-source-update-purchase-${toAutomationIdSegment(item.workItemId)}`}
                                href={purchaseHref}
                                className="underline decoration-border underline-offset-4 hover:decoration-foreground focus-visible:outline-2 focus-visible:outline-offset-4"
                                aria-label={`打开采购单 ${stripDocumentNumberPrefix(item.stableNumber)}`}
                            >
                                查看采购单
                            </Link>
                            <span>采购负责人：{item.ownerUserLabel}</span>
                        </span>
                    }
                >
                    <Tooltip>
                        <TooltipTrigger
                            render={
                                <Button
                                    id={`workspace-source-update-preview-${toAutomationIdSegment(item.workItemId)}`}
                                    type="button"
                                    variant="ghost"
                                    size="icon-sm"
                                    aria-label="查看采购单"
                                    onClick={() => setPreviewOpen(true)}
                                />
                            }
                        >
                            <FileTextIcon aria-hidden="true" />
                        </TooltipTrigger>
                        <TooltipContent>查看采购单</TooltipContent>
                    </Tooltip>
                </WorkspaceTaskIdentityHeader>
            }
            aria-label="当前采购依据更新任务"
        >
            <PurchaseSourceUpdateWorkspace
                purchaseOrderId={item.businessObjectId}
                returnTo={returnTo}
                onRefreshTask={() => taskQuery.refetch()}
            />
            <WorkspaceDocumentPaperDialog
                target={
                    previewOpen
                        ? {
                              kind: "purchase_order",
                              objectId: item.businessObjectId,
                              title: stripDocumentNumberPrefix(
                                  item.stableNumber,
                              ),
                          }
                        : null
                }
                open={previewOpen}
                onOpenChange={setPreviewOpen}
            />
        </WorkspaceTaskPane>
    )
}
