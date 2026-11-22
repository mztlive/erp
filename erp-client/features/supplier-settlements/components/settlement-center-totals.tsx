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
                    taxBasis="gross"
                />
            ),
        },
        {
            id: "freight",
            label: "运费",
            value: (
                <MoneyValue
                    value={detail.totals.freightGross}
                    taxBasis="gross"
                />
            ),
        },
        {
            id: "service",
            label: "服务费",
            value: (
                <MoneyValue
                    value={detail.totals.serviceFeeGross}
                    taxBasis="gross"
                />
            ),
        },
        {
            id: "refund",
            label: "供应商退款",
            value: (
                <MoneyValue
                    value={detail.totals.refundGross}
                    taxBasis="gross"
                />
            ),
        },
        {
            id: "erp",
            label: "ERP 计算金额",
            value: (
                <MoneyValue
                    value={detail.totals.erpAmountGross}
                    taxBasis="gross"
                />
            ),
        },
        {
            id: "supplier",
            label: "供应商账单金额",
            value: detail.totals.supplierAmountGross ? (
                <MoneyValue
                    value={detail.totals.supplierAmountGross}
                    taxBasis="gross"
                />
            ) : (
                <span className="text-body-compact font-normal text-muted-foreground">
                    账单未同步 · 刷新试算后以 ERP 金额预填
                </span>
            ),
        },
        {
            id: "diff",
            label: "差异金额",
            value: detail.totals.differenceAmountGross ? (
                <MoneyValue
                    value={detail.totals.differenceAmountGross}
                    taxBasis="gross"
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
                    taxBasis="gross"
                />
            ),
        },
    ]
    return (
        <section className="py-4" aria-label="金额摘要">
            <h2 className="text-section-title font-semibold tracking-tight">
                金额摘要
            </h2>
            <p className="mt-1 text-xs text-muted-foreground">
                订单、运费、服务费、退款与 ERP
                计算金额、供应商账单金额、差异方向对比 · 全部
                {detail.totals.taxBasisLabel}
            </p>
            <DetailWorkspaceMetrics columns={4}>
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
                            ) : undefined
                        }
                        emphasis={item.id === "erp" ? "primary" : "default"}
                    />
                ))}
            </DetailWorkspaceMetrics>
        </section>
    )
}

export { SettlementCenterTotals }
