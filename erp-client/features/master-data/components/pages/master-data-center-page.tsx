"use client"

import * as React from "react"
import Link from "next/link"
import { useRouter } from "next/navigation"
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
import { cn } from "@/lib/utils"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { useAccountProfileQuery } from "@/features/auth/queries"
import {
    MasterDataDisableDialog,
    MasterDataReviseDialog,
} from "@/features/master-data/components/shared/master-data-action-dialog"
import { revealMasterDataSensitive } from "@/features/master-data/api"
import { masterDataCopy } from "@/features/master-data/lib/copy"
import { resourceLabel } from "@/features/master-data/lib/data"
import { formatEffectiveRange } from "@/features/master-data/lib/filter"
import { useMasterDataCenterQuery } from "@/features/master-data/hooks/queries"
import type {
    MasterDataResource,
    MasterDataSectionId,
} from "@/features/master-data/types"
import { formatDateTime } from "@/lib/datetime"
import { hasPermission } from "@/lib/permissions"

const SECTION_NAV: readonly {
    id: MasterDataSectionId
    label: string
}[] = [
    { id: "overview", label: masterDataCopy.centerOverview },
    { id: "versions", label: masterDataCopy.centerVersions },
    { id: "relations", label: masterDataCopy.centerRelations },
    { id: "audit", label: masterDataCopy.centerAudit },
]

function resolveSection(section?: string | null): MasterDataSectionId {
    const found = SECTION_NAV.find((s) => s.id === section)
    return found?.id ?? "overview"
}

export function MasterDataObjectPage({
    resource,
    stableId,
    section,
}: {
    resource: MasterDataResource
    stableId: string
    section?: string
}) {
    const router = useRouter()
    const query = useMasterDataCenterQuery(resource, stableId)
    const accountQuery = useAccountProfileQuery()
    const activeSection = resolveSection(section)
    const [reviseOpen, setReviseOpen] = React.useState(false)
    const [disableOpen, setDisableOpen] = React.useState(false)

    const data = query.data

    React.useEffect(() => {
        if (!data) return
        const el = document.getElementById(`md-section-${activeSection}`)
        if (el) el.scrollIntoView({ block: "start", behavior: "smooth" })
    }, [activeSection, data])

    if (query.isPending) {
        return (
            <PageScaffold>
                <PageHeader
                    title="基础资料详情"
                    description={masterDataCopy.centerLoading}
                />
                <div
                    className="h-40 animate-pulse rounded-lg bg-muted"
                    aria-busy
                />
            </PageScaffold>
        )
    }

    if (query.isError) {
        return (
            <PageScaffold>
                <PageHeader title="基础资料详情" />
                <BusinessFailureState
                    error={query.error}
                    action={
                        <Button
                            type="button"
                            onClick={() => void query.refetch()}
                        >
                            重试
                        </Button>
                    }
                />
            </PageScaffold>
        )
    }

    if (!data) {
        return (
            <PageScaffold>
                <PageHeader
                    title={masterDataCopy.centerMissingTitle}
                    description={masterDataCopy.centerMissingDesc}
                    actions={
                        <Button
                            render={<Link href={`/master-data/${resource}`} />}
                        >
                            {masterDataCopy.actionBackList}
                        </Button>
                    }
                />
            </PageScaffold>
        )
    }

    const listHref = `/master-data/${resource}`
    const baseHref = `/master-data/${resource}/${data.stableId}`
    const canRevise = data.allowedActions.includes("CREATE_REVISION")
    const canDisable = data.allowedActions.includes("DISABLE")
    const canRevealSensitive = hasPermission(
        accountQuery.data?.permissions,
        "supplier_sensitive:reveal",
    )
    const reviseBlocker = data.actionBlockers.find(
        (b) => b.action === "CREATE_REVISION",
    )
    const disableBlocker = data.actionBlockers.find(
        (b) => b.action === "DISABLE",
    )

    return (
        <PageScaffold>
            <PageHeader
                variant="object-chrome"
                breadcrumbs={[
                    { id: "md", label: "基础资料", href: "/master-data" },
                    {
                        id: "resource",
                        label: resourceLabel(resource),
                        href: listHref,
                    },
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
                                onClick: () => {
                                    // SPA 内导航：保留列表滚动与筛选状态
                                    router.push(listHref)
                                },
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
                            onClick={() => setReviseOpen(true)}
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
                            onClick={() => setDisableOpen(true)}
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
                                (f) =>
                                    !data.sensitiveFields.some(
                                        (s) => s.label === f.label,
                                    ),
                            )
                            .map((f) => (
                                <div key={f.label}>
                                    <dt className="text-xs text-muted-foreground">
                                        {f.label}
                                    </dt>
                                    <dd>{f.value}</dd>
                                </div>
                            ))}
                        {data.resourceFacts.map((f) => (
                            <div key={f.label}>
                                <dt className="text-xs text-muted-foreground">
                                    {f.label}
                                </dt>
                                <dd>{f.value}</dd>
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
                        {data.selectorEligibility.map((s) => (
                            <li
                                key={s.context}
                                className="flex flex-wrap items-center gap-2 rounded-md bg-muted/40 px-2 py-1.5 text-sm"
                            >
                                <span>{s.contextLabel}</span>
                                <Badge
                                    variant={
                                        s.eligible ? "success" : "destructive"
                                    }
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
                            {data.auditEvents.map((ev) => (
                                <li
                                    key={ev.id}
                                    className={cn(
                                        surfaceInsetClassName,
                                        "px-3 py-2",
                                    )}
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
                                        <Badge variant="outline">
                                            {ev.action}
                                        </Badge>
                                    </div>
                                    <div className="mt-1 text-muted-foreground">
                                        {ev.detail}
                                    </div>
                                </li>
                            ))}
                        </ul>
                    )}
                </DocumentSection>
            </div>

            <MasterDataReviseDialog
                open={reviseOpen}
                onOpenChange={setReviseOpen}
                resource={resource}
                target={data}
            />
            <MasterDataDisableDialog
                open={disableOpen}
                onOpenChange={setDisableOpen}
                resource={resource}
                target={data}
            />
        </PageScaffold>
    )
}
