"use client"

import {
    DetailWorkspaceSection,
    DetailWorkspaceSubTabs,
    DetailWorkspaceTabs,
} from "@/components/business"
import { SalesTimeline } from "@/features/document-timeline/components/sales-timeline"
import { toAutomationIdSegment } from "@/lib/automation-id"
import type { SalesOrderDetailView } from "../api/sales-orders"
import { getProcurementAttention } from "../lib/procurement-attention"
import {
    isSalesOrderApprovalInProgress,
    isWorkSection,
    type NavSectionId,
    type SalesOrderDetailActionResult,
    type WorkSectionId,
} from "../lib/sales-order-detail-model"
import {
    SALES_ORDER_SECTION_GROUPS,
    salesOrderSectionGroup,
} from "../lib/sales-order-detail-navigation"
import { AcceptanceWorkspace } from "./acceptance-workspace"
import type { DeliveryProgressQuery } from "./sales-order-delivery-overview"
import {
    AcceptancePanel,
    CollaborationPanel,
    OverviewPanel,
    PurchasePanel,
    VersionsPanel,
} from "./sales-order-detail-panels"
import { SalesOrderCustomerFinancePanel } from "./sales-order-detail-receivable-panel"
import { SalesOrderDocumentsPanel } from "./sales-order-documents-panel"
import { ExtraCostPanel } from "./sales-order-extra-cost-panel"
import { SalesOrderProcessPanel } from "./sales-order-process-panel"

export type SalesOrderVisibleNavItem = {
    id: NavSectionId
    label: string
    hint: string
    show: boolean
}

/** 置于 DetailWorkspaceChrome 底部的一体化通栏分区导航。 */
export function SalesOrderDetailSectionTabs({
    order,
    section,
    navSection,
    visibleNav,
    canAccept,
    onSelectSection,
}: {
    order: SalesOrderDetailView
    section?: string
    navSection: NavSectionId
    visibleNav: SalesOrderVisibleNavItem[]
    canAccept: boolean
    onSelectSection: (
        next: NavSectionId | WorkSectionId,
        extras?: { mode?: "register" },
    ) => void
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
                warning || changing ? (
                    <span className="rounded-sm bg-warning-soft px-1.5 py-0.5 text-tiny leading-3.5 font-medium text-warning-soft-foreground">
                        {warning ? "异常" : "改单中"}
                    </span>
                ) : pending || approving ? (
                    <span className="rounded-sm bg-info-soft px-1.5 py-0.5 text-tiny leading-3.5 font-medium text-info-soft-foreground">
                        {pending ? "待验收" : "审批中"}
                    </span>
                ) : undefined,
        }
    })

    return (
        <DetailWorkspaceTabs
            value={group}
            onValueChange={(value) => {
                const next = value as NavSectionId
                if (next !== group || isWorkSection(section))
                    onSelectSection(next)
            }}
            items={items}
            ariaLabel="销售单分区"
            tabIdPrefix={`sales-orders-detail-tabs-${toAutomationIdSegment(order.id)}-tab`}
        />
    )
}

