"use client"

import type { ReactNode } from "react"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import {
    ObjectSectionTabs,
    ObjectSectionTabsPanel,
} from "@/components/business"
import { SalesTimeline } from "@/features/document-timeline/components/sales-timeline"
import { toAutomationIdSegment } from "@/lib/automation-id"
import type { SalesOrderDetailView } from "../api/sales-orders"
import { ApprovalPanel } from "./sales-order-detail-approval-panel"
import {
    AcceptancePanel,
    CollaborationPanel,
    OverviewPanel,
    PurchasePanel,
    VersionsPanel,
} from "./sales-order-detail-panels"
import { AcceptanceWorkspace } from "./acceptance-workspace"
import { SalesOrderCustomerFinancePanel } from "./sales-order-detail-receivable-panel"
import { SalesOrderDocumentsPanel } from "./sales-order-documents-panel"
import { ExtraCostPanel } from "./sales-order-extra-cost-panel"
import {
    SalesOrderDeliveryOverview,
    type DeliveryProgressQuery,
} from "./sales-order-delivery-overview"
import { getProcurementAttention } from "../lib/procurement-attention"
import {
    SALES_ORDER_SECTION_GROUPS,
    salesOrderSectionGroup,
} from "../lib/sales-order-detail-navigation"
import {
    isSalesOrderApprovalInProgress,
    isWorkSection,
    type NavSectionId,
    type SalesOrderDetailActionResult,
    type WorkSectionId,
} from "../lib/sales-order-detail-model"

