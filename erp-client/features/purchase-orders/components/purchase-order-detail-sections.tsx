"use client"

import * as React from "react"
import { useRouter, useSearchParams } from "next/navigation"

import {
    ObjectSectionTabs,
    ObjectSectionTabsPanel,
    surfacePanelClassName,
} from "@/components/business"
import { Badge } from "@/components/ui/badge"
import { cn } from "@/lib/utils"

import { PurchaseOrderDetailAuditSection } from "@/features/purchase-orders/components/purchase-order-detail-audit-section"
import { PurchaseOrderDetailChangesSection } from "@/features/purchase-orders/components/purchase-order-detail-changes-section"
import { PurchaseOrderDetailFulfillmentSection } from "@/features/purchase-orders/components/purchase-order-detail-fulfillment-section"
import {
    PurchaseOrderDetailOverviewSection,
    PurchaseOrderDetailSummarySection,
} from "@/features/purchase-orders/components/purchase-order-detail-overview-section"
import { PurchaseOrderDetailPayableSection } from "@/features/purchase-orders/components/purchase-order-detail-payable-section"
import type { PurchaseOrderDetailResult } from "@/features/purchase-orders/hooks/use-purchase-order-detail-command-state"
import { isPurchaseOrderApprovalInProgress } from "@/features/purchase-orders/lib/purchase-order-approval"
import {
    PURCHASE_ORDER_DETAIL_NAV,
    purchaseOrderSectionHref,
    resolvePurchaseOrderDetailSection,
    type PurchaseOrderDetailMode,
    type PurchaseOrderDetailSectionId,
} from "@/features/purchase-orders/pages/purchase-order-detail-helpers"
import type { PurchaseOrderCenterView } from "@/features/purchase-orders/types"

type GateView = PurchaseOrderCenterView["progress"]["prepaymentGate"]
type ActionBlocker =
    | PurchaseOrderCenterView["actionBlockers"][number]
    | undefined

/**
 * 采购单详情子区。变更页签承接 PurchaseChangeOrder 通用审批区。
 * PurchaseReturnOrder 为 NO_APPROVAL，关联采购退货不接入审批区。
 */
export function PurchaseOrderDetailSections({
    order,
    activeSection,
    mode,
    costMasked,
    gate,
    canPay,
    canFulfill,
    fulfillBlocker,
    canChange,
    changeBlocker,
    w12PayHref,
    onRequestChange,
    changeWorkItemId,
    changeExpectedTaskVersion,
    changeWorkItemAllowedActions,
    onChangeApprovalResult,
    approvalPanel,
}: {
    order: PurchaseOrderCenterView
    activeSection: PurchaseOrderDetailSectionId
    mode: PurchaseOrderDetailMode
    costMasked: boolean
    gate: GateView
    canPay: boolean
    canFulfill: boolean
    fulfillBlocker: ActionBlocker
    canChange: boolean
    changeBlocker: ActionBlocker
    w12PayHref: string
    onRequestChange: () => void
    changeWorkItemId?: string
    changeExpectedTaskVersion?: string
    changeWorkItemAllowedActions?: readonly string[]
    onChangeApprovalResult?: (result: PurchaseOrderDetailResult) => void
    approvalPanel: React.ReactNode
}) {
    const router = useRouter()
    const searchParams = useSearchParams()

    const handleSectionChange = React.useCallback(
        (next: string) => {
            router.replace(
                purchaseOrderSectionHref(
                    order.identity.purchaseOrderId,
                    resolvePurchaseOrderDetailSection(next),
                    searchParams,
                ),
                { scroll: false },
            )
        },
        [order.identity.purchaseOrderId, router, searchParams],
    )

    const approvalPending = isPurchaseOrderApprovalInProgress(order)
    const items = PURCHASE_ORDER_DETAIL_NAV.map((item) => ({
        ...item,
        badge:
            item.id === "approval" && approvalPending ? (
                <Badge
                    variant="info"
                    className="h-5 px-1.5 text-2xs font-normal"
                >
                    进行中
                </Badge>
            ) : undefined,
    }))

    return (
        <div className={cn(surfacePanelClassName, "min-w-0 overflow-hidden")}>
            <ObjectSectionTabs
                value={activeSection}
                onValueChange={handleSectionChange}
                items={items}
                listLabel="采购单分区"
            >
                <ObjectSectionTabsPanel value="overview">
                    {mode === "view" ? (
                        <>
                            <PurchaseOrderDetailOverviewSection
                                order={order}
                                costMasked={costMasked}
                                gate={gate}
                                canPay={canPay}
                                w12PayHref={w12PayHref}
                            />
                            <PurchaseOrderDetailSummarySection
                                order={order}
                                costMasked={costMasked}
                            />
                        </>
                    ) : null}
                </ObjectSectionTabsPanel>

                <ObjectSectionTabsPanel value="approval">
                    {approvalPanel}
                </ObjectSectionTabsPanel>

                <ObjectSectionTabsPanel value="fulfillment">
                    <PurchaseOrderDetailFulfillmentSection
                        order={order}
                        costMasked={costMasked}
                        gate={gate}
                        canFulfill={canFulfill}
                        fulfillBlocker={fulfillBlocker}
                        w12PayHref={w12PayHref}
                    />
                </ObjectSectionTabsPanel>

                <ObjectSectionTabsPanel value="payable">
                    <PurchaseOrderDetailPayableSection
                        order={order}
                        costMasked={costMasked}
                        canPay={canPay}
                        w12PayHref={w12PayHref}
                    />
                </ObjectSectionTabsPanel>

                <ObjectSectionTabsPanel value="changes">
                    <PurchaseOrderDetailChangesSection
                        order={order}
                        canChange={canChange}
                        changeBlocker={changeBlocker}
                        onRequestChange={onRequestChange}
                        workItemId={changeWorkItemId}
                        expectedTaskVersion={changeExpectedTaskVersion}
                        workItemAllowedActions={changeWorkItemAllowedActions}
                        onApprovalResult={onChangeApprovalResult}
                    />
                </ObjectSectionTabsPanel>

                <ObjectSectionTabsPanel value="audit">
                    <PurchaseOrderDetailAuditSection order={order} />
                </ObjectSectionTabsPanel>
            </ObjectSectionTabs>
        </div>
    )
}
