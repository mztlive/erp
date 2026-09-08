"use client"

import { useState } from "react"
import { usePathname, useSearchParams } from "next/navigation"
import { ArrowUpRightIcon, FileTextIcon } from "lucide-react"
import { WorkspaceTaskPane } from "@/components/business"
import { Button, buttonVariants } from "@/components/ui/button"
import {
    Tooltip,
    TooltipContent,
    TooltipTrigger,
} from "@/components/ui/tooltip"
import { PurchaseOrderCreatePage } from "@/features/purchase-orders/pages/purchase-order-create-page"
import { SalesOrderPaperPreviewDialog } from "@/features/sales-orders/components/sales-order-paper-preview-dialog"
import { StatusBadge } from "@/components/ui/status-badge"
import { isBlockedWorkItem } from "../lib/work-item"
import { toAutomationIdSegment } from "@/lib/automation-id"
import type { WorkspaceWorkItem } from "../types"
import { stripDocumentNumberPrefix } from "../lib/stable-number"
import { WorkspaceTaskHeaderActions } from "./workspace-task-context"

/** W01 供给分配作业面：锁定销售单，在紧凑页头查看原单，正文只展开供给操作。 */
export function WorkspaceProcurementTask({
    item,
    onTaskCompleted,
}: {
    item: WorkspaceWorkItem
    onTaskCompleted?: (workItemId: string) => void
}) {
    const [previewId, setPreviewId] = useState<string | null>(null)
    const pathname = usePathname()
    const searchParams = useSearchParams()
    const returnTo = `${pathname}${searchParams.toString() ? `?${searchParams}` : ""}`
    const href = `/sales/orders/${encodeURIComponent(item.businessObjectId)}?${new URLSearchParams({ from: "workspace", returnTo })}`
    const id = `workspace-procurement-${toAutomationIdSegment(item.workItemId)}`
    return (
        <WorkspaceTaskPane
            className="@container/document"
            header={
                <>
                    <div className="min-w-0 space-y-2">
                        <div className="flex flex-wrap items-center gap-2">
                            <h2 className="text-xl font-semibold tracking-tight">
                                供给分配
                            </h2>
                            {isBlockedWorkItem(item) ? (
                                <StatusBadge label="受阻" tone="warning" />
                            ) : item.dueBucket === "overdue" ? (
                                <StatusBadge
                                    label="已超期"
                                    tone="destructive"
                                />
                            ) : null}
                        </div>
                        <div className="flex flex-wrap items-baseline gap-x-3 gap-y-1">
                            {item.counterpartyName ? (
                                <span className="text-sm font-medium">
                                    {item.counterpartyName}
                                </span>
                            ) : null}
                            <span className="num text-xs text-muted-foreground">
                                {stripDocumentNumberPrefix(item.stableNumber)}
                            </span>
                        </div>
                    </div>
                    <WorkspaceTaskHeaderActions item={item}>
                        <Tooltip>
                            <TooltipTrigger
                                render={
                                    <Button
                                        id={`${id}-preview`}
                                        type="button"
                                        size="icon-sm"
                                        variant="ghost"
                                        aria-label="查看销售单"
                                        onClick={() =>
                                            setPreviewId(item.businessObjectId)
                                        }
                                    />
                                }
                            >
                                <FileTextIcon aria-hidden="true" />
                            </TooltipTrigger>
                            <TooltipContent>查看销售单</TooltipContent>
                        </Tooltip>
                        <Tooltip>
                            <TooltipTrigger
                                render={
                                    <a
                                        id={`${id}-open`}
                                        href={href}
                                        aria-label="打开销售单"
                                        className={buttonVariants({
                                            variant: "ghost",
                                            size: "icon-sm",
                                        })}
                                    />
                                }
                            >
                                <ArrowUpRightIcon aria-hidden="true" />
                            </TooltipTrigger>
                            <TooltipContent>打开销售单</TooltipContent>
                        </Tooltip>
                    </WorkspaceTaskHeaderActions>
                </>
            }
            aria-label="当前供给分配任务"
        >
            <PurchaseOrderCreatePage
                initialSalesOrderId={item.businessObjectId}
                initialWorkItemId={item.workItemId}
                embedded
                onTaskCompleted={onTaskCompleted}
            />
            <SalesOrderPaperPreviewDialog
                salesOrderId={
                    previewId === item.businessObjectId ? previewId : null
                }
                title={stripDocumentNumberPrefix(item.stableNumber)}
                open={previewId === item.businessObjectId}
                onOpenChange={(open) => {
                    if (!open) setPreviewId(null)
                }}
            />
        </WorkspaceTaskPane>
    )
}
