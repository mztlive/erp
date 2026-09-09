"use client"

import type { ReactNode } from "react"

import { Badge } from "@/components/ui/badge"
import {
    ObjectSectionTabs,
    ObjectSectionTabsPanel,
} from "@/components/business"
import type { SalesOrderDetailView } from "@/features/sales-orders/api/sales-orders"
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

    section,
    navSection,
    visibleNav,
    canAccept,
    onSelectSection,
    onApprovalResult,
    sidebar,
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
    onSelectSection: (
        next: NavSectionId | WorkSectionId | "versions",
        extras?: { mode?: "register" },
    ) => void
    onApprovalResult: (result: SalesOrderDetailActionResult) => void
    sidebar?: ReactNode
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
            listClassName="border-border/70"
            sidebar={sidebar}
        >
            <ObjectSectionTabsPanel value="overview" className="py-5">
                <OverviewPanel
                    order={order}
                    related={
                        order.nature === "physical_service" ? (
                            <section
                                className="rounded-lg border border-border/70 px-4 py-3 md:px-5"
                                aria-labelledby="sales-order-procurement-heading"
                            >
                                <h2
                                    id="sales-order-procurement-heading"
                                    className="mb-1 text-lg font-semibold"
                                >
                                    采购进度
                                </h2>
                                <RelatedLanes
                                    order={order}

                                    lanes={["purchase"]}
                                />
                            </section>
                        ) : undefined
                    }
                />
            </ObjectSectionTabsPanel>

            <ObjectSectionTabsPanel value="approval">
                <ApprovalPanel order={order} />
            </ObjectSectionTabsPanel>

            <ObjectSectionTabsPanel value="fulfillment">
                <PurchasePanel order={order} />
            </ObjectSectionTabsPanel>

            <ObjectSectionTabsPanel value="acceptance">
                <AcceptancePanel order={order} />
            </ObjectSectionTabsPanel>

            <ObjectSectionTabsPanel value="receivable">
                <ReceivablePanel key={order.id} order={order} />
            </ObjectSectionTabsPanel>

            <ObjectSectionTabsPanel value="collaboration">
                <CollaborationPanel order={order} />
            </ObjectSectionTabsPanel>

            <ObjectSectionTabsPanel value="versions">
                <VersionsPanel
                    showActiveChange={section !== "change-review"}
                    order={order}
                    onApprovalResult={onApprovalResult}
                />
            </ObjectSectionTabsPanel>
        </ObjectSectionTabs>
    )
}
