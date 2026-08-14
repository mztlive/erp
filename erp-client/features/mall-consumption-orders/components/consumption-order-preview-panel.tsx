"use client"

import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { ScrollArea } from "@/components/ui/scroll-area"
import { Separator } from "@/components/ui/separator"
import type { MallConsumptionOrderView } from "@/features/mall-consumption-orders/types"
import { AmountIdentitySection } from "./preview/amount-identity-section"
import { CostSection } from "./preview/cost-section"
import { FactsSection } from "./preview/facts-section"
import { FulfillmentSection } from "./preview/fulfillment-section"
import { PaymentSourcesSection } from "./preview/payment-sources-section"
import { SupplierSection } from "./preview/supplier-section"

type Props = {
    view: MallConsumptionOrderView
}

/**
 * W25 detail 半屏：身份、金额、关键事实、支付构成、履约链、供应商摘要与成本口径。
 * 字段与对象中心概览保持一致，仅作只读摘要展示。
 */
export function ConsumptionOrderPreviewPanel({ view }: Props) {
    return (
        <div
            data-slot="consumption-order-detail-preview"
            className="flex min-h-0 flex-1 flex-col"
        >
            <ScrollArea className="min-h-0 flex-1">
                <div className="space-y-4 p-4 md:p-5">
                    {view.paymentOccurredAlert ? (
                        <Alert
                            variant={
                                view.paymentOccurredAlert.severity ===
                                "destructive"
                                    ? "destructive"
                                    : "warning"
                            }
                            role="alert"
                            className="py-3"
                        >
                            <AlertTitle className="text-sm">
                                {view.paymentOccurredAlert.title}
                            </AlertTitle>
                            <AlertDescription className="text-xs leading-relaxed">
                                {view.paymentOccurredAlert.message}
                            </AlertDescription>
                        </Alert>
                    ) : null}

                    <AmountIdentitySection view={view} />

                    <Separator />

                    <FactsSection facts={view.facts} />

                    <Separator />

                    <PaymentSourcesSection view={view} />

                    <Separator />

                    <FulfillmentSection view={view} />

                    <Separator />

                    <SupplierSection view={view} />

                    <Separator />

                    <CostSection view={view} />
                </div>
            </ScrollArea>
        </div>
    )
}
