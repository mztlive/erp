"use client"

import type { ReactNode } from "react"

import { MoneyValue, QuantityValue, RateValue } from "@/components/business"
import type { PurchaseOrderCenterView } from "@/features/purchase-orders/types"
import { multiplyFixed } from "@/lib/fixed-decimal"

function LineField({
    label,
    children,
}: {
    label: string
    children: ReactNode
}) {
    return (
        <div className="min-w-0 space-y-1.5">
            <dt className="text-xs leading-5 text-muted-foreground">{label}</dt>
            <dd className="min-w-0 text-sm leading-6 wrap-anywhere">
                {children}
            </dd>
        </div>
    )
}

/** 品名独占主信息区，交易字段按明细容器宽度排列，避免长品名被数值列挤压。 */
export function PurchaseOrderDetailLines({
    order,
    costMasked,
}: {
    order: PurchaseOrderCenterView
    costMasked: boolean
}) {
    const { lines } = order.currentContent
    const amount = (value: string, exact = false) =>
        costMasked ? (
            <span className="text-muted-foreground" aria-label="金额已隐藏">
                •••
            </span>
        ) : (
            <MoneyValue
                value={value}
                display={exact ? "exact" : "full"}
                className="max-w-full wrap-anywhere"
            />
        )

    if (lines.length === 0) {
        return <p className="text-sm text-muted-foreground">暂无采购明细</p>
    }

    return (
        <ol
            aria-label="采购明细"
            className="min-w-0 divide-y divide-border/60 @container/purchase-detail-lines"
        >
            {lines.map((line) => {
                const isFee = line.lineType === "LOGISTICS_FEE"
                const quote = line.inquiryQuoteSource

                return (
                    <li
                        key={line.lineId}
                        className="min-w-0 space-y-4 py-5 first:pt-0 last:pb-0"
                    >
                        <div className="grid min-w-0 gap-3 @lg/purchase-detail-lines:grid-cols-[minmax(0,1fr)_minmax(8rem,auto)] @lg/purchase-detail-lines:gap-6">
                            <div className="min-w-0 space-y-2">
                                <h3 className="text-base leading-7 font-medium wrap-anywhere">
                                    {line.itemName}
                                </h3>
                                <div className="flex flex-wrap items-baseline gap-x-3 gap-y-1 text-xs leading-5 text-muted-foreground">
                                    <span>{isFee ? "费用" : "商品/服务"}</span>
                                    {line.procurementConfirmationLineId ? (
                                        <span className="min-w-0 wrap-anywhere">
                                            {line.salesAllocationLabel ??
                                                `确认分行 · ${line.itemName}`}
                                        </span>
                                    ) : null}
                                </div>
                                {quote ? (
                                    <p className="text-xs leading-5 text-muted-foreground wrap-anywhere">
                                        {quote.inquiry_no
                                            ? `询价 ${quote.inquiry_no}`
                                            : "来源询价"}
                                        {" · 需求 v"}
                                        {quote.requirement_version}
                                        {" · 反馈第 "}
                                        {quote.feedback_round}
                                        {" 轮"}
                                        {" · 实际报价 v"}
                                        {quote.quote_revision_no}
                                        {quote.adopted_quantity ? (
                                            <>
                                                {" · 本次采用 "}
                                                <QuantityValue
                                                    value={
                                                        quote.adopted_quantity
                                                    }
                                                    unit={line.unit}
                                                />
                                            </>
                                        ) : null}
                                    </p>
                                ) : null}
                            </div>
                            <dl className="min-w-0 @lg/purchase-detail-lines:text-right">
                                <div className="space-y-1">
                                    <dt className="text-xs text-muted-foreground">
                                        含税小计
                                    </dt>
                                    <dd className="text-lg font-semibold">
                                        {amount(line.grossAmount)}
                                    </dd>
                                </div>
                                <div className="mt-1 flex flex-wrap items-baseline gap-x-1.5 text-xs text-muted-foreground @lg/purchase-detail-lines:justify-end">
                                    <dt>税额</dt>
                                    <dd>{amount(line.taxAmount)}</dd>
                                </div>
                            </dl>
                        </div>
                        <dl className="grid min-w-0 grid-cols-1 gap-x-6 gap-y-4 rounded-lg bg-muted/30 px-4 py-3 @xs/purchase-detail-lines:grid-cols-2 @2xl/purchase-detail-lines:grid-cols-4">
                            {!isFee ? (
                                <>
                                    <LineField label="数量">
                                        <QuantityValue
                                            value={line.quantity ?? "0"}
                                            unit={line.unit}
                                            className="max-w-full flex-wrap wrap-anywhere"
                                        />
                                    </LineField>
                                    <LineField label="采购含税单价">
                                        {amount(line.unitCostGross, true)}
                                        {!costMasked ? (
                                            <div className="text-xs text-muted-foreground">
                                                税率{" "}
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
                                                />
                                            </div>
                                        ) : null}
                                    </LineField>
                                    <LineField label="客户成交含税单价">
                                        <MoneyValue
                                            value={line.salesUnitPriceGross}
                                            className="max-w-full wrap-anywhere"
                                        />
                                    </LineField>
                                    <LineField label="交期">
                                        <span className="num">
                                            {line.expectedDeliveryDate ?? "—"}
                                        </span>
                                    </LineField>
                                </>
                            ) : (
                                <LineField label="税率">
                                    {costMasked ? (
                                        "•••"
                                    ) : (
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
                                        />
                                    )}
                                </LineField>
                            )}
                        </dl>
                    </li>
                )
            })}
        </ol>
    )
}
