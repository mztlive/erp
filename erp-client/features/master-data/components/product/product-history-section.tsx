"use client"

import { DocumentSection, RevisionTimeline } from "@/components/business"
import { Badge } from "@/components/ui/badge"
import { masterDataCopy } from "@/features/master-data/lib/copy"
import { formatEffectiveRange } from "@/features/master-data/lib/filter"
import type { MasterDataCenterView } from "@/features/master-data/types"
import { cn } from "@/lib/utils"
import { formatDateTime } from "@/lib/datetime"

type ProductHistorySectionProps = {
    data: MasterDataCenterView | null | undefined
}

function ProductHistorySection({ data }: ProductHistorySectionProps) {
    return data ? (
        <section
            id="product-section-history"
            aria-label="历史与引用"
            className={cn(
                "scroll-mt-[var(--product-section-scroll-margin)] px-5",
            )}
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
                                                            销售价{" "}
                                                            {sku.salePrice ??
                                                                "—"}{" "}
                                                            · 市场价{" "}
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
                title={masterDataCopy.centerRelations}
                description={masterDataCopy.centerRelationsDesc}
            >
                <p className="text-sm">
                    {masterDataCopy.centerUsageCount(
                        data.usageSummary.historicalReferenceCount,
                    )}
                    {data.usageSummary.note}
                </p>
                <ul className="mt-3 space-y-2">
                    {data.selectorEligibility.map((s) => (
                        <li
                            key={s.context}
                            className="flex flex-wrap items-center gap-2 rounded-md bg-muted/40 px-2 py-1.5 text-sm"
                        >
                            <span>{s.contextLabel}</span>
                            <Badge
                                variant={s.eligible ? "success" : "destructive"}
                            >
                                {s.eligible
                                    ? masterDataCopy.eligible
                                    : masterDataCopy.ineligible}
                            </Badge>
                            {s.reason ? (
                                <span className="text-xs text-muted-foreground">
                                    {s.reason}
                                </span>
                            ) : null}
                        </li>
                    ))}
                </ul>
            </DocumentSection>

            <DocumentSection
                title={masterDataCopy.centerAudit}
                description={masterDataCopy.centerAuditDesc}
            >
                {data.auditEvents.length === 0 ? (
                    <p className="text-sm text-muted-foreground">
                        {masterDataCopy.centerNoAudit}
                    </p>
                ) : (
                    <ul className="space-y-2 text-sm">
                        {data.auditEvents.map((ev) => (
                            <li
                                key={ev.id}
                                className="rounded-md border border-border px-3 py-2"
                            >
                                <div className="flex flex-wrap gap-2">
                                    <span className="num text-xs text-muted-foreground">
                                        {formatDateTime(
                                            ev.at,
                                            "full",
                                            "passthrough",
                                        )}
                                    </span>
                                    <span>{ev.actor}</span>
                                    <Badge variant="outline">{ev.action}</Badge>
                                </div>
                                <div className="mt-1 text-muted-foreground">
                                    {ev.detail}
                                </div>
                            </li>
                        ))}
                    </ul>
                )}
            </DocumentSection>
        </section>
    ) : null
}

export { ProductHistorySection }
