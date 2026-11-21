"use client"

import {
    BusinessStatusBadge,
    DetailWorkspaceMetricItem,
    DetailWorkspaceMetrics,
    DetailWorkspaceSection,
    MoneyValue,
} from "@/components/business"
import {
    FULFILLMENT_RESPONSIBILITY_LABEL,
    type PurchaseOrderCenterView,
} from "@/features/purchase-orders/types"
import { PurchaseOrderApiExecutions } from "./purchase-order-api-executions"
import { PurchaseOrderDetailSection as DetailRecordSection } from "./purchase-order-detail-section"
import { PurchaseOrderExecutionFunds } from "./purchase-order-execution-funds"

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
        <>
            <DetailWorkspaceSection width="form">
                <DetailWorkspaceMetrics
                    columns={2}
                    variant="inline"
                    ariaLabel="采购履约摘要"
                >
                    <DetailWorkspaceMetricItem
                        label="履约进度"
                        value={
                            <span className="text-body-compact font-semibold">
                                {summary.progressLabel}
                            </span>
                        }
                        detail={
                            FULFILLMENT_RESPONSIBILITY_LABEL[
                                order.header.fulfillmentResponsibility
                            ]
                        }
                    />
                    <DetailWorkspaceMetricItem
                        label="预计交期"
                        value={
                            <span className="num text-body-compact font-semibold">
                                {order.header.expectedDate ?? "—"}
                            </span>
                        }
                        detail={order.header.supplierSnapshot}
                    />
                </DetailWorkspaceMetrics>
            </DetailWorkspaceSection>

            <PurchaseOrderApiExecutions order={order} costMasked={costMasked} />
            <PurchaseOrderExecutionFunds
                order={order}
                costMasked={costMasked}
            />

            {quantities.length > 0 ? (
                <DetailRecordSection width="form" title="履约数量">
                    <DetailWorkspaceMetrics
                        columns={quantities.length === 2 ? 2 : 3}
                        variant="inline"
                    >
                        {quantities.map((item) => (
                            <DetailWorkspaceMetricItem
                                key={item.label}
                                label={item.label}
                                value={item.value}
                            />
                        ))}
                    </DetailWorkspaceMetrics>
                </DetailRecordSection>
            ) : null}

            {summary.note ? (
                <DetailRecordSection width="form" title="履约说明">
                    <p className="text-xs text-muted-foreground">
                        {summary.note}
                    </p>
                </DetailRecordSection>
            ) : null}

            {gate.state !== "NOT_APPLICABLE" ? (
                <DetailRecordSection
                    width="form"
                    title="先款条件"
                    action={
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
                    }
                >
                    <DetailWorkspaceMetrics columns={3} variant="inline">
                        {[
                            { label: "需先款", value: gate.required },
                            { label: "已核销", value: gate.allocated },
                            { label: "还差", value: gate.gap },
                        ].map((item) => (
                            <DetailWorkspaceMetricItem
                                key={item.label}
                                label={item.label}
                                value={amount(item.value)}
                            />
                        ))}
                    </DetailWorkspaceMetrics>
                    <p className="text-xs text-muted-foreground">
                        {gate.message}
                    </p>
                </DetailRecordSection>
            ) : null}
        </>
    )
}
