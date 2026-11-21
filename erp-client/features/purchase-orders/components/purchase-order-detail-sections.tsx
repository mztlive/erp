"use client"

import * as React from "react"

import { DetailWorkspaceTabs } from "@/components/business"
import { PurchaseOrderDetailAuditSection } from "@/features/purchase-orders/components/purchase-order-detail-audit-section"
import { PurchaseOrderDetailChangesSection } from "@/features/purchase-orders/components/purchase-order-detail-changes-section"
import { PurchaseOrderDetailFulfillmentSection } from "@/features/purchase-orders/components/purchase-order-detail-fulfillment-section"
import { PurchaseOrderDetailOverviewSection } from "@/features/purchase-orders/components/purchase-order-detail-overview-section"
import { PurchaseOrderDetailPayableSection } from "@/features/purchase-orders/components/purchase-order-detail-payable-section"
import type { PurchaseOrderDetailResult } from "@/features/purchase-orders/hooks/use-purchase-order-detail-command-state"
import { isPurchaseOrderApprovalInProgress } from "@/features/purchase-orders/lib/purchase-order-approval"
import {
    PURCHASE_ORDER_DETAIL_NAV,
    resolvePurchaseOrderDetailSection,
    type PurchaseOrderDetailSectionId,
} from "@/features/purchase-orders/pages/purchase-order-detail-helpers"
import type { PurchaseOrderCenterView } from "@/features/purchase-orders/types"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { PurchaseOrderDetailSection } from "./purchase-order-detail-section"

type GateView = PurchaseOrderCenterView["progress"]["prepaymentGate"]

/** 置于 DetailWorkspaceChrome 底部的一体化通栏分区导航。 */
export function PurchaseOrderDetailSectionTabs({
    order,
    activeSection,
    onSelectSection,
}: {
    order: PurchaseOrderCenterView
    activeSection: PurchaseOrderDetailSectionId
    onSelectSection: (next: PurchaseOrderDetailSectionId) => void
}) {
    const approvalPending = isPurchaseOrderApprovalInProgress(order)
    const hasActiveChange = Boolean(order.activeChangeOrder)
    const items = PURCHASE_ORDER_DETAIL_NAV.map((item) => ({
        ...item,
        badge:
            item.id === "approval" && approvalPending ? (
                <span className="rounded-sm bg-info-soft px-1.5 py-0.5 text-tiny leading-3.5 font-medium text-info-soft-foreground">
                    进行中
                </span>
            ) : item.id === "changes" && hasActiveChange ? (
                <span className="rounded-sm bg-warning-soft px-1.5 py-0.5 text-tiny leading-3.5 font-medium text-warning-soft-foreground">
                    改单中
                </span>
            ) : undefined,
    }))

    return (
        <DetailWorkspaceTabs
            value={activeSection}
            onValueChange={(next) =>
                onSelectSection(resolvePurchaseOrderDetailSection(next))
            }
            items={items}
            ariaLabel="采购单分区"
            tabIdPrefix={`procurement-orders-detail-tabs-${toAutomationIdSegment(order.identity.purchaseOrderId)}-tab`}
        />
    )
}

/**
 * 采购单详情子区内容。变更页签承接 PurchaseChangeOrder 通用审批区。
 * PurchaseReturnOrder 为 NO_APPROVAL，关联采购退货不接入审批区。
 */
export function PurchaseOrderDetailSectionContent({
    order,
    activeSection,
    editor,
    costMasked,
    gate,
    changeWorkItemId,
    changeExpectedTaskVersion,
    changeWorkItemAllowedActions,
    onChangeApprovalResult,
    approvalPanel,
}: {
    order: PurchaseOrderCenterView
    activeSection: PurchaseOrderDetailSectionId
    editor?: React.ReactNode
    costMasked: boolean
    gate: GateView
    changeWorkItemId?: string
    changeExpectedTaskVersion?: string
    changeWorkItemAllowedActions?: readonly string[]
    onChangeApprovalResult?: (result: PurchaseOrderDetailResult) => void
    approvalPanel: React.ReactNode
}) {
    switch (activeSection) {
        case "overview":
            return (
                editor ?? (
                    <PurchaseOrderDetailOverviewSection
                        order={order}
                        costMasked={costMasked}
                    />
                )
            )
        case "approval":
            return (
                <PurchaseOrderDetailSection width="form" title="审批进度与记录">
                    {approvalPanel}
                </PurchaseOrderDetailSection>
            )
        case "fulfillment":
            return (
                <PurchaseOrderDetailFulfillmentSection
                    order={order}
                    costMasked={costMasked}
                    gate={gate}
                />
            )
        case "payable":
            return (
                <PurchaseOrderDetailPayableSection
                    order={order}
                    costMasked={costMasked}
                />
            )
        case "changes":
            return (
                <PurchaseOrderDetailChangesSection
                    order={order}
                    workItemId={changeWorkItemId}
                    expectedTaskVersion={changeExpectedTaskVersion}
                    workItemAllowedActions={changeWorkItemAllowedActions}
                    onApprovalResult={onChangeApprovalResult}
                />
            )
        case "audit":
            return <PurchaseOrderDetailAuditSection order={order} />
    }
}
