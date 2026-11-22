"use client"

import * as React from "react"
import Link from "next/link"
import { BanIcon, HistoryIcon } from "lucide-react"

import {
    BusinessFailureState,
    DetailWorkspaceBody,
    DetailWorkspaceChrome,
    DetailWorkspacePage,
    DetailWorkspaceSection,
    DetailWorkspaceSkeleton,
    detailWorkspaceStyles,
} from "@/components/business"
import { Button } from "@/components/ui/button"
import { useAccountProfileQuery } from "@/features/auth/queries"
import {
    ObjectCenterAuditSection,
    ObjectCenterOverviewSection,
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
    { id: "audit", label: masterDataCopy.centerAudit },
]

function resolveSection(section?: string | null): MasterDataSectionId {
    return SECTION_NAV.find((item) => item.id === section)?.id ?? "overview"
}

export function ObjectCenterView({
    idPrefix,
    data,
    baseHref,
    section,
    onBack,
    onRevise,
    onDisable,
    dialogs,
}: {
    idPrefix?: string
    data: MasterDataCenterView
    baseHref: string
    section?: string
    onBack: () => void
    onRevise: () => void
    onDisable: () => void
    dialogs: React.ReactNode
}) {
    const prefix = idPrefix ?? "master-data-shared-object-center-view"
    const accountQuery = useAccountProfileQuery()
    const activeSection = resolveSection(section)
    const writePermission =
        data.resource === "categories"
            ? "product_category:update"
            : "warehouse:update"
    const canWrite = hasPermission(
        accountQuery.data?.permissions,
        writePermission,
    )
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

    const scrollToSection = React.useCallback((id: MasterDataSectionId) => {
        const target = document.getElementById(`md-section-${id}`)
        const header = target
            ?.closest(".erp-detail-workspace-page")
            ?.querySelector('[data-slot="detail-workspace-chrome"]')
        if (!target || !header) return
        const main = target.closest("main")
        const scroller =
            main && /^(auto|scroll)$/.test(getComputedStyle(main).overflowY)
                ? main
                : document.scrollingElement
        if (!scroller) return
        const top =
            id === "overview"
                ? 0
                : scroller.scrollTop +
                  target.getBoundingClientRect().top -
                  header.getBoundingClientRect().bottom
        scroller.scrollTo({ top })
    }, [])

    React.useEffect(() => {
        scrollToSection(activeSection)
    }, [activeSection, scrollToSection])

    return (
        <DetailWorkspacePage>
            <DetailWorkspaceChrome
                back={{
                    id: `${prefix}-back`,
                    label: masterDataCopy.actionBackList,
                    onClick: onBack,
                }}
                numberLabel="编号"
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
                    canWrite ? (
                        <span
                            title={
                                !canRevise ? reviseBlocker?.message : undefined
                            }
                            className="inline-flex"
                        >
                            <Button
                                id={`${prefix}-revise`}
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
                    ) : null
                }
                secondaryActions={
                    canWrite ? (
                        <span
                            title={
                                !canDisable
                                    ? disableBlocker?.message
                                    : undefined
                            }
                            className="inline-flex"
                        >
                            <Button
                                id={`${prefix}-disable`}
                                type="button"
                                size="sm"
                                variant="ghost"
                                className={
                                    detailWorkspaceStyles.quietDangerButton
                                }
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
                    ) : null
                }
                tabs={
                    <nav
                        aria-label="资料分区"
                        className={cn(
                            "flex items-center",
                            detailWorkspaceStyles.tabsList,
                        )}
                    >
                        {SECTION_NAV.map((item) => (
                            <Link
                                id={`${prefix}-nav-${item.id}`}
                                key={item.id}
                                href={`${baseHref}${baseHref.includes("?") ? "&" : "?"}section=${item.id}`}
                                scroll={false}
                                aria-current={
                                    item.id === activeSection
                                        ? "location"
                                        : undefined
                                }
                                data-active={
                                    item.id === activeSection ? "" : undefined
                                }
                                className={detailWorkspaceStyles.tab}
                                onClick={(event) => {
                                    if (
                                        item.id === activeSection &&
                                        !event.metaKey &&
                                        !event.ctrlKey &&
                                        !event.shiftKey &&
                                        !event.altKey
                                    ) {
                                        event.preventDefault()
                                        scrollToSection(item.id)
                                    }
                                }}
                            >
                                {item.label}
                            </Link>
                        ))}
                    </nav>
                }
            />

            <DetailWorkspaceBody
                alerts={
                    <>
                        {canWrite && !canRevise && reviseBlocker ? (
                            <p className="text-xs text-muted-foreground">
                                {masterDataCopy.centerUpdateBlocked(
                                    reviseBlocker.message,
                                )}
                            </p>
                        ) : null}
                        {canWrite && !canDisable && disableBlocker ? (
                            <p className="text-xs text-muted-foreground">
                                {masterDataCopy.centerDisableBlocked(
                                    disableBlocker.message,
                                )}
                            </p>
                        ) : null}
                    </>
                }
            >
                <ObjectCenterOverviewSection
                    data={data}
                    canRevealSensitive={canRevealSensitive}
                />
                <ObjectCenterVersionsSection data={data} />
                <ObjectCenterAuditSection data={data} />
            </DetailWorkspaceBody>
            {canWrite ? dialogs : null}
        </DetailWorkspacePage>
    )
}

export function ObjectCenterQueryState({
    idPrefix,
    title,
    listHref,
    isPending,
    isError,
    error,
    onRetry,
    missing,
}: {
    idPrefix?: string
    title: string
    listHref: string
    isPending: boolean
    isError: boolean
    error: unknown
    onRetry: () => void
    missing: boolean
}) {
    const prefix = idPrefix ?? "master-data-shared-object-center-view"
    if (isPending) {
        return (
            <DetailWorkspaceSkeleton
                title={title}
                description={masterDataCopy.centerLoading}
            />
        )
    }
    if (isError) {
        return (
            <DetailWorkspacePage>
                <DetailWorkspaceChrome title={title} />
                <DetailWorkspaceBody>
                    <DetailWorkspaceSection>
                        <BusinessFailureState
                            error={error}
                            action={
                                <Button
                                    id={`${prefix}-retry`}
                                    type="button"
                                    onClick={onRetry}
                                >
                                    重试
                                </Button>
                            }
                        />
                    </DetailWorkspaceSection>
                </DetailWorkspaceBody>
            </DetailWorkspacePage>
        )
    }
    if (missing) {
        return (
            <DetailWorkspacePage>
                <DetailWorkspaceChrome
                    title={masterDataCopy.centerMissingTitle}
                    meta={masterDataCopy.centerMissingDesc}
                    primaryAction={
                        <Button
                            id={`${prefix}-back-list`}
                            render={<Link href={listHref} />}
                        >
                            {masterDataCopy.actionBackList}
                        </Button>
                    }
                />
            </DetailWorkspacePage>
        )
    }
    return null
}
