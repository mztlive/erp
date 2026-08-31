"use client"

import Link from "next/link"

import {
    DocumentSection,
    RevisionTimeline,
    SensitiveValue,
    surfaceInsetClassName,
} from "@/components/business"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { revealMasterDataSensitive } from "@/features/master-data/api"
import { masterDataCopy } from "@/features/master-data/lib/copy"
import { formatEffectiveRange } from "@/features/master-data/lib/filter"
import type { MasterDataCenterView } from "@/features/master-data/types"
import { formatDateTime } from "@/lib/datetime"
import { cn } from "@/lib/utils"

/** 概览：编号、版本、变更原因与各资源字段；敏感字段按权限短时查看。 */
export function ObjectCenterOverviewSection({
    data,
    canRevealSensitive,
}: {
    data: MasterDataCenterView
    canRevealSensitive: boolean
}) {
    return (
        <DocumentSection
            id="md-section-overview"
            title={masterDataCopy.centerOverview}
            description={masterDataCopy.centerOverviewDesc}
        >
            <dl className="grid gap-2 text-sm sm:grid-cols-2">
                <div>
                    <dt className="text-xs text-muted-foreground">
                        {masterDataCopy.colStableNo}
                    </dt>
                    <dd className="num">{data.stableNo}</dd>
                </div>
                <div>
                    <dt className="text-xs text-muted-foreground">
                        {masterDataCopy.centerCurrentVersion}
                    </dt>
                    <dd className="num">v{data.currentRevision.revisionNo}</dd>
                </div>
                <div>
                    <dt className="text-xs text-muted-foreground">
                        {masterDataCopy.centerChangeReason}
                    </dt>
                    <dd>{data.currentRevision.changeReason}</dd>
                </div>
                <div>
                    <dt className="text-xs text-muted-foreground">
                        {masterDataCopy.centerActor}
                    </dt>
                    <dd>{data.currentRevision.actor}</dd>
                </div>
                {data.currentRevision.fields
                    .filter(
                        (field) =>
                            !data.sensitiveFields.some(
                                (sensitive) => sensitive.label === field.label,
                            ),
                    )
                    .map((field) => (
                        <div key={field.label}>
                            <dt className="text-xs text-muted-foreground">
                                {field.label}
                            </dt>
                            <dd>{field.value}</dd>
                        </div>
                    ))}
                {data.resourceFacts.map((field) => (
                    <div key={field.label}>
                        <dt className="text-xs text-muted-foreground">
                            {field.label}
                        </dt>
                        <dd>{field.value}</dd>
                    </div>
                ))}
            </dl>

            {data.sensitiveFields.length > 0 ? (
                <div className="mt-4 space-y-2">
                    <h4 className="text-xs font-medium text-muted-foreground">
                        {masterDataCopy.centerSensitive}
                    </h4>
                    {data.sensitiveFields.map((field) => (
                        <div
                            key={field.label}
                            className="flex flex-wrap items-center gap-2 text-sm"
                        >
                            <span className="text-muted-foreground">
                                {field.label}
                            </span>
                            {field.revealToken && canRevealSensitive ? (
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
                        </div>
                    ))}
                </div>
            ) : null}

            {data.warehouseStockSummary ? (
                <div
                    className={cn(
                        surfaceInsetClassName,
                        "mt-4 space-y-2 p-3 text-sm",
                    )}
                >
                    <p className="text-xs text-muted-foreground">
                        {data.warehouseStockSummary.policyNote}
                    </p>
                    <p>
                        在库{" "}
                        <span className="num">
                            {data.warehouseStockSummary.onHandQty}
                        </span>
                        {" · 预占 "}
                        <span className="num">
                            {data.warehouseStockSummary.reservedQty}
                        </span>
                    </p>
                    <Button
                        id="master-data-shared-object-center-sections-button-1"
                        size="sm"
                        variant="secondary"
                        className="rounded-lg shadow-none"
                        render={
                            <Link href={data.warehouseStockSummary.w10Href} />
                        }
                    >
                        打开库存台账
                    </Button>
                </div>
            ) : null}
        </DocumentSection>
    )
}

/** 变更历史：每一版的名称、原因与生效期间。 */
export function ObjectCenterVersionsSection({
    data,
}: {
    data: MasterDataCenterView
}) {
    return (
        <DocumentSection
            id="md-section-versions"
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
                        </div>
                    ),
                    isCurrent: rev.isCurrent,
                }))}
            />
        </DocumentSection>
    )
}

/** 引用与选用：业务引用次数与各业务页可选性。 */
export function ObjectCenterRelationsSection({
    data,
}: {
    data: MasterDataCenterView
}) {
    return (
        <DocumentSection
            id="md-section-relations"
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
                {data.selectorEligibility.map((item) => (
                    <li
                        key={item.context}
                        className="flex flex-wrap items-center gap-2 rounded-md bg-muted/40 px-2 py-1.5 text-sm"
                    >
                        <span>{item.contextLabel}</span>
                        <Badge
                            variant={item.eligible ? "success" : "destructive"}
                        >
                            {item.eligible
                                ? masterDataCopy.eligible
                                : masterDataCopy.ineligible}
                        </Badge>
                        {item.reason ? (
                            <span className="text-xs text-muted-foreground">
                                {item.reason}
                            </span>
                        ) : null}
                    </li>
                ))}
            </ul>
        </DocumentSection>
    )
}

/** 操作记录：新建、更新、停用等审计事件。 */
export function ObjectCenterAuditSection({
    data,
}: {
    data: MasterDataCenterView
}) {
    return (
        <DocumentSection
            id="md-section-audit"
            title={masterDataCopy.centerAudit}
            description={masterDataCopy.centerAuditDesc}
        >
            {data.auditEvents.length === 0 ? (
                <p className="text-sm text-muted-foreground">
                    {masterDataCopy.centerNoAudit}
                </p>
            ) : (
                <ul className="space-y-2 text-sm">
                    {data.auditEvents.map((event) => (
                        <li
                            key={event.id}
                            className={cn(surfaceInsetClassName, "px-3 py-2")}
                        >
                            <div className="flex flex-wrap gap-2">
                                <span className="num text-xs text-muted-foreground">
                                    {formatDateTime(
                                        event.at,
                                        "full",
                                        "passthrough",
                                    )}
                                </span>
                                <span>{event.actor}</span>
                                <Badge variant="outline">{event.action}</Badge>
                            </div>
                            <div className="mt-1 text-muted-foreground">
                                {event.detail}
                            </div>
                        </li>
                    ))}
                </ul>
            )}
        </DocumentSection>
    )
}
