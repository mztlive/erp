"use client"

import { PermissionGuard } from "@/features/auth/components/permission-guard"

import Link from "next/link"

import {
    DetailWorkspaceSection,
    detailWorkspaceStyles,
    RevisionTimeline,
    SensitiveValue,
} from "@/components/business"
import { Button } from "@/components/ui/button"
import { revealMasterDataSensitive } from "@/features/master-data/api"
import { masterDataRevisionEntry } from "@/features/master-data/components/shared/revision-timeline-item"
import { masterDataCopy } from "@/features/master-data/lib/copy"
import { formatEffectiveRange } from "@/features/master-data/lib/filter"
import type { MasterDataCenterView } from "@/features/master-data/types"
import { formatDateTime } from "@/lib/datetime"

/** 概览：编号、版本、变更原因与各资源字段；敏感字段按权限短时查看。 */
export function ObjectCenterOverviewSection({
    data,
    canRevealSensitive,
}: {
    data: MasterDataCenterView
    canRevealSensitive: boolean
}) {
    return (
        <DetailWorkspaceSection
            id="md-section-overview"
            title={masterDataCopy.centerOverview}
            width="form"
            subtitle={masterDataCopy.centerOverviewDesc}
        >
            <dl className="grid gap-x-8 gap-y-6 text-body-compact sm:grid-cols-2 [&>div]:min-w-0 [&_dd]:mt-1 [&_dd]:break-words [&_dd]:leading-6">
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
                            className="flex flex-wrap items-center gap-2 text-body-compact"
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

            <PermissionGuard permission="stock_balance:list">
                {data.warehouseStockSummary ? (
                    <div className="flex flex-col items-start gap-2 border-t border-border/60 pt-4">
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
                            variant="ghost"
                            className={detailWorkspaceStyles.subtleButton}
                            render={
                                <Link
                                    href={data.warehouseStockSummary.w10Href}
                                />
                            }
                        >
                            打开库存台账
                        </Button>
                    </div>
                ) : null}
            </PermissionGuard>
        </DetailWorkspaceSection>
    )
}

/** 变更历史：每一版的名称、原因与生效期间。 */
export function ObjectCenterVersionsSection({
    data,
}: {
    data: MasterDataCenterView
}) {
    return (
        <DetailWorkspaceSection
            id="md-section-versions"
            title={masterDataCopy.centerVersions}
            width="form"
            subtitle={masterDataCopy.centerVersionsDesc}
        >
            <RevisionTimeline
                revisions={data.revisionTimeline.map((rev) =>
                    masterDataRevisionEntry(rev, {
                        effectiveLabel: formatEffectiveRange(
                            rev.effectiveFrom,
                            rev.effectiveTo,
                        ),
                        showTiming: true,
                    }),
                )}
            />
        </DetailWorkspaceSection>
    )
}

/** 操作记录：新建、更新、停用等审计事件。 */
export function ObjectCenterAuditSection({
    data,
}: {
    data: MasterDataCenterView
}) {
    return (
        <DetailWorkspaceSection
            id="md-section-audit"
            title={masterDataCopy.centerAudit}
            width="form"
            subtitle={masterDataCopy.centerAuditDesc}
        >
            {data.auditEvents.length === 0 ? (
                <p className="text-xs text-muted-foreground">
                    {masterDataCopy.centerNoAudit}
                </p>
            ) : (
                <ul className="divide-y divide-border/60">
                    {data.auditEvents.map((event) => (
                        <li key={event.id} className="py-2 first:pt-0">
                            <div className="flex flex-wrap gap-2">
                                <span className="num text-xs text-muted-foreground">
                                    {formatDateTime(
                                        event.at,
                                        "full",
                                        "passthrough",
                                    )}
                                </span>
                                <span>{event.actor}</span>
                                <span className="text-xs text-muted-foreground">
                                    {event.action}
                                </span>
                            </div>
                            <div className="mt-1 text-muted-foreground">
                                {event.detail}
                            </div>
                        </li>
                    ))}
                </ul>
            )}
        </DetailWorkspaceSection>
    )
}
