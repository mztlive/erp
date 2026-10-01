"use client"

import type { ReactNode } from "react"
import { MoneyValue, QuantityValue, RateValue } from "@/components/business"
import { percentageFromRate } from "../lib/offering-forms"
import { offeringValidity } from "../lib/detail"
import type { OfferingRevisionView } from "../types"

export function OfferingField({
    label,
    children,
}: {
    label: string
    children: ReactNode
}) {
    return (
        <div className="flex min-w-0 items-baseline justify-between gap-5">
            <dt className="shrink-0 text-xs text-muted-foreground">{label}</dt>
            <dd className="min-w-0 break-words text-right text-sm">
                {children}
            </dd>
        </div>
    )
}

/** 当前资料与历史版本复用商业条款正文，历史不接收任何实时字段。 */
export function OfferingTerms({
    terms,
    canViewCosts,
    compact = false,
}: {
    terms: Omit<
        OfferingRevisionView,
        "id" | "created_at" | "revision_no" | "is_current"
    >
    canViewCosts: boolean
    compact?: boolean
}) {
    return (
        <div className="space-y-6 text-sm">
            <section className="border-b border-border pb-6">
                <div className="grid grid-cols-2 gap-4">
                    <div>
                        <h3 className="text-xs font-medium text-muted-foreground">
                            一件代发价（含税）
                        </h3>
                        <MoneyValue
                            value={
                                canViewCosts
                                    ? terms.dropship_supply_price_gross
                                    : null
                            }
                            size="section"
                            className="mt-2"
                        />
                    </div>
                    <div>
                        <h3 className="text-xs font-medium text-muted-foreground">
                            集采价（含税）
                        </h3>
                        <MoneyValue
                            value={
                                canViewCosts
                                    ? terms.bulk_supply_price_gross
                                    : null
                            }
                            size="section"
                            className="mt-2"
                        />
                    </div>
                </div>
                {!canViewCosts ? (
                    <p className="mt-3 text-xs text-muted-foreground">
                        当前账号无权查看采购成本、税率和费用。
                    </p>
                ) : null}
                <dl className="mt-4 space-y-3">
                    <OfferingField label="集采起订量">
                        {terms.bulk_minimum_order_quantity != null ? (
                            <QuantityValue
                                value={terms.bulk_minimum_order_quantity}
                                unit=""
                            />
                        ) : (
                            "未提供"
                        )}
                    </OfferingField>
                    {canViewCosts ? (
                        <OfferingField label="进项税率">
                            {terms.input_tax_rate != null ? (
                                <RateValue
                                    value={percentageFromRate(
                                        terms.input_tax_rate,
                                    )}
                                    precision={2}
                                />
                            ) : (
                                "—"
                            )}
                        </OfferingField>
                    ) : null}
                    {!compact && canViewCosts ? (
                        <>
                            <OfferingField label="一件代发价（不含税）">
                                <MoneyValue
                                    value={terms.dropship_supply_price_net}
                                />
                            </OfferingField>
                            <OfferingField label="集采价（不含税）">
                                <MoneyValue
                                    value={terms.bulk_supply_price_net}
                                />
                            </OfferingField>
                        </>
                    ) : null}
                </dl>
            </section>
            <section className="space-y-3">
                <h3 className="font-medium">供应条件</h3>
                <dl className="space-y-3">
                    <OfferingField label="可供区域">
                        {terms.supply_region.join("、") || "未标注"}
                    </OfferingField>
                    <OfferingField label="条款有效期">
                        <span className="num">{offeringValidity(terms)}</span>
                    </OfferingField>
                    <OfferingField label="快递说明">
                        {terms.dropship_express || "未标注"}
                    </OfferingField>
                    {canViewCosts ? (
                        <>
                            <OfferingField label="运费">
                                <MoneyValue value={terms.freight_amount} />
                            </OfferingField>
                            <OfferingField label="服务费">
                                <MoneyValue value={terms.service_fee_amount} />
                            </OfferingField>
                        </>
                    ) : null}
                    {!compact ? (
                        <OfferingField label="商品能力">
                            {terms.product_capabilities
                                .map(capabilityLabel)
                                .join("、") || "未标注"}
                        </OfferingField>
                    ) : null}
                </dl>
            </section>
        </div>
    )
}

/** 已知能力以业务名称展示，未知编码不直接上屏。 */
function capabilityLabel(value: string) {
    const labels: Record<string, string> = {
        physical: "实物商品",
        virtual: "虚拟商品",
        offline_service: "线下服务",
        api: "接口供货",
        printing: "印刷",
        REFUND: "支持退款",
        CANCEL: "支持取消",
        RETURN: "支持退货",
        DROPSHIP: "一件代发",
        BULK: "集采",
    }
    return (
        labels[value] ??
        (/^[a-zA-Z][a-zA-Z0-9_]*$/.test(value) ? "其他能力" : value)
    )
}
