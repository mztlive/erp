"use client"

import * as React from "react"
import Link from "next/link"
import { ArrowLeftIcon, BanIcon, HistoryIcon } from "lucide-react"

import {
    BusinessFailureState,
    DocumentHeader,
    DocumentSection,
    PageActions,
    PageHeader,
    PageScaffold,
    RevisionTimeline,
    SensitiveValue,
    surfaceInsetClassName,
    surfacePanelClassName,
} from "@/components/business"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { useAccountProfileQuery } from "@/features/auth/queries"
import { revealMasterDataSensitive } from "@/features/master-data/api"
import { masterDataCopy } from "@/features/master-data/lib/copy"
import { formatEffectiveRange } from "@/features/master-data/lib/filter"
import type {
    MasterDataCenterView,
    MasterDataSectionId,
} from "@/features/master-data/types"
import { formatDateTime } from "@/lib/datetime"
import { hasPermission } from "@/lib/permissions"
import { cn } from "@/lib/utils"

const SECTION_NAV: readonly { id: MasterDataSectionId; label: string }[] = [
    { id: "overview", label: masterDataCopy.centerOverview },
    { id: "versions", label: masterDataCopy.centerVersions },
    { id: "relations", label: masterDataCopy.centerRelations },
    { id: "audit", label: masterDataCopy.centerAudit },
]

function resolveSection(section?: string | null): MasterDataSectionId {
    return SECTION_NAV.find((item) => item.id === section)?.id ?? "overview"
}

