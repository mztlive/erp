"use client"

import * as React from "react"
import Link from "next/link"
import { ArrowLeftIcon, BanIcon, HistoryIcon } from "lucide-react"

import {
    BusinessFailureState,
    DocumentHeader,
    PageActions,
    PageHeader,
    PageScaffold,
    surfacePanelClassName,
} from "@/components/business"
import { Button } from "@/components/ui/button"
import { useAccountProfileQuery } from "@/features/auth/queries"
import {
    ObjectCenterAuditSection,
    ObjectCenterOverviewSection,
    ObjectCenterRelationsSection,
    ObjectCenterVersionsSection,
} from "@/features/master-data/components/shared/object-center-sections"
import { masterDataCopy } from "@/features/master-data/lib/copy"
import { formatEffectiveRange } from "@/features/master-data/lib/filter"
import type {
    MasterDataCenterView,
    MasterDataSectionId,
} from "@/features/master-data/types"
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
                <ObjectCenterOverviewSection
                    data={data}
                    canRevealSensitive={canRevealSensitive}
                />
                <ObjectCenterVersionsSection data={data} />
                <ObjectCenterRelationsSection data={data} />
                <ObjectCenterAuditSection data={data} />
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
