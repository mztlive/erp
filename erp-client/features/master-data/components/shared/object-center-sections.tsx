"use client"

import * as React from "react"
import Link from "next/link"
import { ArrowUpRightIcon } from "lucide-react"

import { PermissionGuard } from "@/features/auth/components/permission-guard"
import {
    DetailWorkspaceFact,
    DetailWorkspaceFacts,
    DetailWorkspaceSection,
    detailWorkspaceStyles,
    RevisionTimeline,
    SensitiveValue,
} from "@/components/business"
import { Button } from "@/components/ui/button"
import { revealMasterDataSensitive } from "@/features/master-data/api"
import { MasterDataAuditList } from "@/features/master-data/components/shared/audit-event-list"
import { masterDataRevisionEntry } from "@/features/master-data/components/shared/revision-timeline-item"
import { masterDataCopy } from "@/features/master-data/lib/copy"
import { formatEffectiveRange } from "@/features/master-data/lib/filter"
import type { MasterDataCenterView } from "@/features/master-data/types"

const DUPLICATE_CODE_LABELS = new Set<string>(["仓库代码", "分类代码"])

/** 概览：编号、版本、变更原因与各资源字段；敏感字段按权限短时查看。 */
export function ObjectCenterOverviewSection({
    data,
    canRevealSensitive,
    extra,
}: {
    data: MasterDataCenterView
    canRevealSensitive: boolean
    extra?: React.ReactNode
}) {
    const sensitiveLabels = new Set(
        data.sensitiveFields.map((sensitive) => sensitive.label),
    )
    const seenLabels = new Set<string>([
        masterDataCopy.colStableNo,
        masterDataCopy.centerCurrentVersion,
        masterDataCopy.centerChangeReason,
        masterDataCopy.centerActor,
    ])
    const overviewFields: Array<{ label: string; value: string }> = []
    for (const field of [
        ...data.currentRevision.fields,
        ...data.resourceFacts,
    ]) {
        if (sensitiveLabels.has(field.label)) continue
        if (seenLabels.has(field.label)) continue
        if (
            DUPLICATE_CODE_LABELS.has(field.label) &&
            field.value === data.stableNo
        ) {
            continue
        }
        seenLabels.add(field.label)
        overviewFields.push(field)
    }

    return (
        <DetailWorkspaceSection
            id="md-section-overview"
            title={masterDataCopy.centerOverview}
            width="form"
        >
            <DetailWorkspaceFacts columns={2}>
                <DetailWorkspaceFact
                    label={masterDataCopy.colStableNo}
                    value={data.stableNo}
                    numeric
                />
                <DetailWorkspaceFact
                    label={masterDataCopy.centerCurrentVersion}
                    value={`v${data.currentRevision.revisionNo}`}
                    numeric
                />
                <DetailWorkspaceFact
                    label={masterDataCopy.centerChangeReason}
                    value={data.currentRevision.changeReason}
                />
                <DetailWorkspaceFact
                    label={masterDataCopy.centerActor}
                    value={data.currentRevision.actor}
                />
                {overviewFields.map((field) => (
                    <DetailWorkspaceFact
                        key={field.label}
                        label={field.label}
                        value={field.value}
                    />
                ))}
            </DetailWorkspaceFacts>

            {extra}

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
                    <div className="flex flex-wrap items-center justify-between gap-3 border-t border-border/60 pt-4">
                        <div className="space-y-0.5">
                            <p className="text-body-compact font-medium text-foreground">
                                库存台账
                            </p>
                            <p className="text-xs text-muted-foreground">
                                {data.warehouseStockSummary.policyNote}
                            </p>
                        </div>
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
                            <ArrowUpRightIcon
                                data-icon="inline-end"
                                className="size-3.5"
                                aria-hidden="true"
                            />
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
        >
            <MasterDataAuditList events={data.auditEvents} />
        </DetailWorkspaceSection>
    )
}
