"use client"

import { DocumentSummary } from "@/components/business"
import { Badge } from "@/components/ui/badge"
import { Card, CardContent } from "@/components/ui/card"
import type {
    ProductPublicationRevisionView,
    ProductPublicationView,
} from "@/features/product-publications/types"
import {
    MEDIA_ROLE_LABEL,
    MEDIA_SCAN_STATUS_LABEL,
} from "@/features/product-publications/types"
import { formatDateTime } from "@/lib/datetime"
import { compactFixed, multiplyFixed } from "@/lib/fixed-decimal"

/** 税率小数 → 百分比展示（0.13 → 13%） */
function percentLabel(rate: string): string {
    try {
        return `${compactFixed(
            multiplyFixed(rate, "100", {
                leftMaxScale: 6,
                rightMaxScale: 0,
                outputScale: 0,
            }),
        )}%`
    } catch {
        return rate
    }
}

export function RevisionContent({
    rev,
    fieldPermissions,
}: {
    rev: ProductPublicationRevisionView
    fieldPermissions: ProductPublicationView["fieldPermissions"]
}) {
    const supplyMasked = fieldPermissions.supplyPriceGross === "masked"
    return (
        <div className="space-y-4">
            <DocumentSummary
                columns="three"
                items={[
                    { id: "name", label: "展示名称", value: rev.name },
                    { id: "spec", label: "规格", value: rev.specification },
                    { id: "cat", label: "类目", value: rev.categoryLabel },
                    {
                        id: "price",
                        label: "含税销售价",
                        value: (
                            <span className="num">
                                ¥{rev.salesPriceGross}
                                <span className="ml-1 text-xs text-muted-foreground">
                                    税率 {percentLabel(rev.salesTaxRate)}
                                </span>
                            </span>
                        ),
                    },
                    {
                        id: "moq",
                        label: "最小购买量",
                        value: (
                            <span className="num">
                                {rev.minimumPurchaseQuantity} {rev.baseUnitCode}
                            </span>
                        ),
                    },
                    {
                        id: "saleStatus",
                        label: "商城销售状态",
                        value: rev.saleStatusLabel,
                    },
                    {
                        id: "region",
                        label: "可销售区域",
                        value: rev.salesRegionLabel,
                    },
                    {
                        id: "valid",
                        label: "生效区间",
                        value: (
                            <span className="num text-xs">
                                {formatDateTime(rev.validFrom, "default")}
                                {rev.validTo
                                    ? ` ~ ${formatDateTime(rev.validTo, "default")}`
                                    : " 起"}
                            </span>
                        ),
                    },
                ]}
            />
            <div>
                <div className="mb-1 text-xs font-medium text-muted-foreground">
                    商城销售说明
                </div>
                <p className="whitespace-pre-wrap text-sm">
                    {rev.salesDescription}
                </p>
            </div>
            <div>
                <div className="mb-1 text-xs font-medium text-muted-foreground">
                    商品能力
                </div>
                <div className="flex flex-wrap gap-1">
                    {rev.productCapabilities.map((c) => (
                        <Badge key={c} variant="secondary">
                            {c}
                        </Badge>
                    ))}
                </div>
            </div>
            <div>
                <div className="mb-1 text-xs font-medium text-muted-foreground">
                    唯一固定供给
                </div>
                <Card
                    size="sm"
                    className="border-0 bg-muted/40 shadow-none ring-0"
                >
                    <CardContent className="space-y-1 pt-3 text-sm">
                        <div>
                            {rev.fixedOffering.supplierName} ·{" "}
                            {rev.fixedOffering.availabilityLabel}
                        </div>
                        <div className="text-xs text-muted-foreground">
                            供货价{" "}
                            {supplyMasked ||
                            !rev.fixedOffering.supplyPriceVisible
                                ? "******"
                                : rev.fixedOffering.supplyPriceGross
                                  ? `¥${rev.fixedOffering.supplyPriceGross}`
                                  : "—"}
                            {rev.fixedOffering.supplierMoq ? (
                                <>
                                    {" · 供应商起订 "}
                                    <span className="num">
                                        {rev.fixedOffering.supplierMoq}
                                    </span>
                                    （不自动写入最小购买量）
                                </>
                            ) : null}
                        </div>
                        <p className="text-xs text-muted-foreground">
                            本修订恰好绑定一条固定供给；供货价变化不会自动修改销售价。
                        </p>
                    </CardContent>
                </Card>
            </div>
            <div>
                <div className="mb-1 text-xs font-medium text-muted-foreground">
                    媒体
                </div>
                <ul className="grid gap-2 sm:grid-cols-2">
                    {rev.media.map((m) => (
                        <li
                            key={`${m.fileAssetId}-${m.sortNo}`}
                            className="flex gap-2 rounded-lg bg-muted/40 p-2 text-sm"
                        >
                            <div className="flex size-14 shrink-0 items-center justify-center rounded bg-muted text-2xs text-muted-foreground">
                                {MEDIA_ROLE_LABEL[m.mediaRole]}
                            </div>
                            <div className="min-w-0">
                                <div className="font-medium">
                                    {MEDIA_ROLE_LABEL[m.mediaRole]} · #
                                    {m.sortNo}
                                </div>
                                <div className="truncate text-xs text-muted-foreground">
                                    {m.altText || "（缺图片说明）"}
                                </div>
                                <div className="text-xs">
                                    安全检查{" "}
                                    <Badge
                                        variant={
                                            m.securityScanStatus === "PASSED"
                                                ? "secondary"
                                                : "destructive"
                                        }
                                        className="text-2xs"
                                    >
                                        {
                                            MEDIA_SCAN_STATUS_LABEL[
                                                m.securityScanStatus
                                            ]
                                        }
                                    </Badge>
                                </div>
                            </div>
                        </li>
                    ))}
                </ul>
            </div>
            <p className="text-xs text-muted-foreground">
                历史记录不随后续主档变化覆盖
            </p>
        </div>
    )
}