export function ObjectCenterView({
    data,
    listHref,
    listLabel,
    baseHref,
    section,
    onBack,
    onRevise,
    onDisable,
    dialogs,
}: {
    data: MasterDataCenterView
    listHref: string
    listLabel: string
    baseHref: string
    section?: string
    onBack: () => void
    onRevise: () => void
    onDisable: () => void
    dialogs: React.ReactNode
}) {
    const accountQuery = useAccountProfileQuery()
    const activeSection = resolveSection(section)
    const canRevise = data.allowedActions.includes("CREATE_REVISION")
    const canDisable = data.allowedActions.includes("DISABLE")
    const canRevealSensitive = hasPermission(
        accountQuery.data?.permissions,
        "supplier_sensitive:reveal",
    )
    const reviseBlocker = data.actionBlockers.find(
        (blocker) => blocker.action === "CREATE_REVISION",
    )
    const disableBlocker = data.actionBlockers.find(
        (blocker) => blocker.action === "DISABLE",
    )

    React.useEffect(() => {
        const el = document.getElementById(`md-section-${activeSection}`)
        if (el) el.scrollIntoView({ block: "start", behavior: "smooth" })
    }, [activeSection, data])

    return (
        <PageScaffold>
            <PageHeader
                variant="object-chrome"
                breadcrumbs={[
                    { id: "md", label: "基础资料", href: "/master-data" },
                    { id: "resource", label: listLabel, href: listHref },
                    { id: "object", label: data.name, current: true },
                ]}
                actions={
                    <PageActions
                        actions={[
                            {
                                actionKey: "back",
                                label: masterDataCopy.actionBackList,
                                icon: ArrowLeftIcon,
                                variant: "ghost",
                                onClick: onBack,
                            },
                        ]}
                    />
                }
            />

            <DocumentHeader
                density="compact"
                title={data.name}
                documentNumber={data.stableNo}
                version={data.currentRevision.revisionNo}
                primaryStatus={{
                    label: data.lifecycleStatusLabel,
                    tone: data.lifecycleTone,
                }}
                meta={
                    <span className="num text-muted-foreground">
                        {formatEffectiveRange(
                            data.currentRevision.effectiveFrom,
                            data.currentRevision.effectiveTo,
                        )}
                    </span>
                }
                statuses={[
                    {
                        id: "timing",
                        label: masterDataCopy.centerVersionState,
                        status: {
                            label: data.revisionTimingLabel,
                            tone:
                                data.revisionTiming === "FUTURE"
                                    ? "warning"
                                    : "info",
                        },
                    },
                    ...(data.scheduledLifecycleLabel
                        ? [
                              {
                                  id: "scheduled",
                                  label: masterDataCopy.centerScheduledLifecycle,
                                  status: {
                                      label: data.scheduledLifecycleLabel,
                                      tone: "neutral" as const,
                                  },
                              },
                          ]
                        : []),
                ]}
                primaryAction={
                    <span
                        title={!canRevise ? reviseBlocker?.message : undefined}
                        className="inline-flex"
                    >
                        <Button
                            type="button"
                            size="sm"
                            disabled={!canRevise}
                            onClick={onRevise}
                        >
                            <HistoryIcon
                                data-icon="inline-start"
                                aria-hidden="true"
                            />
                            {masterDataCopy.actionUpdate}
                        </Button>
                    </span>
                }
                secondaryActions={
                    <span
                        title={
                            !canDisable ? disableBlocker?.message : undefined
                        }
                        className="inline-flex"
                    >
                        <Button
                            type="button"
                            size="sm"
                            variant="outline"
                            disabled={!canDisable}
                            onClick={onDisable}
                        >
                            <BanIcon
                                data-icon="inline-start"
                                aria-hidden="true"
                            />
                            {masterDataCopy.actionDisable}
                        </Button>
                    </span>
                }
            />

            {!canRevise && reviseBlocker ? (
                <p className="text-xs text-muted-foreground">
                    {masterDataCopy.centerUpdateBlocked(reviseBlocker.message)}
                </p>
            ) : null}
            {!canDisable && disableBlocker ? (
                <p className="text-xs text-muted-foreground">
                    {masterDataCopy.centerDisableBlocked(
                        disableBlocker.message,
                    )}
                </p>
            ) : null}

            <nav
                aria-label="资料分区"
                className="sticky top-0 z-10 inline-flex max-w-full flex-wrap items-center gap-0.5 rounded-lg bg-muted p-0.5 ring-1 ring-foreground/10"
            >
                {SECTION_NAV.map((item) => {
                    const selected = item.id === activeSection
                    return (
                        <Button
                            key={item.id}
                            size="sm"
                            variant="ghost"
                            className={cn(
                                "h-7 rounded-md px-2.5 text-sm",
                                selected
                                    ? "bg-card font-medium text-foreground shadow-sm ring-1 ring-foreground/10 hover:bg-card"
                                    : "text-muted-foreground hover:bg-foreground/5 hover:text-foreground",
                            )}
                            render={
                                <Link href={`${baseHref}?section=${item.id}`} />
                            }
                        >
                            {item.label}
                        </Button>
                    )
                })}
            </nav>

            <div className={cn(surfacePanelClassName, "space-y-6 p-4 md:p-5")}>
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
                            <dd className="num">
                                v{data.currentRevision.revisionNo}
                            </dd>
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
                                        (sensitive) =>
                                            sensitive.label === field.label,
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
                                size="sm"
                                variant="secondary"
                                className="rounded-lg shadow-none"
                                render={
                                    <Link
                                        href={
                                            data.warehouseStockSummary.w10Href
                                        }
                                    />
                                }
                            >
                                打开库存台账
                            </Button>
                        </div>
                    ) : null}
                </DocumentSection>

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
                                            {rev.lifecycleAtRevision ===
                                            "ENABLED"
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
                                    variant={
                                        item.eligible
                                            ? "success"
                                            : "destructive"
                                    }
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
                                    className={cn(
                                        surfaceInsetClassName,
                                        "px-3 py-2",
                                    )}
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
                                        <Badge variant="outline">
                                            {event.action}
                                        </Badge>
                                    </div>
                                    <div className="mt-1 text-muted-foreground">
                                        {event.detail}
                                    </div>
                                </li>
                            ))}
                        </ul>
                    )}
                </DocumentSection>
            </div>
            {dialogs}
        </PageScaffold>
    )
}

export function ObjectCenterQueryState({
    title,
    listHref,
    isPending,
    isError,
    error,
    onRetry,
    missing,
}: {
    title: string
    listHref: string
    isPending: boolean
    isError: boolean
    error: unknown
    onRetry: () => void
    missing: boolean
}) {
    if (isPending) {
        return (
            <PageScaffold>
                <PageHeader
                    title={title}
                    description={masterDataCopy.centerLoading}
                />
                <div
                    className="h-40 animate-pulse rounded-lg bg-muted"
                    aria-busy
                />
            </PageScaffold>
        )
    }
    if (isError) {
        return (
            <PageScaffold>
                <PageHeader title={title} />
                <BusinessFailureState
                    error={error}
                    action={
                        <Button type="button" onClick={onRetry}>
                            重试
                        </Button>
                    }
                />
            </PageScaffold>
        )
    }
    if (missing) {
        return (
            <PageScaffold>
                <PageHeader
                    title={masterDataCopy.centerMissingTitle}
                    description={masterDataCopy.centerMissingDesc}
                    actions={
                        <Button render={<Link href={listHref} />}>
                            {masterDataCopy.actionBackList}
                        </Button>
                    }
                />
            </PageScaffold>
        )
    }
    return null
}
