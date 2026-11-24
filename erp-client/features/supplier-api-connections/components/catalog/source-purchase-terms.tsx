"use client"

import { QuantityValue } from "@/components/business"
import { Badge } from "@/components/ui/badge"
import type { SourcePurchaseTerms as PurchaseTerms } from "@/lib/api/source-purchase-terms"
import { formatDateTime } from "@/lib/datetime"

const RESTRICTION_LABEL = {
    VIRTUAL_PRODUCT: "虚拟商品",
    INSTALLATION: "安装服务",
    APPOINTMENT: "预约服务",
    GIFT_SELECTION: "供应商附加履约要求",
    EXTENDED_WARRANTY: "延保服务",
    PERSONALIZED_PRICE: "专属报价条件",
    OTHER_REQUIRED_PROCESS: "供应商附加履约要求",
} as const

/** 展示来源采购约束；未取得的来源事实不补造默认值。 */
export function SourcePurchaseTerms({
    terms,
    unit,
    canReadCost = true,
}: {
    terms: PurchaseTerms | null | undefined
    unit: string | null
    canReadCost?: boolean
}) {
    if (!terms) return null
    const quote = terms.quote_context
    return (
        <section className="space-y-3" aria-label="来源采购要求">
            <h3 className="font-medium">来源采购要求</h3>
            <dl className="grid grid-cols-2 gap-x-4 gap-y-3 text-body-compact">
                {[
                    ["起购数量", terms.minimum_quantity],
                    ["购买倍数", terms.quantity_multiple],
                    ["数量上限", terms.maximum_quantity],
                ].map(([label, value]) => (
                    <div key={label} className="space-y-1">
                        <dt className="text-xs text-muted-foreground">
                            {label}
                        </dt>
                        <dd>
                            {value == null ? (
                                "未取得"
                            ) : (
                                <QuantityValue
                                    value={value}
                                    unit={unit ?? ""}
                                />
                            )}
                        </dd>
                    </div>
                ))}
            </dl>
            {quote && canReadCost ? (
                <p className="text-xs text-muted-foreground">
                    报价核验数量{" "}
                    <QuantityValue value={quote.quantity} unit={unit ?? ""} />
                    <span className="ml-2 num">
                        {formatDateTime(
                            new Date(quote.observed_at * 1000).toISOString(),
                        )}
                    </span>
                </p>
            ) : null}
            {terms.restrictions.length > 0 ? (
                <div className="space-y-2">
                    <div className="flex flex-wrap gap-1.5">
                        {terms.restrictions.map((restriction, index) => (
                            <Badge
                                key={`${restriction.kind}-${index}`}
                                variant="warning"
                            >
                                {RESTRICTION_LABEL[restriction.kind]}
                            </Badge>
                        ))}
                    </div>
                    <p className="text-xs text-muted-foreground">
                        该商品存在额外采购或履约条件，自动发单前须完成对应核验；未支持的履约要求会阻断发单。
                    </p>
                </div>
            ) : null}
            <p className="text-xs text-muted-foreground">
                实际采购仍按本次数量和收货地址核验可售、库存、报价与运费。
            </p>
        </section>
    )
}
