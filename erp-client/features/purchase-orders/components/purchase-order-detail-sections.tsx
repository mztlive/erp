"use client"

import * as React from "react"

import { DetailWorkspaceTabs } from "@/components/business"
import { PurchaseOrderDetailApprovalSection } from "@/features/purchase-orders/components/purchase-order-detail-approval-section"
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
    const items = PURCHASE_ORDER_DETAIL_NAV.map((item) => ({
        ...item,
        badge:
            item.id === "approval" && approvalPending ? (
                <span className="rounded-sm bg-info-soft px-1.5 py-0.5 text-tiny font-medium text-info-soft-foreground">
                    进行中
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
    hasHeaderPrimaryAction = false,
    onChangeApprovalResult,
}: {
    order: PurchaseOrderCenterView
    activeSection: PurchaseOrderDetailSectionId
    editor?: React.ReactNode
    costMasked: boolean
    hasHeaderPrimaryAction?: boolean
    onChangeApprovalResult?: (result: PurchaseOrderDetailResult) => void
}) {
    if (editor) return editor

    switch (activeSection) {
        case "overview":
            return (
                <PurchaseOrderDetailOverviewSection
                    order={order}
                    costMasked={costMasked}
                />
            )
        case "approval":
            return (
                <PurchaseOrderDetailApprovalSection
                    approval={order.approval}
                    idPrefix="purchase-order-approval"
                />
            )
        case "fulfillment":
            return (
                <PurchaseOrderDetailFulfillmentSection
                    order={order}
                    costMasked={costMasked}
                />
            )
        case "payable":
            return (
                <PurchaseOrderDetailPayableSection
                    order={order}
                    costMasked={costMasked}
                    hasHeaderPrimaryAction={hasHeaderPrimaryAction}
                />
            )
        case "changes":
            return (
                <PurchaseOrderDetailChangesSection
                    order={order}
                    onApprovalResult={onChangeApprovalResult}
                />
            )
    }
}
