"use client"

import { BusinessStatusBadge } from "@/components/business"
import { Badge } from "@/components/ui/badge"
import type { MallConsumptionOrderView } from "@/features/mall-consumption-orders/types"
import {
    ATTRIBUTION_STATUS_LABEL,
    ATTRIBUTION_STATUS_TONE,
} from "@/features/mall-consumption-orders/types"
import { SectionTitle } from "./section-title"

type Props = {
    view: MallConsumptionOrderView
}

export function PaymentSourcesSection({ view }: Props) {
    return (
        <section className="space-y-2" aria-label="支付构成">
            <SectionTitle>支付构成</SectionTitle>
            {view.paymentSources.length === 0 ? (
                <p className="text-xs text-muted-foreground">暂无支付来源</p>
            ) : (
                <ul className="space-y-1.5">
                    {view.paymentSources.map((s) => (
                        <li
                            key={s.paymentSourceId}
                            className="flex flex-wrap items-center gap-x-3 gap-y-1 text-xs"
                        >
                            <Badge variant="secondary">
                                {s.sourceType === "CARD" ? "卡券" : "微信"} ¥
                                {s.amount}
                                <span className="num ml-1">
                                    {s.sourceReference}
                                </span>
                                {s.sourceType === "CARD" ? " · 非卡号" : ""}
                            </Badge>
                            <BusinessStatusBadge
                                context="list"
                                label={
                                    ATTRIBUTION_STATUS_LABEL[
                                        s.attributionStatus
                                    ]
                                }
                                tone={
                                    ATTRIBUTION_STATUS_TONE[s.attributionStatus]
                                }
                            />
                        </li>
                    ))}
                </ul>
            )}
            <p className="text-tiny text-muted-foreground">
                金额核对：{" "}
                {view.conservation.orderTotal.valid ? "有效" : "差异"} ·
                含税实付{" "}
                <span className="num">
                    {view.conservation.orderTotal.actual}
                </span>
            </p>
        </section>
    )
}
