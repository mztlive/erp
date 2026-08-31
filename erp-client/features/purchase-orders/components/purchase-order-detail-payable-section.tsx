"use client"

import Link from "next/link"

import { DocumentSection, MoneyValue } from "@/components/business"
import {
    DescriptionDetails,
    DescriptionItem,
    DescriptionList,
    DescriptionTerm,
} from "@/components/ui/description-list"
import { Button } from "@/components/ui/button"

import type { PurchaseOrderCenterView } from "@/features/purchase-orders/types"

export function PurchaseOrderDetailPayableSection({
    order,
    costMasked,
    canPay,
    w12PayHref,
}: {
    order: PurchaseOrderCenterView
    costMasked: boolean
    canPay: boolean
    w12PayHref: string
}) {
    return (
        <DocumentSection title="应付与票款">
            {order.payableSummary ? (
                <DescriptionList columns="three">
                    <DescriptionItem>
                        <DescriptionTerm>应付未结</DescriptionTerm>
                        <DescriptionDetails>
                            {costMasked ? (
                                "•••"
                            ) : (
                                <MoneyValue
                                    value={
                                        order.payableSummary.payableOpenAmount
                                    }
                                />
                            )}
                        </DescriptionDetails>
                    </DescriptionItem>
                    <DescriptionItem>
                        <DescriptionTerm>已付并核销</DescriptionTerm>
                        <DescriptionDetails>
                            {costMasked ? (
                                "•••"
                            ) : (
                                <MoneyValue
                                    value={
                                        order.payableSummary.paidAllocatedAmount
                                    }
                                />
                            )}
                        </DescriptionDetails>
                    </DescriptionItem>
                    <DescriptionItem>
                        <DescriptionTerm>已收票并核销</DescriptionTerm>
                        <DescriptionDetails>
                            {costMasked ? (
                                "•••"
                            ) : (
                                <MoneyValue
                                    value={
                                        order.payableSummary
                                            .purchaseInvoiceAllocatedAmount
                                    }
                                />
                            )}
                        </DescriptionDetails>
                    </DescriptionItem>
                </DescriptionList>
            ) : (
                <p className="text-sm text-muted-foreground">
                    尚未形成应付（需审批通过）。
                </p>
            )}
            <div className="mt-4">
                <Button
                    type="button"
                    variant="outline"
                    disabled={!canPay}
                    render={
                        <Link
                            id={`procurement-orders-detail-payable-go-${order.identity.purchaseOrderId}`}
                            href={w12PayHref}
                        />
                    }
                >
                    去供应商往来
                </Button>
            </div>
        </DocumentSection>
    )
}
