"use client"

import { DocumentSection, RevisionTimeline } from "@/components/business"
import { Badge } from "@/components/ui/badge"
import { MasterDataAuditList } from "@/features/master-data/components/shared/audit-event-list"
import { masterDataCopy } from "@/features/master-data/lib/copy"
import { formatEffectiveRange } from "@/features/master-data/lib/filter"
import type { MasterDataCenterView } from "@/features/master-data/types"

type ProductHistorySectionProps = {
    data: MasterDataCenterView | null | undefined
}

function ProductHistorySection({ data }: ProductHistorySectionProps) {
    return data ? (
        <section
            id="product-section-history"
            aria-label="历史与引用"
            className="[&_[data-slot=document-section]:first-of-type]:pt-0"
        >
            <DocumentSection
                title={masterDataCopy.centerVersions}
                description={masterDataCopy.centerVersionsDesc}
            >
                <RevisionTimeline
                    revisions={data.revisionTimeline.map((rev) => ({
                        id: rev.id,
                        version: rev.revisionNo,
                        source: "erp-change" as const,
                        actor: rev.actor,
                        effectiveAt: {
                            dateTime: rev.effectiveFrom,
                            label: formatEffectiveRange(
                                rev.effectiveFrom,
                                rev.effectiveTo,
                            ),
                        },
                        reason: (
                            <div className="space-y-1">
                                <div>
                                    {masterDataCopy.centerHistoryName}：
                                    <strong>{rev.nameSnapshot}</strong>
                                </div>
                                <div>{rev.changeReason}</div>
                                <div className="flex flex-wrap gap-2">
                                    <Badge variant="outline">
                                        {rev.timingLabel}
                                    </Badge>
                                    <Badge variant="secondary">
                                        {rev.lifecycleAtRevision === "ENABLED"
                                            ? "启用"
                                            : "停用"}
                                    </Badge>
                                </div>
                                {rev.productSnapshot ? (
                                    <details className="mt-2 rounded-lg border bg-muted/30 p-2 text-xs">
                                        <summary className="cursor-pointer font-medium">
                                            查看本版本的完整 SKU 与价格明细
                                        </summary>
                                        <div className="mt-2 space-y-2">
                                            <div>
                                                单位{" "}
                                                {rev.productSnapshot.baseUnit}（
                                                {
                                                    rev.productSnapshot
                                                        .baseUnitCode
                                                }
                                                ） · 分类{" "}
                                                {rev.productSnapshot.category} ·
                                                品牌 {rev.productSnapshot.brand}
                                            </div>
                                            {rev.productSnapshot.skus.map(
                                                (sku) => (
                                                    <div
                                                        key={`${rev.id}:${sku.skuNo}`}
                                                        className="rounded border bg-card p-2"
                                                    >
                                                        <div className="font-medium">
                                                            {sku.skuNo} ·{" "}
                                                            {sku.specLabel}
                                                        </div>
                                                        <div className="mt-1 text-muted-foreground">
                                                            一件代发价{" "}
                                                            {sku.salePrice ??
                                                                "—"}
                                                            {" · "}集采价{" "}
                                                            {sku.bulkPriceGross ??
                                                                "—"}
                                                            {" · "}集采起订量{" "}
                                                            {sku.bulkMinQuantity ??
                                                                "—"}
                                                            {" · "}市场价{" "}
                                                            {sku.marketPrice ??
                                                                "—"}
                                                        </div>
                                                    </div>
                                                ),
                                            )}
                                            <p className="text-muted-foreground">
                                                供应商、订货编码、成本、税费和起订量按供给关系独立维护，不写入商品版本。
                                            </p>
                                        </div>
                                    </details>
                                ) : null}
                            </div>
                        ),
                        isCurrent: rev.isCurrent,
                    }))}
                />
            </DocumentSection>

            <DocumentSection
                title={masterDataCopy.centerAudit}
                description={masterDataCopy.centerAuditDesc}
            >
                <MasterDataAuditList events={data.auditEvents} />
            </DocumentSection>
        </section>
    ) : null
}

export { ProductHistorySection }
