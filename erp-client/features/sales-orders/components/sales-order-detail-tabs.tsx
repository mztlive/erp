"use client"

import { SalesTimeline } from "@/features/document-timeline/components/sales-timeline"
import { toAutomationIdSegment } from "@/lib/automation-id"
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
    VersionsPanel,
} from "@/features/sales-orders/components/sales-order-detail-panels"
import { SalesOrderCustomerFinancePanel } from "./sales-order-detail-receivable-panel"
import { SalesOrderDocumentsPanel } from "./sales-order-documents-panel"
import { ExtraCostPanel } from "@/features/sales-orders/components/sales-order-extra-cost-panel"
import { SalesOrderProcurementProgress } from "@/features/sales-orders/components/sales-order-procurement-progress"
import { compareDecimal } from "@/lib/fixed-decimal"
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
    beforeContent,
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
    beforeContent?: ReactNode
}) {
    const items = visibleNav.map((item) => {
        const todoOnAcceptance = item.id === "acceptance" && Boolean(canAccept)
        const changeOnVersions =
            item.id === "versions" && Boolean(order.activeChangeOrder)
        const approvalPending =
            item.id === "approval" && isSalesOrderApprovalInProgress(order)
        const purchaseCount =
            item.id === "fulfillment" ? order.related.purchaseOrders : 0

        const purchaseNeedsAttention =
            item.id === "fulfillment" &&
            (compareDecimal(
                order.related.procurementProgress.remainingQuantity,
                "0",
                6,
            ) > 0 ||
                order.related.procurementProgress.sourceChanged)

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
                ) : purchaseCount > 0 || purchaseNeedsAttention ? (
                    <Badge
                        variant={
                            purchaseNeedsAttention ? "warning" : "secondary"
                        }
                        className="h-5 px-1.5 text-2xs font-normal"
                    >
                        {purchaseCount > 0 ? purchaseCount : "待覆盖"}
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
            scrollMode="panels"
            beforeContent={beforeContent}
        >
            <ObjectSectionTabsPanel value="overview" className="py-5">
                <OverviewPanel
                    order={order}
                    related={
                        order.nature === "physical_service" ? (
                            <SalesOrderProcurementProgress
                                order={order}
                                surface="overview"
                            />
                        ) : undefined
                    }
                />
                {navSection === "overview" && (
                    <div className="mt-6 border-t pt-5">
                        <SalesTimeline
                            salesOrderId={order.id}
                            idPrefix={`sales-overview-timeline-${toAutomationIdSegment(order.id)}`}
                            compact
                            onOpenFull={() => onSelectSection("timeline")}
                        />
                    </div>
                )}
            </ObjectSectionTabsPanel>

            <ObjectSectionTabsPanel value="timeline" className="py-5">
                {navSection === "timeline" && (
                    <SalesTimeline
                        salesOrderId={order.id}
                        idPrefix={`sales-detail-timeline-${toAutomationIdSegment(order.id)}`}
                    />
                )}
            </ObjectSectionTabsPanel>

            <ObjectSectionTabsPanel value="approval">
                <ApprovalPanel order={order} />
            </ObjectSectionTabsPanel>

            <ObjectSectionTabsPanel value="fulfillment">
                <PurchasePanel order={order} />
            </ObjectSectionTabsPanel>

            <ObjectSectionTabsPanel value="extra-cost">
                {navSection === "extra-cost" ? (
                    <ExtraCostPanel order={order} />
                ) : null}
            </ObjectSectionTabsPanel>

            <ObjectSectionTabsPanel value="acceptance">
                <AcceptancePanel order={order} />
            </ObjectSectionTabsPanel>

            <ObjectSectionTabsPanel value="receivable">
                <SalesOrderCustomerFinancePanel
                    key={order.id}
                    order={order}
                    section="receivable"
                />
            </ObjectSectionTabsPanel>

            <ObjectSectionTabsPanel value="receipt">
                <SalesOrderCustomerFinancePanel
                    key={order.id}
                    order={order}
                    section="receipt"
                />
            </ObjectSectionTabsPanel>
            <ObjectSectionTabsPanel value="invoice">
                <SalesOrderCustomerFinancePanel
                    key={order.id}
                    order={order}
                    section="invoice"
                />
            </ObjectSectionTabsPanel>

            <ObjectSectionTabsPanel value="documents">
                <SalesOrderDocumentsPanel order={order} />
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