/** 销售单分区正文：二级柔底分段切换 + 无框水平细线分区内容。 */
export function SalesOrderDetailSectionContent({
    order,
    section,
    navSection,
    visibleNav,
    canAccept,
    canOperateAcceptance,
    canReadDelivery,
    canViewExtraCost,
    canAddExpensePurchase,
    deliveryQuery,
    onSelectSection,
    onApprovalResult,
}: {
    order: SalesOrderDetailView
    section?: string
    navSection: NavSectionId
    visibleNav: SalesOrderVisibleNavItem[]
    canAccept: boolean
    canOperateAcceptance: boolean
    canReadDelivery: boolean
    canViewExtraCost: boolean
    canAddExpensePurchase: boolean
    deliveryQuery: DeliveryProgressQuery
    onSelectSection: (
        next: NavSectionId | WorkSectionId,
        extras?: { mode?: "register" },
    ) => void
    onApprovalResult: (result: SalesOrderDetailActionResult) => void
}) {
    const group = salesOrderSectionGroup(navSection)
    const isCard = order.nature === "card_voucher"
    const subitems = visibleNav.filter(
        (item) =>
            salesOrderSectionGroup(item.id) === group &&
            !(isCard && item.id === "collaboration"),
    )
    const prefix = toAutomationIdSegment(order.id)
    const pendingAcceptanceCount =
        canReadDelivery && !deliveryQuery.isError
            ? (deliveryQuery.data?.pendingFactCount ?? 0)
            : 0
    const pendingFreightCount = canAddExpensePurchase
        ? (order.related.freightFollowUps?.length ?? 0)
        : 0

    return (
        <>
            {group !== "timeline" && subitems.length > 1 ? (
                <DetailWorkspaceSubTabs
                    value={navSection}
                    onValueChange={(next) =>
                        onSelectSection(next as NavSectionId)
                    }
                    ariaLabel="当前分区内容"
                    idPrefix="sales-order-section"
                    items={subitems.map((item) => ({
                        id: item.id,
                        title: item.hint,
                        label:
                            item.id === "fulfillment"
                                ? "销售明细与交付"
                                : item.id === "acceptance"
                                  ? "客户验收"
                                  : item.label,
                        badge:
                            item.id === "acceptance" &&
                            pendingAcceptanceCount > 0 ? (
                                <span
                                    className={
                                        canAccept
                                            ? "num rounded-sm bg-info-soft px-1.5 py-0.5 text-tiny leading-3.5 font-medium text-info-soft-foreground"
                                            : "num rounded-sm bg-muted px-1.5 py-0.5 text-tiny leading-3.5 font-medium text-muted-foreground"
                                    }
                                >
                                    待验 {pendingAcceptanceCount}
                                </span>
                            ) : item.id === "extra-cost" &&
                              pendingFreightCount > 0 ? (
                                <span className="rounded-sm bg-warning-soft px-1.5 py-0.5 text-tiny leading-3.5 font-medium text-warning-soft-foreground">
                                    运费待补
                                </span>
                            ) : undefined,
                    }))}
                />
            ) : null}

            {navSection === "overview" ? (
                <OverviewPanel
                    order={order}
                    related={
                        <DetailWorkspaceSection width="form">
                            <SalesTimeline
                                salesOrderId={order.id}
                                idPrefix={`sales-overview-timeline-${prefix}`}
                                compact
                                onOpenFull={() => onSelectSection("timeline")}
                                currentDocument={{
                                    type: "sales_order",
                                    id: order.id,
                                }}
                            />
                        </DetailWorkspaceSection>
                    }
                />
            ) : null}

            {group === "timeline" ? (
                <SalesOrderProcessPanel
                    deliveryQuery={deliveryQuery}
                    canReadDelivery={canReadDelivery}
                    visibleSections={visibleNav
                        .filter((item) => item.show)
                        .map((item) => item.id)}
                    key={order.id}
                    order={order}
                    section={navSection}
                    onSelectSection={onSelectSection}
                />
            ) : null}

            {navSection === "fulfillment" ? (
                isCard ? (
                    <CollaborationPanel order={order} />
                ) : (
                    <PurchasePanel
                        order={order}
                        deliveryQuery={deliveryQuery}
                        canReadDelivery={canReadDelivery}
                        canAccept={canAccept}
                        onStartAcceptance={() =>
                            onSelectSection("acceptance", { mode: "register" })
                        }
                        onOpenExtraCost={
                            canViewExtraCost || canAddExpensePurchase
                                ? () => onSelectSection("extra-cost")
                                : undefined
                        }
                    />
                )
            ) : null}

            {navSection === "collaboration" ? (
                <CollaborationPanel order={order} />
            ) : null}

            {navSection === "extra-cost" ? (
                <ExtraCostPanel
                    order={order}
                    canViewExtraCost={canViewExtraCost}
                    canAddExpensePurchase={canAddExpensePurchase}
                />
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
                    <DetailWorkspaceSection width="form">
                        <p className="text-xs text-muted-foreground">
                            当前无法读取客户验收资料，请核对账号权限。
                        </p>
                    </DetailWorkspaceSection>
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
                <DetailWorkspaceSection width="form">
                    <SalesTimeline
                        key={`returns-${order.id}`}
                        salesOrderId={order.id}
                        idPrefix={`sales-returns-timeline-${prefix}`}
                        fixedCategory="returns"
                        currentDocument={{ type: "sales_order", id: order.id }}
                    />
                </DetailWorkspaceSection>
            ) : null}
        </>
    )
}
