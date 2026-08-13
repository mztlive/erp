"use client"

import Link from "next/link"
import {
    ChevronDownIcon,
    MapPinIcon,
    PackageIcon,
    UsersIcon,
} from "lucide-react"

import {
    BusinessStatusBadge,
    MoneyValue,
    RevisionTimeline,
    SensitiveValue,
} from "@/components/business"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { Separator } from "@/components/ui/separator"
import { useAccountProfileQuery } from "@/features/auth/queries"
import {
    masterDataActionLabel,
    masterDataCopy,
} from "@/features/master-data/copy"
import { formatEffectiveRange } from "@/features/master-data/filter"
import { revealMasterDataSensitive } from "@/features/master-data/queries"
import type {
    MasterDataCenterView,
    MasterDataListItem,
} from "@/features/master-data/types"
import { hasPermission } from "@/lib/permissions"

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
                <>
                    <Separator />
                    <section className="space-y-2">
                        <h3 className="text-xs font-medium text-muted-foreground">
                            {masterDataCopy.previewSensitive}
                        </h3>
                        <ul className="space-y-2">
                            {detail.sensitiveFields.map((field) => (
                                <li
                                    key={field.label}
                                    className="flex flex-wrap items-center gap-2"
                                >
                                    <span className="text-muted-foreground">
                                        {field.label}
                                    </span>
                                    {field.visibility === "masked" &&
                                    field.revealToken &&
                                    canRevealSensitive ? (
                                        <SensitiveValue
                                            label={field.label}
                                            maskedValue={field.maskedValue}
                                            onReveal={() =>
                                                revealMasterDataSensitive(
                                                    field.revealToken!,
                                                )
                                            }
                                        />
                                    ) : (
                                        <code className="num rounded bg-muted px-2 py-0.5 text-xs">
                                            {field.maskedValue}
                                        </code>
                                    )}
                                </li>
                            ))}
                        </ul>
                    </section>
                </>
            ) : null}

            {detail?.warehouseStockSummary ? (
                <>
                    <Separator />
                    <section className="space-y-2">
                        <h3 className="text-xs font-medium text-muted-foreground">
                            {masterDataCopy.previewStock}
                        </h3>
                        <p className="text-xs text-muted-foreground">
                            {detail.warehouseStockSummary.policyNote}
                        </p>
                        <p>
                            在库{" "}
                            <span className="num">
                                {detail.warehouseStockSummary.onHandQty}
                            </span>
                            {" · "}
                            预占{" "}
                            <span className="num">
                                {detail.warehouseStockSummary.reservedQty}
                            </span>
                        </p>
                        <Button
                            type="button"
                            size="sm"
                            variant="outline"
                            render={
                                <Link
                                    href={detail.warehouseStockSummary.w10Href}
                                />
                            }
                        >
                            打开库存台账
                        </Button>
                    </section>
                </>
            ) : null}

            {detail?.revisionTimeline && detail.revisionTimeline.length > 0 ? (
                <>
                    <Separator />
                    <section className="space-y-2">
                        <details className="group" open={false}>
                            <summary className="flex cursor-pointer list-none items-center gap-1 text-xs font-medium text-muted-foreground [&::-webkit-details-marker]:hidden">
                                {masterDataCopy.previewHistory}
                                <ChevronDownIcon
                                    className="size-3.5 transition-transform group-open:rotate-180"
                                    aria-hidden
                                />
                            </summary>
                            <div className="mt-2">
                                <RevisionTimeline
                                    revisions={detail.revisionTimeline.map(
                                        (rev) => ({
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
                                                        {
                                                            masterDataCopy.centerHistoryName
                                                        }
                                                        ：
                                                        <strong>
                                                            {rev.nameSnapshot}
                                                        </strong>
                                                    </div>
                                                    <div className="text-muted-foreground">
                                                        {rev.changeReason}
                                                    </div>
                                                    <div className="flex flex-wrap gap-2">
                                                        <Badge variant="outline">
                                                            {rev.timingLabel}
                                                        </Badge>
                                                        <Badge variant="secondary">
                                                            {rev.lifecycleAtRevision ===
                                                            "ENABLED"
                                                                ? "启用"
                                                                : "停用"}
                                                        </Badge>
                                                    </div>
                                                </div>
                                            ),
                                            isCurrent: rev.isCurrent,
                                        }),
                                    )}
                                />
                            </div>
                        </details>
                    </section>
                </>
            ) : null}

            {row.actionBlockers.length > 0 ? (
                <>
                    <Separator />
                    <section className="space-y-2">
                        <details className="group">
                            <summary className="flex cursor-pointer list-none items-center gap-1 text-xs font-medium text-muted-foreground [&::-webkit-details-marker]:hidden">
                                {masterDataCopy.previewActionBlocked}
                                <ChevronDownIcon
                                    className="size-3.5 transition-transform group-open:rotate-180"
                                    aria-hidden
                                />
                            </summary>
                            <ul className="mt-2 space-y-1 text-xs">
                                {row.actionBlockers.map((b) => (
                                    <li key={`${b.action}-${b.code}`}>
                                        <span className="font-medium">
                                            {masterDataActionLabel(b.action)}
                                        </span>
                                        <div className="text-muted-foreground">
                                            {b.message}
                                        </div>
                                    </li>
                                ))}
                            </ul>
                        </details>
                    </section>
                </>
            ) : null}
        </div>
    )
}
