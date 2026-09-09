"use client"

import { BusinessStatusBadge, MoneyValue } from "@/components/business"
import {
    DetailRecordSection,
    DetailSummary,
    DetailSummaryItem,
} from "@/components/business/detail-presentation"
import {
    FULFILLMENT_RESPONSIBILITY_LABEL,
    type PurchaseOrderCenterView,
} from "@/features/purchase-orders/types"

/** 履约按业务进度展示，交付与付款在各自工作页面办理。 */
export function PurchaseOrderDetailFulfillmentSection({
    order,
    costMasked,
    gate,
}: {
    order: PurchaseOrderCenterView
    costMasked: boolean
    gate: PurchaseOrderCenterView["progress"]["prepaymentGate"]
}) {
    const summary = order.fulfillmentSummary
    const quantities = [
        { label: "已入库数量", value: summary.inboundQty },
        { label: "已发货数量", value: summary.shippedQty },
        { label: "剩余数量", value: summary.remainingQty },
    ].filter((item) => item.value !== "—" && item.value !== "")
    const amount = (value: string) =>
        costMasked ? "•••" : <MoneyValue value={value} />
    return (
        <div className="space-y-6">
            <DetailSummary label="采购履约摘要">
                <DetailSummaryItem
                    label="履约进度"
                    value={summary.progressLabel}
                    detail={
                        FULFILLMENT_RESPONSIBILITY_LABEL[
                            order.header.fulfillmentResponsibility
                        ]
                    }
                />
                <DetailSummaryItem
                    label="预计交期"
                    value={order.header.expectedDate ?? "—"}
                    detail={order.header.supplierSnapshot}
                />
            </DetailSummary>
            <div className="divide-y divide-border/70">
                {quantities.length > 0 ? (
                    <DetailRecordSection title="履约数量">
                        <dl className="grid grid-cols-1 gap-4 sm:grid-cols-3">
                            {quantities.map((item) => (
                                <div key={item.label}>
                                    <dt className="text-xs text-muted-foreground">
                                        {item.label}
                                    </dt>
                                    <dd className="num mt-1 text-lg font-medium">
                                        {item.value}
                                    </dd>
                                </div>
                            ))}
                        </dl>
                    </DetailRecordSection>
                ) : null}
                {summary.note ? (
                    <DetailRecordSection title="履约说明">
                        <p className="text-sm text-muted-foreground">
                            {summary.note}
                        </p>
                    </DetailRecordSection>
                ) : null}
                {gate.state !== "NOT_APPLICABLE" ? (
                    <DetailRecordSection title="先款条件">
                        <BusinessStatusBadge
                            context="detail"
                            label={
                                gate.state === "SATISFIED" ? "已满足" : "未满足"
                            }
                            tone={
                                gate.state === "SATISFIED"
                                    ? "success"
                                    : "warning"
                            }
                        />
                        <dl className="grid grid-cols-1 gap-4 sm:grid-cols-3">
                            {[
                                { label: "需先款", value: gate.required },
                                { label: "已核销", value: gate.allocated },
                                { label: "还差", value: gate.gap },
                            ].map((item) => (
                                <div key={item.label}>
                                    <dt className="text-xs text-muted-foreground">
                                        {item.label}
                                    </dt>
                                    <dd className="mt-1 text-lg font-medium">
                                        {amount(item.value)}
                                    </dd>
                                </div>
                            ))}
                        </dl>
                        <p className="text-xs leading-5 text-muted-foreground">
                            {gate.message}
                        </p>
                    </DetailRecordSection>
                ) : null}
            </div>
        </div>
    )
}
