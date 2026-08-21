"use client"

import { MapPinIcon, PackageIcon, UsersIcon } from "lucide-react"

import { MoneyValue } from "@/components/business"
import { Badge } from "@/components/ui/badge"
import { formatEffectiveRange } from "@/features/master-data/lib/filter"
import type { MasterDataListItem } from "@/features/master-data/types"

/** 公司商品池只读预览：突出销售价、规格与当前可供范围。 */
export function SellableItemPreviewPanel({ row }: { row: MasterDataListItem }) {
    const item = row.sellableItem
    if (!item) return null

    return (
        <div className="space-y-5 text-sm">
            <section className="rounded-xl border border-primary/20 bg-primary/5 p-4">
                <div className="text-xs font-medium text-muted-foreground">
                    销售价
                </div>
                <MoneyValue
                    value={item.salesVisiblePriceGross}
                    taxBasis="gross"
                    className="mt-2 [&>span:first-child]:text-2xl"
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
                <p className="mt-3 text-xs leading-5 text-muted-foreground">
                    销售选品默认使用的公司 SKU 价格，不包含任何供应商采购成本。
                </p>
            </section>

            <section className="space-y-3 rounded-xl border bg-card p-4">
                <div className="flex items-center gap-2 font-medium">
                    <MapPinIcon className="size-4 text-primary" aria-hidden />
                    <h3>可供区域</h3>
                </div>
                <div className="flex flex-wrap gap-2">
                    {item.supplyRegions.length > 0 ? (
                        item.supplyRegions.map((region) => (
                            <Badge key={region} variant="outline">
                                {region}
                            </Badge>
                        ))
                    ) : (
                        <span className="text-muted-foreground">
                            未标注区域
                        </span>
                    )}
                </div>
                <div className="flex items-center gap-2 border-t pt-3 text-xs text-muted-foreground">
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
                    <PackageIcon className="size-4 text-primary" aria-hidden />
                    <h3>商品资料</h3>
                </div>
                <dl className="grid grid-cols-2 gap-2">
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
                            className="min-w-0 rounded-lg bg-muted/50 p-3"
                        >
                            <dt className="text-xs text-muted-foreground">
                                {label}
                            </dt>
                            <dd
                                className="num mt-1 truncate font-medium"
                                title={value}
                            >
                                {value}
                            </dd>
                        </div>
                    ))}
                </dl>
            </section>

            <section className="rounded-xl border bg-muted/30 p-4">
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
