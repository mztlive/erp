"use client"

import { BusinessStatusBadge } from "@/components/business"
import { Badge } from "@/components/ui/badge"
import { Separator } from "@/components/ui/separator"
import { useAccountProfileQuery } from "@/features/auth/queries"
import {
    PreviewBlockedSection,
    PreviewHistorySection,
    PreviewSensitiveSection,
    PreviewStockSection,
} from "@/features/master-data/components/list/master-data-preview-sections"
import { masterDataCopy } from "@/features/master-data/lib/copy"
import { formatEffectiveRange } from "@/features/master-data/lib/filter"
import type {
    MasterDataCenterView,
    MasterDataListItem,
} from "@/features/master-data/types"
import { hasPermission } from "@/lib/permissions"

export { SellableItemPreviewPanel } from "@/features/master-data/components/list/master-data-sellable-preview"

export function MasterDataPreviewPanel({
    row,
    detail,
    detailLoading,
}: {
    row: MasterDataListItem
    detail: MasterDataCenterView | null | undefined
    detailLoading?: boolean
}) {
    const accountQuery = useAccountProfileQuery()
    const canRevealSensitive = hasPermission(
        accountQuery.data?.permissions,
        "supplier_sensitive:reveal",
    )

    return (
        <div className="space-y-4 text-sm">
            <section className="space-y-2">
                <h3 className="text-xs font-medium text-muted-foreground">
                    {masterDataCopy.previewIdentity}
                </h3>
                <dl className="grid grid-cols-[7rem_1fr] gap-x-3 gap-y-1.5">
                    <dt className="text-muted-foreground">
                        {masterDataCopy.colStableNo}
                    </dt>
                    <dd className="num">{row.stableNo}</dd>
                    <dt className="text-muted-foreground">
                        {masterDataCopy.colName}
                    </dt>
                    <dd>{row.name}</dd>
                    <dt className="text-muted-foreground">
                        {masterDataCopy.colLifecycle}
                    </dt>
                    <dd className="flex flex-wrap items-center gap-2">
                        <BusinessStatusBadge
                            context="preview"
                            label={row.lifecycleStatusLabel}
                            tone={row.lifecycleTone}
                        />
                        {row.scheduledLifecycleLabel ? (
                            <Badge variant="outline">
                                {row.scheduledLifecycleLabel}
                            </Badge>
                        ) : null}
                    </dd>
                    <dt className="text-muted-foreground">
                        {masterDataCopy.colVersionState}
                    </dt>
                    <dd>
                        <Badge
                            variant={
                                row.revisionTiming === "FUTURE"
                                    ? "warning"
                                    : "secondary"
                            }
                        >
                            {row.revisionTimingLabel}
                        </Badge>
                        <span className="ml-2 num text-muted-foreground">
                            v{row.revisionNo}
                        </span>
                    </dd>
                    <dt className="text-muted-foreground">
                        {masterDataCopy.colEffective}
                    </dt>
                    <dd className="num">
                        {formatEffectiveRange(
                            row.effectiveFrom,
                            row.effectiveTo,
                        )}
                    </dd>
                    {row.primaryBlocker ? (
                        <>
                            <dt className="text-muted-foreground">
                                {masterDataCopy.colBlocker}
                            </dt>
                            <dd className="text-destructive">
                                {row.primaryBlocker}
                            </dd>
                        </>
                    ) : null}
                </dl>
            </section>

            <Separator />

            <section className="space-y-2">
                <h3 className="text-xs font-medium text-muted-foreground">
                    {masterDataCopy.previewKeyFacts}
                </h3>
                <dl className="grid grid-cols-[7rem_1fr] gap-x-3 gap-y-1.5">
                    {row.keyFacts.map((fact) => (
                        <div key={fact.label} className="contents">
                            <dt className="text-muted-foreground">
                                {fact.label}
                            </dt>
                            <dd>{fact.value}</dd>
                        </div>
                    ))}
                </dl>
            </section>

            <Separator />

            <section className="space-y-2">
                <h3 className="text-xs font-medium text-muted-foreground">
                    {masterDataCopy.previewUsability}
                </h3>
                <ul className="space-y-1.5">
                    {row.selectorEligibility.map((s) => (
                        <li
                            key={s.context}
                            className="flex flex-wrap items-center gap-2 rounded-md bg-muted/50 px-2 py-1.5"
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
            </section>

            {detailLoading ? (
                <p className="text-xs text-muted-foreground">
                    {masterDataCopy.centerLoading}
                </p>
            ) : null}

            {detail?.sensitiveFields && detail.sensitiveFields.length > 0 ? (
                <PreviewSensitiveSection
                    fields={detail.sensitiveFields}
                    canRevealSensitive={canRevealSensitive}
                />
            ) : null}

            {detail?.warehouseStockSummary ? (
                <PreviewStockSection
                    policyNote={detail.warehouseStockSummary.policyNote}
                    onHandQty={detail.warehouseStockSummary.onHandQty}
                    reservedQty={detail.warehouseStockSummary.reservedQty}
                    w10Href={detail.warehouseStockSummary.w10Href}
                />
            ) : null}

            {detail?.revisionTimeline && detail.revisionTimeline.length > 0 ? (
                <PreviewHistorySection revisions={detail.revisionTimeline} />
            ) : null}

            {row.actionBlockers.length > 0 ? (
                <PreviewBlockedSection blockers={row.actionBlockers} />
            ) : null}
        </div>
    )
}
