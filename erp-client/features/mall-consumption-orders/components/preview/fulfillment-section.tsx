"use client"

import { BusinessStatusBadge } from "@/components/business"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Badge } from "@/components/ui/badge"
import type { MallConsumptionOrderView } from "@/features/mall-consumption-orders/types"
import {
    FULFILLMENT_CHAIN_LABEL,
    FULFILLMENT_CHAIN_TONE,
} from "@/features/mall-consumption-orders/types"
import { formatDateTime } from "@/lib/datetime"
import { SectionTitle } from "./section-title"

type Props = {
    view: MallConsumptionOrderView
}

export function FulfillmentSection({ view }: Props) {
    return (
        <section className="space-y-2" aria-label="履约链">
            <SectionTitle>履约链</SectionTitle>
            <div className="flex flex-wrap items-center gap-2">
                <BusinessStatusBadge
                    context="list"
                    label={FULFILLMENT_CHAIN_LABEL[view.fulfillment.chain]}
                    tone={FULFILLMENT_CHAIN_TONE[view.fulfillment.chain]}
                />
                <Badge variant="secondary">
                    支付成功时间{" "}
                    {formatDateTime(
                        view.fulfillment.decidedByOccurredAt,
                        "default",
                    )}
                    {view.fulfillment.chain === "LEGACY_MANUAL"
                        ? " · 早于切换时点"
                        : " · 不早于切换时点"}
                </Badge>
            </div>
            {view.fulfillment.chain === "LEGACY_MANUAL" ? (
                <Alert variant="default" className="py-2">
                    <AlertTitle>原人工履约链</AlertTitle>
                    <AlertDescription>
                        该支付发生在履约主责切换之前，仅作历史记录，不创建供应商子订单。
                    </AlertDescription>
                </Alert>
            ) : null}
            {view.fulfillment.autoFulfillmentBlocker ? (
                <Alert variant="warning" className="py-2">
                    <AlertTitle>自动履约条件不足</AlertTitle>
                    <AlertDescription>
                        {view.fulfillment.autoFulfillmentBlocker}
                    </AlertDescription>
                </Alert>
            ) : null}
        </section>
    )
}
