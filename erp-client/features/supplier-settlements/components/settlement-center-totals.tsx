"use client"

import {
    DocumentTotals,
    MoneyValue,
    surfacePanelClassName,
} from "@/components/business"
import {
    Card,
    CardContent,
    CardDescription,
    CardHeader,
    CardTitle,
} from "@/components/ui/card"
import type { SettlementDetailView } from "@/features/supplier-settlements/types"

function SettlementCenterTotals({
    detail,
}: {
    detail: SettlementDetailView
}) {
    const st = detail.statement
    return (
        <Card size="sm" className={surfacePanelClassName}>
            <CardHeader className="rounded-t-lg border-b border-grid py-3">
                <CardTitle className="text-base">金额摘要</CardTitle>
                <CardDescription>
                    订单、运费、服务费、退款与 ERP
                    计算金额、供应商账单金额、差异方向对比 · 全部
                    {detail.totals.taxBasisLabel}
                </CardDescription>
            </CardHeader>
            <CardContent className="pt-4">
                <DocumentTotals
                    title={null}
                    items={[
                        {
                            id: "order",
                            label: "订单结算价",
                            value: (
                                <MoneyValue
                                    value={detail.totals.orderAmountGross}
                                    taxBasis="gross"
                                />
                            ),
                            basis: "含税",
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
                            basis: "含税",
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
                            basis: "含税",
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
                            basis: "含税",
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
                            basis: "含税",
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
                                "账单未同步 · 刷新试算后以 ERP 金额预填"
                            ),
                            basis: "含税",
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
                            basis: "含税",
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
                            basis: "含税",
                        },
                    ]}
                />
            </CardContent>
        </Card>
    )
}

export { SettlementCenterTotals }