/** 主分区按销售任务组织，原 URL 继续定位到各业务面。 */
export function SalesOrderDetailTabs({
    order,
    section,
    navSection,
    visibleNav,
    canAccept,
    canOperateAcceptance,
    canReadDelivery,
    deliveryQuery,
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
    canOperateAcceptance: boolean
    canReadDelivery: boolean
    deliveryQuery: DeliveryProgressQuery
    onSelectSection: (
        next: NavSectionId | WorkSectionId,
        extras?: { mode?: "register" },
    ) => void
    onApprovalResult: (result: SalesOrderDetailActionResult) => void
    sidebar?: ReactNode
    beforeContent?: ReactNode
}) {
    const group = salesOrderSectionGroup(navSection)
    const isCard = order.nature === "card_voucher"
    const purchaseNeedsAttention =
        !isCard &&
        order.currentRevisionNo != null &&
        order.primaryStatus.code !== "voided" &&
        getProcurementAttention(order.related.procurementProgress)
            .needsAttention
    const items = SALES_ORDER_SECTION_GROUPS.filter((item) =>
        visibleNav.some((nav) => salesOrderSectionGroup(nav.id) === item.id),
    ).map((item) => {
        const warning = item.id === "fulfillment" && purchaseNeedsAttention
        const changing =
            item.id === "versions" && Boolean(order.activeChangeOrder)
        const pending = item.id === "fulfillment" && canAccept
        const approving =
            item.id === "timeline" && isSalesOrderApprovalInProgress(order)
        return {
            ...item,
            label:
                isCard && item.id === "fulfillment"
                    ? "履约"
                    : isCard && item.id === "versions"
                      ? "历史版本"
                      : item.label,
            badge:
                warning || changing || pending || approving ? (
                    <Badge
                        variant={warning || changing ? "warning" : "info"}
                        className="h-5 px-1.5 text-2xs font-normal"
                    >
                        {warning
                            ? "异常"
                            : changing
                              ? "改单中"
                              : pending
                                ? "待验收"
                                : "审批中"}
                    </Badge>
                ) : undefined,
        }
    })
    const subitems = visibleNav.filter(
        (item) =>
            salesOrderSectionGroup(item.id) === group &&
            !(isCard && item.id === "collaboration"),
    )
    const prefix = toAutomationIdSegment(order.id)
    return (
        <ObjectSectionTabs
            id={`sales-orders-detail-tabs-${order.id}`}
            value={group}
            onValueChange={(value) => {
                const next = value as NavSectionId
                if (next !== group || isWorkSection(section))
                    onSelectSection(next)
            }}
            items={items}
            listLabel="销售单分区"
            listClassName="border-border/70"
            sidebar={sidebar}
            scrollMode="panels"
            scrollResetKey={navSection}
            beforeContent={beforeContent}
        >
            <ObjectSectionTabsPanel value={group}>
                {subitems.length > 1 ? (
                    <nav
                        aria-label="当前分区内容"
                        className="flex flex-wrap gap-2"
                    >
                        {subitems.map((item) => (
                            <Button
                                key={item.id}
                                id={`sales-order-section-${item.id}`}
                                variant={
                                    navSection === item.id
                                        ? "secondary"
                                        : "ghost"
                                }
                                size="sm"
                                aria-pressed={navSection === item.id}
                                title={item.hint}
                                onClick={() => onSelectSection(item.id)}
                            >
                                {item.id === "fulfillment"
                                    ? "采购与交付"
                                    : item.id === "acceptance"
                                      ? "客户验收"
                                      : item.label}
                            </Button>
                        ))}
                    </nav>
                ) : null}
                {navSection === "overview" ? (
                    <>
                        <div className="min-w-0 space-y-6 rounded-lg border border-border/70 p-4 md:p-5">
                            {isCard ? (
                                <OverviewPanel order={order} />
                            ) : (
                                <SalesOrderDeliveryOverview
                                    order={order}
                                    query={deliveryQuery}
                                    canRead={
                                        canReadDelivery &&
                                        order.currentRevisionNo != null
                                    }
                                />
                            )}
                            <div className="border-t border-border/70 pt-5">
                                <SalesTimeline
                                    salesOrderId={order.id}
                                    idPrefix={`sales-overview-timeline-${prefix}`}
                                    compact
                                    onOpenFull={() =>
                                        onSelectSection("timeline")
                                    }
                                />
                            </div>
                        </div>
                        {!isCard ? (
                            <details className="group rounded-lg border border-border/70 p-4 md:p-5">
                                <summary
                                    id="sales-order-commercial-details-toggle"
                                    className="cursor-pointer text-sm font-medium"
                                >
                                    销售明细与交易约定
                                </summary>
                                <div className="mt-5">
                                    <OverviewPanel order={order} />
                                </div>
                            </details>
                        ) : null}
                    </>
                ) : null}
                {navSection === "timeline" ? (
                    <SalesTimeline
                        salesOrderId={order.id}
                        idPrefix={`sales-detail-timeline-${prefix}`}
                    />
                ) : null}
                {navSection === "approval" ? (
                    <ApprovalPanel order={order} />
                ) : null}
                {navSection === "fulfillment" ? (
                    isCard ? (
                        <CollaborationPanel order={order} />
                    ) : (
                        <PurchasePanel order={order} />
                    )
                ) : null}
                {navSection === "collaboration" ? (
                    <CollaborationPanel order={order} />
                ) : null}
                {navSection === "extra-cost" ? (
                    <ExtraCostPanel order={order} />
                ) : null}
                {navSection === "acceptance" ? (
                    canReadDelivery ? (
                        canOperateAcceptance &&
                        order.allowedActions.includes("REGISTER_ACCEPTANCE") ? (
                            <AcceptanceWorkspace
                                key={order.id}
                                salesOrderId={order.id}
                                ownerUserId={order.ownerUserId}
                                ownerName={order.ownerName}
                                onPosted={({
                                    acceptanceNo,
                                    remainingEligibleCount,
                                }) =>
                                    onApprovalResult({
                                        status: "succeeded",
                                        title: "客户验收已登记",
                                        description:
                                            remainingEligibleCount > 0
                                                ? `还有 ${remainingEligibleCount} 批交付待验收。`
                                                : "当前交付批次已全部验收，交付进度已更新。",
                                        reference: acceptanceNo,
                                    })
                                }
                            />
                        ) : (
                            <AcceptancePanel order={order} />
                        )
                    ) : (
                        <p className="text-sm text-muted-foreground">
                            当前无法读取客户验收资料，请核对账号权限。
                        </p>
                    )
                ) : null}
                {navSection === "receivable" ||
                navSection === "receipt" ||
                navSection === "invoice" ? (
                    <SalesOrderCustomerFinancePanel
                        key={order.id}
                        order={order}
                        section={navSection}
                    />
                ) : null}
                {navSection === "documents" ? (
                    <SalesOrderDocumentsPanel order={order} />
                ) : null}
                {navSection === "versions" ? (
                    <VersionsPanel
                        showActiveChange={section !== "change-review"}
                        order={order}
                        onApprovalResult={onApprovalResult}
                    />
                ) : null}
                {navSection === "returns" ? (
                    <SalesTimeline
                        key={`returns-${order.id}`}
                        salesOrderId={order.id}
                        idPrefix={`sales-returns-timeline-${prefix}`}
                        fixedCategory="returns"
                    />
                ) : null}
            </ObjectSectionTabsPanel>
        </ObjectSectionTabs>
    )
}
