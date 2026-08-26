"use client"

import Link from "next/link"

import {
    BusinessStatusBadge,
    DocumentSection,
    PrepaymentGate,
} from "@/components/business"
import {
    DescriptionDetails,
    DescriptionItem,
    DescriptionList,
    DescriptionTerm,
} from "@/components/ui/description-list"
import { Button } from "@/components/ui/button"
import { fulfillmentTasksHref } from "@/features/workspace/lib/fulfillment-destination"

import type { PurchaseOrderCenterView } from "@/features/purchase-orders/types"

type GateView = PurchaseOrderCenterView["progress"]["prepaymentGate"]

export function PurchaseOrderDetailFulfillmentSection({
    order,
    costMasked,
    gate,
    canFulfill,
    fulfillBlocker,
    w12PayHref,
}: {
    order: PurchaseOrderCenterView
    costMasked: boolean
    gate: GateView
    canFulfill: boolean
    fulfillBlocker:
        | PurchaseOrderCenterView["actionBlockers"][number]
        | undefined
    w12PayHref: string
}) {
    return (
        <DocumentSection title="履约">
            <DescriptionList columns="three">
                <DescriptionItem>
                    <DescriptionTerm>进度</DescriptionTerm>
                    <DescriptionDetails>
                        <BusinessStatusBadge
                            context="detail"
                            label={order.fulfillmentSummary.progressLabel}
                            tone={order.fulfillmentSummary.progressTone}
                        />
                    </DescriptionDetails>
                </DescriptionItem>
                <DescriptionItem>
                    <DescriptionTerm>入库</DescriptionTerm>
                    <DescriptionDetails className="num">
                        {order.fulfillmentSummary.inboundQty}
                    </DescriptionDetails>
                </DescriptionItem>
                <DescriptionItem>
                    <DescriptionTerm>发货</DescriptionTerm>
                    <DescriptionDetails className="num">
                        {order.fulfillmentSummary.shippedQty}
                    </DescriptionDetails>
                </DescriptionItem>
                <DescriptionItem>
                    <DescriptionTerm>剩余</DescriptionTerm>
                    <DescriptionDetails className="num">
                        {order.fulfillmentSummary.remainingQty}
                    </DescriptionDetails>
                </DescriptionItem>
            </DescriptionList>
            {order.fulfillmentSummary.note ? (
                <p className="mt-2 text-sm text-muted-foreground">
                    {order.fulfillmentSummary.note}
                </p>
            ) : null}
            <div className="mt-4 flex flex-wrap gap-2">
                {canFulfill ? (
                    <Button
                        type="button"
                        render={
                            <Link
                                href={fulfillmentTasksHref(
                                    order.identity.purchaseOrderId,
                                )}
                            />
                        }
                    >
                        去交付与代发
                    </Button>
                ) : (
                    <div className="space-y-1">
                        <Button type="button" disabled>
                            履约入口未开放
                        </Button>
                        <p className="text-xs text-muted-foreground">
                            {fulfillBlocker?.message ??
                                "当前状态下不能进入交付，可先完成前置条件。"}
                        </p>
                    </div>
                )}
                {fulfillBlocker?.code === "PREPAYMENT_GATE" ? (
                    <Button
                        type="button"
                        variant="outline"
                        render={<Link href={w12PayHref} />}
                    >
                        去供应商往来
                    </Button>
                ) : null}
            </div>
            {gate.state === "BLOCKED" ? (
                <div className="mt-4">
                    <PrepaymentGate
                        condition={{
                            kind: "amount",
                            required: costMasked ? "•••" : gate.required,
                            description: gate.message,
                        }}
                        allocated={costMasked ? "•••" : gate.allocated}
                        gap={costMasked ? "•••" : gate.gap}
                        updatedAt={{
                            dateTime: gate.updatedAt,
                            label: gate.updatedAt,
                        }}
                        allowed={false}
                        paymentAction={
                            <Button
                                type="button"
                                size="sm"
                                render={<Link href={w12PayHref} />}
                            >
                                去供应商往来
                            </Button>
                        }
                    />
                </div>
            ) : null}
        </DocumentSection>
    )
}
