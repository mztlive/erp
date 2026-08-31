"use client"

import { Badge } from "@/components/ui/badge"
import {
    ObjectSectionTabs,
    ObjectSectionTabsPanel,
} from "@/components/business"
import type { SalesOrderDetailView } from "@/features/sales-orders/api/sales-orders"
import type { WorkItemProjection } from "@/features/work-items/types"
import { ApprovalPanel } from "@/features/sales-orders/components/sales-order-detail-approval-panel"
import {
    AcceptancePanel,
    CollaborationPanel,
    OverviewPanel,
    PurchasePanel,
    ReceivablePanel,
    VersionsPanel,
} from "@/features/sales-orders/components/sales-order-detail-panels"
import { RelatedLanes } from "@/features/sales-orders/components/sales-order-detail-related-lanes"
import {
    isSalesOrderApprovalInProgress,
    isWorkSection,
    type NavSectionId,
    type SalesOrderDetailActionResult,
    type WorkSectionId,
} from "@/features/sales-orders/lib/sales-order-detail-model"

export function SalesOrderDetailTabs({
    order,
    selfReturn,
    section,
    navSection,
    visibleNav,
    canAccept,
    focusedWorkItem,
    onSelectSection,
    onApprovalResult,
    onDataChanged,
}: {
    order: SalesOrderDetailView
    selfReturn: string
    section?: string
    navSection: NavSectionId
    visibleNav: Array<{
        id: NavSectionId
        label: string
        hint: string
        show: boolean
    }>
    canAccept: boolean
    focusedWorkItem?: WorkItemProjection
    onSelectSection: (
        next: NavSectionId | WorkSectionId | "versions",
        extras?: { mode?: "register" },
    ) => void
    onApprovalResult: (result: SalesOrderDetailActionResult) => void
    onDataChanged: () => void
}) {
    const items = visibleNav.map((item) => {
        const todoOnAcceptance = item.id === "acceptance" && Boolean(canAccept)
        const changeOnVersions =
            item.id === "versions" && Boolean(order.activeChangeOrder)
        const approvalPending =
            item.id === "approval" && isSalesOrderApprovalInProgress(order)
        const purchaseCount =
            item.id === "fulfillment" ? order.related.purchaseOrders : 0

        return {
            id: item.id,
            label: item.label,
            title: item.hint,
            badge:
                todoOnAcceptance || changeOnVersions || approvalPending ? (
                    <Badge
                        variant={changeOnVersions ? "warning" : "info"}
                        className="h-5 px-1.5 text-2xs font-normal"
                    >
                        {changeOnVersions
                            ? "改单中"
                            : approvalPending
                              ? "进行中"
                              : "待办"}
                    </Badge>
                ) : purchaseCount > 0 ? (
                    <Badge
                        variant="secondary"
                        className="h-5 px-1.5 text-2xs font-normal"
                    >
                        {purchaseCount}
                    </Badge>
                ) : undefined,
        }
    })

    return (
        <ObjectSectionTabs
            id={`sales-orders-detail-tabs-${order.id}`}
            value={navSection}
            onValueChange={(next) => {
                const target = next as NavSectionId
                if (target !== navSection || isWorkSection(section)) {
                    onSelectSection(target)
                }
            }}
            items={items}
            listLabel="销售单分区"
        >
            <ObjectSectionTabsPanel value="overview">
                <OverviewPanel order={order} />
                {order.nature === "physical_service" ? (
                    <section
                        className="rounded-lg border border-grid px-3"
                        aria-labelledby="sales-order-procurement-heading"
                    >
                        <h2
                            id="sales-order-procurement-heading"
                            className="border-b border-grid py-2 text-sm font-medium"
                        >
                            采购进度
                        </h2>
                        <RelatedLanes
                            order={order}
                            selfReturn={selfReturn}
                            lanes={["purchase"]}
                        />
                    </section>
                ) : null}
            </ObjectSectionTabsPanel>

            <ObjectSectionTabsPanel value="approval">
                <ApprovalPanel
                    order={order}
                    workItemId={focusedWorkItem?.workItemId}
                    expectedTaskVersion={focusedWorkItem?.taskVersion}
                    workItemAllowedActions={focusedWorkItem?.allowedActions}
                    onApprovalResult={onApprovalResult}
                />
            </ObjectSectionTabsPanel>

            <ObjectSectionTabsPanel value="fulfillment">
                <PurchasePanel order={order} selfReturn={selfReturn} />
            </ObjectSectionTabsPanel>

            <ObjectSectionTabsPanel value="acceptance">
                <AcceptancePanel order={order} workItem={focusedWorkItem} />
            </ObjectSectionTabsPanel>

            <ObjectSectionTabsPanel value="receivable" keepMounted>
                <ReceivablePanel
                    order={order}
                    selfReturn={selfReturn}
                    onDataChanged={onDataChanged}
                />
            </ObjectSectionTabsPanel>

            <ObjectSectionTabsPanel value="collaboration">
                <CollaborationPanel order={order} />
            </ObjectSectionTabsPanel>

            <ObjectSectionTabsPanel value="versions">
                <VersionsPanel
                    order={order}
                    onApprovalResult={onApprovalResult}
                />
            </ObjectSectionTabsPanel>
        </ObjectSectionTabs>
    )
}
