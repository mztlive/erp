"use client"

import * as React from "react"
import Link from "next/link"
import { BanIcon, InfoIcon, PencilIcon } from "lucide-react"

import {
    BusinessEmptyState,
    BusinessFailureState,
    DetailWorkspaceBody,
    DetailWorkspaceChrome,
    DetailWorkspacePage,
    DetailWorkspaceSection,
    DetailWorkspaceSkeleton,
    DetailWorkspaceTabs,
    detailWorkspaceStyles,
    type DetailWorkspaceBackLink,
} from "@/components/business"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
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

const SECTION_NAV: readonly { id: MasterDataSectionId; label: string }[] = [
    { id: "overview", label: masterDataCopy.centerOverview },
    { id: "versions", label: masterDataCopy.centerVersions },
    { id: "audit", label: masterDataCopy.centerAudit },
]

function resolveSection(section?: string | null): MasterDataSectionId {
    return SECTION_NAV.find((item) => item.id === section)?.id ?? "overview"
}

export type ObjectCenterPrimaryActionConfig = Readonly<{
    action: string
    permission: string
    label: string
    icon?: React.ComponentType<React.SVGProps<SVGSVGElement>>
    blockedMessage?: (message: string) => string
}>

export function ObjectCenterView({
    idPrefix,
    data,
    baseHref,
    section,
    backVisibility,
    primaryActionConfig,
    hideDisableAction = false,
    overviewExtra,
    onBack,
    onRevise,
    onDisable,
    dialogs,
}: {
    idPrefix?: string
    data: MasterDataCenterView
    baseHref: string
    section?: string
    backVisibility?: DetailWorkspaceBackLink["visibility"]
    primaryActionConfig?: ObjectCenterPrimaryActionConfig
    hideDisableAction?: boolean
    overviewExtra?: React.ReactNode
    onBack: () => void
    onRevise: () => void
    onDisable?: () => void
    dialogs: React.ReactNode
}) {
    const prefix = idPrefix ?? "master-data-shared-object-center-view"
    const accountQuery = useAccountProfileQuery()
    const [activeSection, setActiveSection] =
        React.useState<MasterDataSectionId>(() => resolveSection(section))

    React.useEffect(() => {
        setActiveSection(resolveSection(section))
    }, [section])

    const defaultWritePermission =
        data.resource === "categories"
            ? "product_category:update"
            : "warehouse:update"
    const primaryActionKey = primaryActionConfig?.action ?? "CREATE_REVISION"
    const primaryPermission =
        primaryActionConfig?.permission ?? defaultWritePermission
    const primaryLabel =
        primaryActionConfig?.label ?? masterDataCopy.actionUpdate
    const PrimaryIcon = primaryActionConfig?.icon ?? PencilIcon

    const canWritePrimary = hasPermission(
        accountQuery.data?.permissions,
        primaryPermission,
    )
    const canWriteDisable = hasPermission(
        accountQuery.data?.permissions,
        defaultWritePermission,
    )
    const canPrimary = data.allowedActions.includes(primaryActionKey)
    const canDisable =
        !hideDisableAction && data.allowedActions.includes("DISABLE")
    const canRevealSensitive = hasPermission(
        accountQuery.data?.permissions,
        "supplier_sensitive:reveal",
    )
    const primaryBlocker = data.actionBlockers.find(
        (blocker) => blocker.action === primaryActionKey,
    )
    const disableBlocker = !hideDisableAction
        ? data.actionBlockers.find((blocker) => blocker.action === "DISABLE")
        : undefined

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

    const handleSectionChange = React.useCallback(
        (next: string) => {
            const resolved = resolveSection(next)
            setActiveSection(resolved)
            scrollToSection(resolved)
            const nextHref = `${baseHref}${baseHref.includes("?") ? "&" : "?"}section=${resolved}`
            window.history.replaceState(window.history.state, "", nextHref)
        },
        [baseHref, scrollToSection],
    )

    const hasAlerts =
        (canWritePrimary && !canPrimary && Boolean(primaryBlocker)) ||
        (!hideDisableAction &&
            canWriteDisable &&
            !canDisable &&
            Boolean(disableBlocker))

    return (
        <DetailWorkspacePage>
            <DetailWorkspaceChrome
                back={{
                    id: `${prefix}-back`,
                    label: masterDataCopy.actionBackList,
                    visibility: backVisibility,
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
                    canWritePrimary ? (
                        <span
                            title={
                                !canPrimary
                                    ? primaryBlocker?.message
                                    : undefined
                            }
                            className="inline-flex"
                        >
                            <Button
                                id={`${prefix}-revise`}
                                type="button"
                                size="sm"
                                disabled={!canPrimary}
                                onClick={onRevise}
                            >
                                <PrimaryIcon
                                    data-icon="inline-start"
                                    aria-hidden="true"
                                />
                                {primaryLabel}
                            </Button>
                        </span>
                    ) : null
                }
                secondaryActions={
                    !hideDisableAction && canWriteDisable && onDisable ? (
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
                    <DetailWorkspaceTabs
                        value={activeSection}
                        onValueChange={handleSectionChange}
                        ariaLabel="资料分区"
                        items={SECTION_NAV.map((item) => ({
                            id: item.id,
                            label: item.label,
                            triggerId: `${prefix}-nav-${item.id}`,
                        }))}
                    />
                }
            />

            <DetailWorkspaceBody
                alerts={
                    hasAlerts ? (
                        <>
                            {canWritePrimary &&
                            !canPrimary &&
                            primaryBlocker ? (
                                <Alert variant="info">
                                    <InfoIcon aria-hidden="true" />
                                    <AlertTitle>当前不可执行主操作</AlertTitle>
                                    <AlertDescription>
                                        {primaryActionConfig?.blockedMessage
                                            ? primaryActionConfig.blockedMessage(
                                                  primaryBlocker.message,
                                              )
                                            : masterDataCopy.centerUpdateBlocked(
                                                  primaryBlocker.message,
                                              )}
                                    </AlertDescription>
                                </Alert>
                            ) : null}
                            {!hideDisableAction &&
                            canWriteDisable &&
                            !canDisable &&
                            disableBlocker ? (
                                <Alert variant="info">
                                    <InfoIcon aria-hidden="true" />
                                    <AlertTitle>当前不可停用</AlertTitle>
                                    <AlertDescription>
                                        {masterDataCopy.centerDisableBlocked(
                                            disableBlocker.message,
                                        )}
                                    </AlertDescription>
                                </Alert>
                            ) : null}
                        </>
                    ) : undefined
                }
            >
                <ObjectCenterOverviewSection
                    data={data}
                    canRevealSensitive={canRevealSensitive}
                    extra={overviewExtra}
                />
                <ObjectCenterVersionsSection data={data} />
                <ObjectCenterAuditSection data={data} />
            </DetailWorkspaceBody>
            {canWritePrimary || canWriteDisable ? dialogs : null}
        </DetailWorkspacePage>
    )
}

export function ObjectCenterQueryState({
    idPrefix,
    title,
    listHref,
    backVisibility,
    isPending,
    isError,
    error,
    onRetry,
    missing,
}: {
    idPrefix?: string
    title: string
    listHref: string
    backVisibility?: DetailWorkspaceBackLink["visibility"]
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
                <DetailWorkspaceChrome
                    title={title}
                    back={{
                        id: `${prefix}-back`,
                        label: masterDataCopy.actionBackList,
                        visibility: backVisibility,
                        href: listHref,
                    }}
                />
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
                    title={title}
                    back={{
                        id: `${prefix}-back`,
                        label: masterDataCopy.actionBackList,
                        visibility: backVisibility,
                        href: listHref,
                    }}
                />
                <DetailWorkspaceBody>
                    <DetailWorkspaceSection>
                        <BusinessEmptyState
                            kind="no-data"
                            title={masterDataCopy.centerMissingTitle}
                            description={masterDataCopy.centerMissingDesc}
                            action={
                                <Button
                                    id={`${prefix}-back-list`}
                                    render={<Link href={listHref} />}
                                >
                                    {masterDataCopy.actionBackList}
                                </Button>
                            }
                        />
                    </DetailWorkspaceSection>
                </DetailWorkspaceBody>
            </DetailWorkspacePage>
        )
    }
    return null
}
