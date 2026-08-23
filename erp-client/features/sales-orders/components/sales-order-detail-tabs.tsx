"use client"

import { Badge } from "@/components/ui/badge"
import {
    ObjectSectionTabs,
    ObjectSectionTabsPanel,
} from "@/components/business"
import type { SalesOrderDetailView } from "@/features/sales-orders/api/sales-orders"
import { ApprovalPanel } from "@/features/sales-orders/components/sales-order-detail-approval-panel"
import {
    CollaborationPanel,
    FulfillmentPanel,
    OverviewPanel,
    ReceivablePanel,
    VersionsPanel,
} from "@/features/sales-orders/components/sales-order-detail-panels"
import {
    isWorkSection,
    type NavSectionId,
    type SalesOrderDetailActionResult,
    type WorkSectionId,
} from "@/features/sales-orders/lib/sales-order-detail-model"

export function SalesOrderDetailTabs({
    order,
    section,
    navSection,
    visibleNav,
    canAccept,
    acceptanceExpanded,
    selfReturn,
    workItemId,
    expectedTaskVersion,
    workItemAllowedActions,
    onSelectSection,
    onApprovalResult,
}: {
    order: SalesOrderDetailView
    section?: string
    navSection: NavSectionId
    visibleNav: Array<{
        id: NavSectionId
        label: string
        hint: string
        show: boolean
    }>
    canAccept: boolean
    acceptanceExpanded: boolean
    selfReturn: string
    workItemId?: string
    expectedTaskVersion?: string
    workItemAllowedActions?: readonly string[]
    onSelectSection: (next: NavSectionId | WorkSectionId | "versions") => void
    onApprovalResult: (result: SalesOrderDetailActionResult) => void
}) {
    const items = visibleNav.map((item) => {
        const todoOnFulfillment =
            item.id === "fulfillment" && Boolean(canAccept)
        const changeOnVersions =
            item.id === "versions" && Boolean(order.activeChangeOrder)
        const approvalPending =
            item.id === "approval" && Boolean(order.approval?.instance)

        return {
            id: item.id,
            label: item.label,
            title: item.hint,
            badge:
                todoOnFulfillment || changeOnVersions || approvalPending ? (
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
                ) : undefined,
        }
    })

    return (
        <ObjectSectionTabs
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
            </ObjectSectionTabsPanel>

            <ObjectSectionTabsPanel value="approval">
                <ApprovalPanel
                    order={order}
                    workItemId={workItemId}
                    expectedTaskVersion={expectedTaskVersion}
                    workItemAllowedActions={workItemAllowedActions}
                    onApprovalResult={onApprovalResult}
                />
            </ObjectSectionTabsPanel>

            <ObjectSectionTabsPanel value="fulfillment">
                <FulfillmentPanel
                    order={order}
                    selfReturn={selfReturn}
                    acceptanceExpanded={acceptanceExpanded}
                    canAccept={Boolean(canAccept)}
                    onExpandAcceptance={() => onSelectSection("acceptance")}
                    onCollapseAcceptance={() => onSelectSection("fulfillment")}
                />
            </ObjectSectionTabsPanel>

            <ObjectSectionTabsPanel value="receivable">
                <ReceivablePanel order={order} selfReturn={selfReturn} />
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
