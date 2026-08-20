"use client"

import Link from "next/link"

import { surfacePanelClassName } from "@/components/business"
import { Button } from "@/components/ui/button"
import { cn } from "@/lib/utils"

import { PurchaseOrderDetailAuditSection } from "@/features/purchase-orders/components/purchase-order-detail-audit-section"
import { PurchaseOrderDetailChangesSection } from "@/features/purchase-orders/components/purchase-order-detail-changes-section"
import { PurchaseOrderDetailFulfillmentSection } from "@/features/purchase-orders/components/purchase-order-detail-fulfillment-section"
import { PurchaseOrderDetailLinesSection } from "@/features/purchase-orders/components/purchase-order-detail-lines-section"
import {
    PurchaseOrderDetailOverviewSection,
    PurchaseOrderDetailSummarySection,
} from "@/features/purchase-orders/components/purchase-order-detail-overview-section"
import { PurchaseOrderDetailPayableSection } from "@/features/purchase-orders/components/purchase-order-detail-payable-section"
import type { PurchaseOrderDetailResult } from "@/features/purchase-orders/hooks/use-purchase-order-detail-command-state"
import type {
    PurchaseOrderDetailMode,
    PurchaseOrderDetailNavItem,
    PurchaseOrderDetailSectionId,
} from "@/features/purchase-orders/pages/purchase-order-detail-helpers"
import type { PurchaseOrderCenterView } from "@/features/purchase-orders/types"

type GateView = PurchaseOrderCenterView["progress"]["prepaymentGate"]
type ActionBlocker =
    PurchaseOrderCenterView["actionBlockers"][number] | undefined

/**
 * 采购单详情子区。变更页签承接 PurchaseChangeOrder 通用审批区。
 * PurchaseReturnOrder 为 NO_APPROVAL，关联采购退货不接入审批区。
 */
export function PurchaseOrderDetailSections({
    order,
    activeSection,
    mode,
    navItems,
    costMasked,
    gate,
    canPay,
    canFulfill,
    fulfillBlocker,
    canChange,
    changeBlocker,
    baseHref,
    w12PayHref,
    onRequestChange,
    changeWorkItemId,
    changeExpectedTaskVersion,
    changeWorkItemAllowedActions,
    onChangeApprovalResult,
}: {
    order: PurchaseOrderCenterView
    activeSection: PurchaseOrderDetailSectionId
    mode: PurchaseOrderDetailMode
    navItems: PurchaseOrderDetailNavItem[]
    costMasked: boolean
    gate: GateView
    canPay: boolean
    canFulfill: boolean
    fulfillBlocker: ActionBlocker
    canChange: boolean
    changeBlocker: ActionBlocker
    baseHref: string
    w12PayHref: string
    onRequestChange: () => void
    changeWorkItemId?: string
    changeExpectedTaskVersion?: string
    changeWorkItemAllowedActions?: readonly string[]
    onChangeApprovalResult?: (result: PurchaseOrderDetailResult) => void
}) {
    return (
        <div className={cn(surfacePanelClassName, "min-w-0 overflow-hidden")}>
            <nav
                className="flex flex-wrap gap-1 border-b border-grid px-3 py-1.5"
                aria-label="详情子区"
            >
                {navItems.map((item) => (
                    <Button
                        key={item.id}
                        type="button"
                        size="sm"
                        variant={
                            activeSection === item.id ? "secondary" : "ghost"
                        }
                        render={<Link href={item.href} />}
                    >
                        {item.label}
                    </Button>
                ))}
            </nav>

            <div className="space-y-4 px-3 py-4 md:px-4">
                {mode === "view" || activeSection !== "overview" ? (
                    <div className="grid gap-4">
                        {activeSection === "overview" && mode === "view" ? (
                            <PurchaseOrderDetailOverviewSection
                                order={order}
                                costMasked={costMasked}
                                gate={gate}
                                canPay={canPay}
                                w12PayHref={w12PayHref}
                            />
                        ) : null}

                        {activeSection === "lines" ? (
                            <PurchaseOrderDetailLinesSection
                                order={order}
                                costMasked={costMasked}
                            />
                        ) : null}

                        {activeSection === "fulfillment" ? (
                            <PurchaseOrderDetailFulfillmentSection
                                order={order}
                                costMasked={costMasked}
                                gate={gate}
                                canFulfill={canFulfill}
                                fulfillBlocker={fulfillBlocker}
                                baseHref={baseHref}
                                w12PayHref={w12PayHref}
                            />
                        ) : null}

                        {activeSection === "payable" ? (
                            <PurchaseOrderDetailPayableSection
                                order={order}
                                costMasked={costMasked}
                                canPay={canPay}
                                w12PayHref={w12PayHref}
                            />
                        ) : null}

                        {activeSection === "changes" ? (
                            <PurchaseOrderDetailChangesSection
                                order={order}
                                canChange={canChange}
                                changeBlocker={changeBlocker}
                                onRequestChange={onRequestChange}
                                workItemId={changeWorkItemId}
                                expectedTaskVersion={changeExpectedTaskVersion}
                                workItemAllowedActions={
                                    changeWorkItemAllowedActions
                                }
                                onApprovalResult={onChangeApprovalResult}
                            />
                        ) : null}

                        {activeSection === "audit" ? (
                            <PurchaseOrderDetailAuditSection order={order} />
                        ) : null}

                        {activeSection === "overview" && mode === "view" ? (
                            <PurchaseOrderDetailSummarySection
                                order={order}
                                costMasked={costMasked}
                            />
                        ) : null}
                    </div>
                ) : null}
            </div>
        </div>
    )
}
