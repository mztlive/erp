"use client"

import { UsersIcon } from "lucide-react"

import { MoneyValue } from "@/components/business"
import { formatEffectiveRange } from "@/features/master-data/lib/filter"
import type { MasterDataListItem } from "@/features/master-data/types"

/** 公司商品池只读预览：突出销售价、规格与当前可供范围。 */
export function SellableItemPreviewPanel({ row }: { row: MasterDataListItem }) {
    const item = row.sellableItem
    if (!item) return null

    return (
        <div className="space-y-6 text-sm">
            <section className="border-b border-border pb-6">
                <div className="text-xs font-medium text-muted-foreground">
                    销售价（含税）
                </div>
                <MoneyValue
                    value={item.salesVisiblePriceGross}
                    className="mt-2 [&>span:first-child]:text-[32px] [&>span:first-child]:font-semibold [&>span:first-child]:tracking-tight"
                />
                {item.marketPrice ? (
                    <div className="mt-2 flex items-center gap-2 text-xs text-muted-foreground">
                        <span>市场参考价</span>
                        <MoneyValue
                            value={item.marketPrice}
                            className="[&>span:first-child]:text-xs [&>span:first-child]:text-muted-foreground"
                        />
                    </div>
                ) : null}
            </section>

            <section className="space-y-3 border-b border-border pb-6">
                <div className="flex items-center gap-2 font-medium">
                    <h3>可供区域</h3>
                </div>
                <div className="flex flex-wrap gap-2">
                    {item.supplyRegions.length > 0 ? (
                        item.supplyRegions.map((region) => (
                            <span key={region} className="text-sm">
                                {region}
                            </span>
                        ))
                    ) : (
                        <span className="text-muted-foreground">
                            未标注区域
                        </span>
                    )}
                </div>
                <div className="flex items-center gap-2 text-xs text-muted-foreground">
                    <UsersIcon className="size-3.5" aria-hidden />
                    <span>
                        当前由{" "}
                        <strong className="num text-foreground">
                            {item.supplierCount}
                        </strong>{" "}
                        家有效供应商支持供货
                    </span>
                </div>
            </section>

            <section className="space-y-3">
                <div className="flex items-center gap-2 font-medium">
                    <h3>商品资料</h3>
                </div>
                <dl className="space-y-3">
                    {[
                        ["SKU 编号", row.stableNo],
                        ["商品编号", item.productNo],
                        ["商品类型", item.productKindLabel],
                        ["基础单位", item.baseUnit],
                        ["SKU 版本", `v${row.revisionNo}`],
                        ["条码", item.barcode ?? "—"],
                    ].map(([label, value]) => (
                        <div
                            key={label}
                            className="flex min-w-0 items-baseline justify-between gap-5"
                        >
                            <dt className="text-xs text-muted-foreground">
                                {label}
                            </dt>
                            <dd
                                className="num min-w-0 break-all text-right text-[13px]"
                                title={value}
                            >
                                {value}
                            </dd>
                        </div>
                    ))}
                </dl>
            </section>

            <section className="border-t border-border pt-6">
                <h3 className="text-xs font-medium text-muted-foreground">
                    当前可售期间
                </h3>
                <div className="num mt-1 font-medium">
                    {formatEffectiveRange(row.effectiveFrom, row.effectiveTo)}
                </div>
                <p className="mt-2 text-xs leading-5 text-muted-foreground">
                    销售资格核对时点：{item.eligibilityAsOf}
                    。价格、商品状态或供给变化后，资格将重新计算。
                </p>
            </section>
        </div>
    )
}
