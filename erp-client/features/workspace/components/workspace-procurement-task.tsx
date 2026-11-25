"use client"

import { useEffect, useState } from "react"
import { FileTextIcon, PanelRightCloseIcon, XIcon } from "lucide-react"
import {
    BusinessFailureState,
    DetailWorkspaceMeta,
    PaperDocumentViewport,
    WorkspaceProcessingPane,
} from "@/components/business"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Button } from "@/components/ui/button"
import { Spinner } from "@/components/ui/spinner"
import { useCreationBasesQuery } from "@/features/purchase-orders/hooks/queries"
import { buildSourcingWorkspace } from "@/features/purchase-orders/lib/purchase-order-create-model"
import { earliestOrderDeliveryDeadline } from "@/features/purchase-orders/lib/sourcing/pricing"
import { PurchaseOrderCreatePage } from "@/features/purchase-orders/pages/purchase-order-create-page"
import { SalesOrderPaperDocument } from "@/features/sales-orders/components/sales-order-paper-dialog"
import { SalesOrderPaperPreviewDialog } from "@/features/sales-orders/components/sales-order-paper-preview-dialog"
import { useSalesOrderDetailQuery } from "@/features/sales-orders/hooks/queries"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { TASK_TYPE_CANONICAL_LABELS } from "@/lib/task-status-dictionary"
import { stripDocumentNumberPrefix } from "../lib/stable-number"
import type { WorkspaceWorkItem } from "../types"
import { WorkspaceTaskIdentityHeader } from "./workspace-task-identity-header"

/**
 * W01 供给分配作业面：
 * - 修复页头副标题重复标题的问题，清晰呈现客户、销售单号、合同与最早承诺交付日；
 * - 宽屏支持右侧可拖拽「对照销售单」参考轨（默认折叠，零遮挡边看边分单），窄屏平滑回退为销售单弹窗。
 */
export function WorkspaceProcurementTask({
    item,
    onTaskCompleted,
}: {
    item: WorkspaceWorkItem
    onTaskCompleted?: (workItemId: string) => void
}) {
    const [salesRailOpen, setSalesRailOpen] = useState(false)
    const [narrowPreviewOpen, setNarrowPreviewOpen] = useState(false)
    const [isWideViewport, setIsWideViewport] = useState(true)

    useEffect(() => {
        if (typeof window === "undefined") return
        const media = window.matchMedia("(min-width: 960px)")
        const sync = () => setIsWideViewport(media.matches)
        sync()
        media.addEventListener("change", sync)
        return () => media.removeEventListener("change", sync)
    }, [])

    useEffect(() => {
        setSalesRailOpen(false)
        setNarrowPreviewOpen(false)
    }, [item.workItemId])

    const basesQuery = useCreationBasesQuery({
        salesOrderId: item.businessObjectId,
        workItemId: item.workItemId,
    })
    const workspaceOrders = buildSourcingWorkspace(basesQuery.data ?? [])
    const sourcingOrder = workspaceOrders.find(
        (candidate) => candidate.salesOrderId === item.businessObjectId,
    )
    const basis = basesQuery.data?.find(
        (entry) => entry.salesOrderId === item.businessObjectId,
    )

    const resolvedTitle =
        item.workItemTypeLabel ||
        TASK_TYPE_CANONICAL_LABELS.PROCUREMENT_ORDER_CREATION.label
    const strippedStableNo = stripDocumentNumberPrefix(item.stableNumber)
    const salesOrderNo =
        sourcingOrder?.salesOrderNo ||
        (strippedStableNo && strippedStableNo !== resolvedTitle
            ? strippedStableNo
            : "")
    const customerName =
        sourcingOrder?.customerName ||
        (item.counterpartyName &&
        item.counterpartyName.trim() !== resolvedTitle.trim()
            ? item.counterpartyName
            : "")
    const earliestDeadline = earliestOrderDeliveryDeadline(sourcingOrder)

    const handleToggleSalesReference = () => {
        if (isWideViewport) {
            setSalesRailOpen((prev) => !prev)
        } else {
            setNarrowPreviewOpen(true)
        }
    }

    return (
        <WorkspaceProcessingPane
            className="@container/document"
            idPrefix={`workspace-procurement-${toAutomationIdSegment(item.workItemId)}`}
            contextRail={
                isWideViewport
                    ? {
                          collapsible: true,
                          resizable: true,
                          allowOnFullWidth: true,
                          defaultWidthPercent: 38,
                          minWidthPercent: 26,
                          maxWidthPercent: 52,
                          collapsed: !salesRailOpen,
                          onCollapsedChange: (collapsed) =>
                              setSalesRailOpen(!collapsed),
                          label: "对照来源销售单",
                          content: (
                              <SalesOrderReferenceRail
                                  salesOrderId={item.businessObjectId}
                                  salesOrderNo={salesOrderNo}
                                  enabled={salesRailOpen}
                                  onClose={() => setSalesRailOpen(false)}
                              />
                          ),
                      }
                    : undefined
            }
            header={
                <WorkspaceTaskIdentityHeader
                    item={item}
                    title={resolvedTitle}
                    subtitle={
                        <DetailWorkspaceMeta
                            items={[
                                customerName || null,
                                salesOrderNo ? (
                                    <span
                                        key="number"
                                        className="num select-all"
                                        data-workspace-document-number={
                                            salesOrderNo
                                        }
                                    >
                                        {salesOrderNo}
                                    </span>
                                ) : null,
                                basis?.contractNumber
                                    ? `合同 ${basis.contractNumber}`
                                    : null,
                                earliestDeadline ? (
                                    <span key="deadline" className="num">
                                        最晚交付 {earliestDeadline}
                                    </span>
                                ) : null,
                                item.ownerUserLabel
                                    ? `采购 ${item.ownerUserLabel}`
                                    : null,
                                basis?.salesOwnerName
                                    ? `销售 ${basis.salesOwnerName}`
                                    : null,
                                item.dueAt
                                    ? `任务截止 ${item.dueAtLabel}`
                                    : null,
                            ]}
                        />
                    }
                >
                    <Button
                        id={`workspace-procurement-sales-preview-${toAutomationIdSegment(item.workItemId)}`}
                        type="button"
                        variant={salesRailOpen ? "secondary" : "ghost"}
                        size="sm"
                        aria-pressed={
                            isWideViewport ? salesRailOpen : undefined
                        }
                        onClick={handleToggleSalesReference}
                    >
                        {salesRailOpen && isWideViewport ? (
                            <PanelRightCloseIcon
                                aria-hidden="true"
                                data-icon="inline-start"
                            />
                        ) : (
                            <FileTextIcon
                                aria-hidden="true"
                                data-icon="inline-start"
                            />
                        )}
                        {salesRailOpen && isWideViewport
                            ? "收起销售单"
                            : "对照销售单"}
                    </Button>
                </WorkspaceTaskIdentityHeader>
            }
            aria-label="当前采购供给分配任务"
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
                title={salesOrderNo || strippedStableNo}
                open={narrowPreviewOpen}
                onOpenChange={setNarrowPreviewOpen}
            />
        </WorkspaceProcessingPane>
    )
}

