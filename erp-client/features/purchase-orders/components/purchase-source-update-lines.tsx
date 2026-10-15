"use client"

import { MoneyValue, QuantityValue, RateValue } from "@/components/business"
import { multiplyFixed } from "@/lib/fixed-decimal"
import type { PurchaseOrderCenterView } from "../types"

/** 工作台明细用紧凑行展示主字段，辅助信息在行内换行。 */
export function PurchaseSourceUpdateLines({
    order,
}: {
    order: PurchaseOrderCenterView
}) {
    const { lines, costMasked } = order.currentContent
    const amount = (value: string) =>
        costMasked ? (
            <span className="text-muted-foreground" aria-label="金额已隐藏">
                •••
            </span>
        ) : (
            <MoneyValue value={value} className="max-w-full wrap-anywhere" />
        )

    if (lines.length === 0)
        return <p className="text-sm text-muted-foreground">暂无采购明细</p>

    return (
        <div className="min-w-0 @container/purchase-lines">
            <div
                aria-hidden="true"
                className="hidden grid-cols-[minmax(0,1fr)_5rem_7rem_8rem] gap-x-4 border-b border-grid pb-2 text-xs text-muted-foreground @xl/purchase-lines:grid"
            >
                <span>商品 / 费用</span>
                <span className="text-right">采购数量</span>
                <span className="text-right">含税单价</span>
                <span className="text-right">含税小计</span>
            </div>
            <div className="divide-y divide-grid">
                {lines.map((line) => {
                    const isFee = line.lineType === "LOGISTICS_FEE"
                    return (
                        <article
                            key={line.lineId}
                            className="min-w-0 space-y-2.5 py-4"
                        >
                            <div className="grid min-w-0 grid-cols-3 items-baseline gap-x-4 gap-y-3 @xl/purchase-lines:grid-cols-[minmax(0,1fr)_5rem_7rem_8rem]">
                                <div className="col-span-3 min-w-0 @xl/purchase-lines:col-span-1">
                                    <h4 className="text-sm font-medium wrap-anywhere">
                                        {line.itemName}
                                    </h4>
                                    {line.salesAllocationLabel || isFee ? (
                                        <p className="mt-1 text-xs text-muted-foreground wrap-anywhere">
                                            {isFee
                                                ? "费用"
                                                : line.salesAllocationLabel}
                                        </p>
                                    ) : null}
                                </div>
                                <div className="min-w-0 text-sm @xl/purchase-lines:text-right">
                                    <span className="mb-1 block text-xs text-muted-foreground @xl/purchase-lines:sr-only">
                                        采购数量
                                    </span>
                                    {isFee || line.quantity == null ? (
                                        "—"
                                    ) : (
                                        <QuantityValue
                                            value={line.quantity}
                                            unit={line.unit}
                                            className="max-w-full flex-wrap wrap-anywhere"
                                        />
                                    )}
                                </div>
                                <div className="min-w-0 text-right text-sm">
                                    <span className="mb-1 block text-xs text-muted-foreground @xl/purchase-lines:sr-only">
                                        含税单价
                                    </span>
                                    {isFee ? "—" : amount(line.unitCostGross)}
                                </div>
                                <div className="min-w-0 text-right text-sm">
                                    <span className="mb-1 block text-xs text-muted-foreground @xl/purchase-lines:sr-only">
                                        含税小计
                                    </span>
                                    {amount(line.grossAmount)}
                                </div>
                            </div>
                            <dl className="flex min-w-0 flex-wrap gap-x-5 gap-y-1 text-xs text-muted-foreground">
                                <div className="flex min-w-0 flex-wrap gap-x-1.5">
                                    <dt>预计交付</dt>
                                    <dd className="num">
                                        {line.expectedDeliveryDate ?? "未填写"}
                                    </dd>
                                </div>
                                {!isFee ? (
                                    <div className="flex min-w-0 flex-wrap gap-x-1.5">
                                        <dt>客户成交含税单价</dt>
                                        <dd className="min-w-0">
                                            <MoneyValue
                                                value={line.salesUnitPriceGross}
                                                className="max-w-full font-normal wrap-anywhere"
                                            />
                                        </dd>
                                    </div>
                                ) : null}
                                <div className="flex min-w-0 flex-wrap gap-x-1.5">
                                    <dt>税率</dt>
                                    <dd>
                                        <RateValue
                                            value={multiplyFixed(
                                                line.inputTaxRate,
                                                "100",
                                                {
                                                    leftMaxScale: 6,
                                                    rightMaxScale: 0,
                                                    outputScale: 2,
                                                },
                                            )}
                                            precision={2}
                                            className="text-muted-foreground"
                                        />
                                    </dd>
                                </div>
                                <div className="flex min-w-0 flex-wrap gap-x-1.5">
                                    <dt>税额</dt>
                                    <dd className="min-w-0">
                                        {amount(line.taxAmount)}
                                    </dd>
                                </div>
                            </dl>
                        </article>
                    )
                })}
            </div>
        </div>
    )
}
