"use client"

import { MoneyValue } from "@/components/business"
import {
    DetailHint,
    DetailRecordSection,
    DetailSummary,
    DetailSummaryItem,
} from "@/components/business/detail-presentation"
import type { PurchaseOrderCenterView } from "@/features/purchase-orders/types"

/** 票款只展示本单已确认的应付、付款核销和收票金额。 */
export function PurchaseOrderDetailPayableSection({
    order,
    costMasked,
}: {
    order: PurchaseOrderCenterView
    costMasked: boolean
}) {
    const summary = order.payableSummary
    if (!summary) {
        return (
            <DetailRecordSection title="本单应付" compact>
                <p className="text-sm text-muted-foreground">尚未形成应付</p>
            </DetailRecordSection>
        )
    }
    const amount = (value: string) =>
        costMasked ? "•••" : <MoneyValue value={value} />
    return (
        <DetailSummary label="采购票款摘要">
            <DetailSummaryItem
                title="付款"
                label="待付金额"
                value={amount(summary.payableOpenAmount)}
                detail={<>已付并核销 {amount(summary.paidAllocatedAmount)}</>}
                hint={
                    <DetailHint
                        id="purchase-order-payment-hint"
                        label="付款核销"
                    >
                        仅统计已核销到本单的付款金额，审批中的付款不计入。
                    </DetailHint>
                }
            />
            <DetailSummaryItem
                title="收票"
                label="已收票并核销"
                value={amount(summary.purchaseInvoiceAllocatedAmount)}
                detail={order.progress.invoice}
                hint={
                    <DetailHint
                        id="purchase-order-invoice-hint"
                        label="收票核销"
                    >
                        仅统计已核销到本单的采购发票金额。
                    </DetailHint>
                }
            />
        </DetailSummary>
    )
}