function SalesOrderReferenceRail({
    salesOrderId,
    salesOrderNo,
    enabled,
    onClose,
}: {
    salesOrderId: string
    salesOrderNo: string
    enabled: boolean
    onClose: () => void
}) {
    return (
        <div className="flex h-full min-h-0 flex-col bg-card">
            <div className="flex shrink-0 items-center justify-between gap-2 border-b border-border/60 py-2.5 pr-1">
                <div className="min-w-0">
                    <h2 className="truncate text-body-compact font-semibold text-foreground">
                        来源销售单对照
                    </h2>
                    {salesOrderNo ? (
                        <p className="num truncate text-xs text-muted-foreground">
                            {salesOrderNo}
                        </p>
                    ) : null}
                </div>
                <Button
                    id="workspace-procurement-sales-rail-close"
                    type="button"
                    variant="ghost"
                    size="icon-sm"
                    aria-label="收起来源销售单对照栏"
                    onClick={onClose}
                >
                    <XIcon aria-hidden="true" />
                </Button>
            </div>

            <div className="min-h-0 flex-1 overflow-y-auto overscroll-contain py-3">
                {enabled ? (
                    <SalesOrderReferenceRailBody salesOrderId={salesOrderId} />
                ) : null}
            </div>
        </div>
    )
}

function SalesOrderReferenceRailBody({
    salesOrderId,
}: {
    salesOrderId: string
}) {
    const query = useSalesOrderDetailQuery(salesOrderId)

    if (query.isPending) {
        return (
            <div className="flex min-h-48 items-center justify-center py-10">
                <p className="flex items-center gap-2 text-xs text-muted-foreground">
                    <Spinner aria-label="正在读取销售单" />
                    正在读取销售单…
                </p>
            </div>
        )
    }

    if (query.isError) {
        return (
            <BusinessFailureState
                title="销售单读取失败"
                error={query.error}
                action={
                    <Button
                        id="workspace-procurement-sales-rail-retry"
                        type="button"
                        variant="outline"
                        size="sm"
                        onClick={() => void query.refetch()}
                    >
                        重试
                    </Button>
                }
            />
        )
    }

    if (!query.data) {
        return (
            <p className="py-10 text-center text-xs text-muted-foreground">
                未找到这张销售单，可能已删除或当前角色无权查看。
            </p>
        )
    }

    return (
        <PaperDocumentViewport
            fitMode="width"
            role="region"
            aria-label="来源销售单纸面"
            tabIndex={0}
            fitKey={salesOrderId}
            className="bg-transparent p-0"
        >
            <SalesOrderPaperDocument order={query.data} />
        </PaperDocumentViewport>
    )
}
