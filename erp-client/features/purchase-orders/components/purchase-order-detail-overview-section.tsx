"use client"

import Link from "next/link"

import { DocumentSection, PrepaymentGate } from "@/components/business"
import {
    DescriptionDetails,
    DescriptionItem,
    DescriptionList,
    DescriptionTerm,
} from "@/components/ui/description-list"
import { Button } from "@/components/ui/button"

import { LinesTable } from "@/features/purchase-orders/components/purchase-order-surfaces"
import { PurchaseOrderDetailTotals } from "@/features/purchase-orders/components/purchase-order-detail-totals"
import type { PurchaseOrderCenterView } from "@/features/purchase-orders/types"

type GateView = PurchaseOrderCenterView["progress"]["prepaymentGate"]

export function PurchaseOrderDetailOverviewSection({
    order,
    costMasked,
    gate,
    canPay,
    w12PayHref,
}: {
    order: PurchaseOrderCenterView
    costMasked: boolean
    gate: GateView
    canPay: boolean
    w12PayHref: string
}) {
    return (
        <DocumentSection title="概览">
            <DescriptionList columns="three">
                <DescriptionItem>
                    <DescriptionTerm>供应商</DescriptionTerm>
                    <DescriptionDetails>
                        {order.header.supplierSnapshot}
                    </DescriptionDetails>
                </DescriptionItem>
                <DescriptionItem>
                    <DescriptionTerm>来源销售单</DescriptionTerm>
                    <DescriptionDetails>
                        <Link
                            href={`/sales/orders/${order.header.salesOrderId}`}
                            className="num text-primary underline-offset-2 hover:underline"
                        >
                            {order.header.salesOrderNo}
                        </Link>
                    </DescriptionDetails>
                </DescriptionItem>
                <DescriptionItem>
                    <DescriptionTerm>付款条件</DescriptionTerm>
                    <DescriptionDetails>
                        {order.header.paymentTermLabel}
                    </DescriptionDetails>
                </DescriptionItem>
                <DescriptionItem>
                    <DescriptionTerm>内容来源</DescriptionTerm>
                    <DescriptionDetails>
                        {order.currentContent.source === "DRAFT"
                            ? "草稿"
                            : order.currentContent.source === "SUBMISSION"
                              ? "已提交内容"
                              : "生效版本"}
                    </DescriptionDetails>
                </DescriptionItem>
                <DescriptionItem>
                    <DescriptionTerm>最近预计交期</DescriptionTerm>
                    <DescriptionDetails className="num">
                        {order.header.expectedDate ?? "—"}
                    </DescriptionDetails>
                </DescriptionItem>
                <DescriptionItem>
                    <DescriptionTerm>负责人</DescriptionTerm>
                    <DescriptionDetails>
                        {order.header.ownerName}
                    </DescriptionDetails>
                </DescriptionItem>
            </DescriptionList>
            {gate.state !== "NOT_APPLICABLE" ? (
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
                        allowed={gate.state === "SATISFIED"}
                        paymentAction={
                            canPay ? (
                                <Button
                                    type="button"
                                    size="sm"
                                    render={<Link href={w12PayHref} />}
                                >
                                    去供应商往来
                                </Button>
                            ) : undefined
                        }
                    />
                </div>
            ) : null}
            <PurchaseOrderDetailTotals
                className="mt-4 max-w-md"
                order={order}
                costMasked={costMasked}
            />
        </DocumentSection>
    )
}

export function PurchaseOrderDetailSummarySection({
    order,
    costMasked,
}: {
    order: PurchaseOrderCenterView
    costMasked: boolean
}) {
    return (
        <DocumentSection title="明细摘要">
            <LinesTable order={order} costMasked={costMasked} />
        </DocumentSection>
    )
}
