"use client"

import { MoneyValue } from "@/components/business"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import type { MallConsumptionOrderView } from "@/features/mall-consumption-orders/types"
import {
    SUPPLIER_CANCEL_LABEL,
    SUPPLIER_REFUND_LABEL,
    SUPPLIER_STATUS_LABEL,
} from "@/features/mall-consumption-orders/types"
import { SectionTitle } from "./section-title"

type Props = {
    view: MallConsumptionOrderView
}

export function SupplierSection({ view }: Props) {
    return (
        <section className="space-y-2" aria-label="供应商摘要">
            <SectionTitle>供应商摘要</SectionTitle>
            {view.fulfillment.chain === "LEGACY_MANUAL" ? (
                <p className="text-xs text-muted-foreground">
                    原人工履约链 · 无供应商子订单
                </p>
            ) : view.supplierOrders.length === 0 ? (
                <Alert variant="warning" className="py-2">
                    <AlertTitle>未形成供应商子订单</AlertTitle>
                    <AlertDescription>
                        {view.fulfillment.autoFulfillmentBlocker ??
                            "自动履约条件不足或归集未完成；支付记录已保留。"}
                    </AlertDescription>
                </Alert>
            ) : (
                <ul className="space-y-2">
                    {view.supplierOrders.map((so) => (
                        <li
                            key={so.supplierFulfillmentOrderId}
                            className="rounded-lg border border-border bg-card px-3 py-2 text-xs"
                        >
                            <div className="flex flex-wrap items-center gap-x-2">
                                <span className="num font-medium">
                                    {so.fulfillmentOrderNo}
                                </span>
                                <span className="text-muted-foreground">
                                    {so.supplierLabel}
                                </span>
                            </div>
                            <div className="mt-0.5 flex flex-wrap gap-x-3 text-muted-foreground">
                                <span>
                                    履约{" "}
                                    {
                                        SUPPLIER_STATUS_LABEL[
                                            so.fulfillmentStatus
                                        ]
                                    }
                                </span>
                                <span>
                                    取消{" "}
                                    {SUPPLIER_CANCEL_LABEL[so.cancelStatus] ??
                                        so.cancelStatus}
                                </span>
                                <span>
                                    退款{" "}
                                    {SUPPLIER_REFUND_LABEL[so.refundStatus] ??
                                        so.refundStatus}
                                </span>
                            </div>
                            {so.supplierRefundSummary ? (
                                <div className="mt-1 flex flex-wrap gap-x-3 text-muted-foreground">
                                    <span>
                                        成本冲减{" "}
                                        <MoneyValue
                                            value={
                                                so.supplierRefundSummary
                                                    .costReductionGross
                                            }
                                        />
                                    </span>
                                    <span>
                                        应付冲减{" "}
                                        <MoneyValue
                                            value={
                                                so.supplierRefundSummary
                                                    .payableReductionGross
                                            }
                                        />
                                    </span>
                                    <span>
                                        现金退回{" "}
                                        <MoneyValue
                                            value={
                                                so.supplierRefundSummary
                                                    .cashRefundGross
                                            }
                                        />
                                    </span>
                                </div>
                            ) : null}
                        </li>
                    ))}
                </ul>
            )}
        </section>
    )
}
