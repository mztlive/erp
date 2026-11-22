"use client"

import { WorkspaceProcessingPane } from "./workspace-processing-layout"

import { useState } from "react"
import { FileTextIcon } from "lucide-react"

import {
    DetailWorkspaceChrome,
    DetailWorkspaceMeta,
} from "@/components/business"
import { Button } from "@/components/ui/button"
import { PurchaseOrderCreatePage } from "@/features/purchase-orders/pages/purchase-order-create-page"
import { useCreationBasesQuery } from "@/features/purchase-orders/hooks/queries"
import { SalesOrderPaperPreviewDialog } from "@/features/sales-orders/components/sales-order-paper-preview-dialog"
import { Alert, AlertTitle, AlertDescription } from "@/components/ui/alert"
import { StatusBadge } from "@/components/ui/status-badge"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { isBlockedWorkItem } from "../lib/work-item"
import type { WorkspaceWorkItem } from "../types"
import { stripDocumentNumberPrefix } from "../lib/stable-number"
import { WorkspaceTaskHeaderActions } from "./workspace-task-context"

/** W01 供给分配作业面：锁定当前任务及销售责任行，正文展开供给操作。 */
export function WorkspaceProcurementTask({
    item,
    onTaskCompleted,
}: {
    item: WorkspaceWorkItem
    onTaskCompleted?: (workItemId: string) => void
}) {
    const [previewTaskId, setPreviewTaskId] = useState<string | null>(null)
    const basesQuery = useCreationBasesQuery({
        salesOrderId: item.businessObjectId,
        workItemId: item.workItemId,
    })
    const order = basesQuery.data?.find(
        (basis) => basis.salesOrderId === item.businessObjectId,
    )
    return (
        <WorkspaceProcessingPane
            className="@container/document"
            header={
                <DetailWorkspaceChrome
                    title="供给分配"
                    primaryStatus={{
                        label: item.statusLabel,
                        tone: item.statusTone,
                    }}
                    titleExtra={
                        isBlockedWorkItem(item) ? (
                            <StatusBadge label="受阻" tone="warning" />
                        ) : item.dueBucket === "overdue" ? (
                            <StatusBadge label="已超期" tone="destructive" />
                        ) : null
                    }
                    meta={
                        <DetailWorkspaceMeta
                            items={[
                                item.counterpartyName,
                                <span
                                    key="number"
                                    className="num"
                                    data-workspace-document-number={
                                        stripDocumentNumberPrefix(
                                            item.stableNumber,
                                        ) || undefined
                                    }
                                >
                                    {stripDocumentNumberPrefix(
                                        item.stableNumber,
                                    )}
                                </span>,
                                `采购处理人 ${item.ownerUserLabel}`,
                                item.dueAt
                                    ? `截止时间 ${item.dueAtLabel}`
                                    : null,
                                order?.contractNumber
                                    ? `合同 ${order.contractNumber}`
                                    : null,
                                order?.salesOwnerName
                                    ? `销售负责人 ${order.salesOwnerName}`
                                    : null,
                            ]}
                        />
                    }
                    secondaryActions={
                        <WorkspaceTaskHeaderActions
                            item={item}
                            contextLabel="任务详情"
                        >
                            <Button
                                id={`workspace-procurement-sales-preview-${toAutomationIdSegment(item.workItemId)}`}
                                type="button"
                                variant="ghost"
                                size="sm"
                                onClick={() =>
                                    setPreviewTaskId(item.workItemId)
                                }
                            >
                                <FileTextIcon aria-hidden="true" />
                                预览销售单
                            </Button>
                        </WorkspaceTaskHeaderActions>
                    }
                />
            }
            aria-label="当前供给分配任务"
        >
            {item.actionBlockers.length ? (
                <Alert variant="warning">
                    <AlertTitle>办理限制</AlertTitle>
                    <AlertDescription>
                        {item.actionBlockers
                            .map((blocker) => blocker.message)
                            .join("；")}
                    </AlertDescription>
                </Alert>
            ) : null}
            <PurchaseOrderCreatePage
                initialSalesOrderId={item.businessObjectId}
                initialWorkItemId={item.workItemId}
                embedded
                onTaskCompleted={onTaskCompleted}
            />
            <SalesOrderPaperPreviewDialog
                salesOrderId={item.businessObjectId}
                title={stripDocumentNumberPrefix(item.stableNumber)}
                open={previewTaskId === item.workItemId}
                onOpenChange={(open) =>
                    setPreviewTaskId(open ? item.workItemId : null)
                }
            />
        </WorkspaceProcessingPane>
    )
}
