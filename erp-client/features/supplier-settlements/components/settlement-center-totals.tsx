"use client"

import {
    DetailWorkspaceMetricItem,
    DetailWorkspaceMetrics,
    MoneyValue,
} from "@/components/business"
import type { SettlementDetailView } from "@/features/supplier-settlements/types"

function SettlementCenterTotals({ detail }: { detail: SettlementDetailView }) {
    const st = detail.statement
    const items = [
        {
            id: "order",
            label: "订单结算价",
            value: (
                <MoneyValue
                    value={detail.totals.orderAmountGross}
                    display="exact"
                />
            ),
        },
        {
            id: "freight",
            label: "运费",
            value: (
                <MoneyValue
                    value={detail.totals.freightGross}
                    display="exact"
                />
            ),
        },
        {
            id: "service",
            label: "服务费",
            value: (
                <MoneyValue
                    value={detail.totals.serviceFeeGross}
                    display="exact"
                />
            ),
        },
        {
            id: "refund",
            label: "供应商退款",
            value: (
                <MoneyValue value={detail.totals.refundGross} display="exact" />
            ),
        },
        {
            id: "erp",
            label: "ERP 计算金额",
            value: (
                <MoneyValue
                    value={detail.totals.erpAmountGross}
                    display="exact"
                />
            ),
            detail: `口径：${detail.totals.taxBasisLabel}`,
        },
        {
            id: "supplier",
            label: "供应商账单金额",
            value: detail.totals.supplierAmountGross ? (
                <MoneyValue
                    value={detail.totals.supplierAmountGross}
                    display="exact"
                />
            ) : (
                <span className="text-body-compact font-normal text-muted-foreground">
                    账单未同步
                </span>
            ),
            detail: detail.totals.supplierAmountGross
                ? undefined
                : "刷新试算后以 ERP 金额预填",
        },
        {
            id: "diff",
            label: "差异金额",
            value: detail.totals.differenceAmountGross ? (
                <MoneyValue
                    value={detail.totals.differenceAmountGross}
                    display="exact"
                />
            ) : (
                "—"
            ),
            warning: detail.totals.differenceDirectionLabel,
        },
        {
            id: "cost",
            label:
                st.status === "CONFIRMED"
                    ? "已确认成本差额"
                    : "待确认成本差额预览",
            value: (
                <MoneyValue
                    value={
                        detail.totals.confirmedCostDeltaGross ??
                        detail.totals.pendingCostDeltaGross ??
                        "0.00"
                    }
                    display="exact"
                />
            ),
        },
    ]

    return (
        <DetailWorkspaceMetrics
            columns={4}
            variant="bar"
            ariaLabel={`结算金额摘要（${detail.totals.taxBasisLabel}）`}
        >
            {items.map((item) => (
                <DetailWorkspaceMetricItem
                    key={item.id}
                    label={item.label}
                    value={item.value}
                    detail={
                        item.warning ? (
                            <span className="text-warning-soft-foreground">
                                {item.warning}
                            </span>
                        ) : (
                            item.detail
                        )
                    }
                    emphasis={item.id === "erp" ? "primary" : "default"}
                />
            ))}
        </DetailWorkspaceMetrics>
    )
}

export { SettlementCenterTotals }
