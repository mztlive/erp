"use client"

import { DetailWorkspaceSection, RevisionTimeline } from "@/components/business"
import { MasterDataAuditList } from "@/features/master-data/components/shared/audit-event-list"
import { masterDataRevisionEntry } from "@/features/master-data/components/shared/revision-timeline-item"
import { masterDataCopy } from "@/features/master-data/lib/copy"
import { formatEffectiveRange } from "@/features/master-data/lib/filter"
import type { MasterDataCenterView } from "@/features/master-data/types"

type ProductHistorySectionProps = {
    data: MasterDataCenterView | null | undefined
}

function ProductHistorySection({ data }: ProductHistorySectionProps) {
    return data ? (
        <>
            <DetailWorkspaceSection
                id="product-section-history"
                title={masterDataCopy.centerVersions}
                width="form"
            >
                <RevisionTimeline
                    revisions={data.revisionTimeline.map((rev) =>
                        masterDataRevisionEntry(rev, {
                            effectiveLabel: formatEffectiveRange(
                                rev.effectiveFrom,
                                rev.effectiveTo,
                            ),
                            showTiming: true,
                            extra: rev.productSnapshot ? (
                                <details className="mt-2 rounded-md bg-muted/35 px-3 py-2 text-xs">
                                    <summary className="cursor-pointer font-medium text-foreground">
                                        查看本版本的完整 SKU 与价格明细
                                    </summary>
                                    <div className="mt-2 space-y-2">
                                        <div className="text-muted-foreground">
                                            单位 {rev.productSnapshot.baseUnit}
                                            （
                                            <span className="num">
                                                {
                                                    rev.productSnapshot
                                                        .baseUnitCode
                                                }
                                            </span>
                                            ） · 分类{" "}
                                            {rev.productSnapshot.category} ·
                                            品牌 {rev.productSnapshot.brand}
                                        </div>
                                        <ul className="divide-y divide-border/60 border-y border-border/60">
                                            {rev.productSnapshot.skus.map(
                                                (sku) => (
                                                    <li
                                                        key={`${rev.id}:${sku.skuNo}`}
                                                        className="py-2"
                                                    >
                                                        <div className="font-medium text-foreground">
                                                            <span className="num">
                                                                {sku.skuNo}
                                                            </span>{" "}
                                                            · {sku.specLabel}
                                                        </div>
                                                        <div className="num mt-0.5 text-muted-foreground">
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
                                                    </li>
                                                ),
                                            )}
                                        </ul>
                                        <p className="text-muted-foreground">
                                            供应商、订货编码、成本、税费和起订量按供给关系独立维护，不写入商品版本。
                                        </p>
                                    </div>
                                </details>
                            ) : null,
                        }),
                    )}
                />
            </DetailWorkspaceSection>

            <DetailWorkspaceSection
                title={masterDataCopy.centerAudit}
                width="form"
            >
                <MasterDataAuditList events={data.auditEvents} />
            </DetailWorkspaceSection>
        </>
    ) : null
}

export { ProductHistorySection }
