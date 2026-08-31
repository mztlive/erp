"use client"

import * as React from "react"
import { ArrowLeftIcon } from "lucide-react"

import {
    BusinessEmptyState,
    BusinessFailureState,
    FormalActionResult,
    PageScaffold,
    surfacePanelClassName,
} from "@/components/business"
import { Button } from "@/components/ui/button"
import { Tabs, TabsList, TabsTrigger } from "@/components/ui/tabs"
import { CenterAlerts } from "@/features/supplier-api-connections/components/center-alerts"
import { CenterHeader } from "@/features/supplier-api-connections/components/center-header"
import {
    AuditSection,
    CapabilitiesSection,
    CapConfigDialog,
    CatalogSection,
    HealthSection,
    OverviewSection,
    RelatedSection,
    SecuritySection,
} from "@/features/supplier-api-connections/components/connection-center-sections"
import { DisableConnectionDialog } from "@/features/supplier-api-connections/components/dialogs/disable-connection-dialog"
import { EnableConnectionDialog } from "@/features/supplier-api-connections/components/dialogs/enable-connection-dialog"
import { ReferenceBindDialog } from "@/features/supplier-api-connections/components/dialogs/reference-bind-dialog"
import { RunHealthCheckDialog } from "@/features/supplier-api-connections/components/dialogs/run-health-check-dialog"
import {
    useBindCredentialMutation,
    useBindEndpointMutation,
    useConnectionCenterQuery,
    useConnectionListQuery,
    useDisableConnectionMutation,
    useEnableConnectionMutation,
    useRunHealthCheckMutation,
    useStartCatalogSyncMutation,
    useUpdateCapabilitiesMutation,
} from "@/features/supplier-api-connections/hooks/queries"
import {
    newIdempotencyKey,
    outcomeToResult,
} from "@/features/supplier-api-connections/lib/operations"
import type { ConnectionsUrlState } from "@/features/supplier-api-connections/lib/url-state"
import {
    SECTION_LABEL,
    SECTIONS,
    type ConnectionSection,
    type FormalOutcome,
} from "@/features/supplier-api-connections/types"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { cn } from "@/lib/utils"
import { type ResultState } from "@/components/business/feedback"

export function ConnectionCenter({
    connectionId,
    urlState,
    patchUrl,
    onBack,
}: {
    connectionId: string
    urlState: ConnectionsUrlState
    patchUrl: (patch: Partial<ConnectionsUrlState>) => void
    onBack: () => void
}) {
    const centerQuery = useConnectionCenterQuery(connectionId)
    const [result, setResult] = React.useState<ResultState>(null)
    const [disableOpen, setDisableOpen] = React.useState(false)
    const [credOpen, setCredOpen] = React.useState(false)
    const [endpointOpen, setEndpointOpen] = React.useState(false)
    const [selectedRef, setSelectedRef] = React.useState<string>("")
    const [selectedEndpointRef, setSelectedEndpointRef] =
        React.useState<string>("")
    const [confirmHealthOpen, setConfirmHealthOpen] = React.useState(false)
    const [confirmEnableOpen, setConfirmEnableOpen] = React.useState(false)
    const [capConfigOpen, setCapConfigOpen] = React.useState(false)

    const bindCred = useBindCredentialMutation()
    const bindEndpoint = useBindEndpointMutation()
    const updateCaps = useUpdateCapabilitiesMutation()
    const runHealth = useRunHealthCheckMutation()
    const startCatalog = useStartCatalogSyncMutation()
    const disableMut = useDisableConnectionMutation()
    const enableMut = useEnableConnectionMutation()
    const listQuery = useConnectionListQuery({
        environment: "ALL",
        page: 1,
    })

    const conn = centerQuery.data
    const section = urlState.section

    const applyOutcome = (outcome: FormalOutcome) =>
        setResult(outcomeToResult(outcome))

    if (centerQuery.isPending) {
        return (
            <PageScaffold>
                <div className="h-10 w-40 animate-pulse rounded-lg bg-muted" />
                <div className="h-24 animate-pulse rounded-lg bg-muted" />
                <div className="h-64 animate-pulse rounded-lg bg-muted" />
            </PageScaffold>
        )
    }

    if (centerQuery.isError) {
        return (
            <PageScaffold>
                <BusinessFailureState
                    title="连接详情加载失败"
                    error={centerQuery.error}
                    action={
                        <Button
                            id="supplier-api-connections-center-retry"
                            type="button"
                            onClick={() => void centerQuery.refetch()}
                        >
                            重试
                        </Button>
                    }
                />
            </PageScaffold>
        )
    }

    if (!conn) {
        return (
            <PageScaffold>
                <Button
                    id="supplier-api-connections-center-back-empty"
                    type="button"
                    variant="ghost"
                    size="sm"
                    onClick={onBack}
                >
                    <ArrowLeftIcon className="size-4" aria-hidden="true" />
                    返回列表
                </Button>
                <BusinessEmptyState
                    kind="no-data"
                    className="rounded-lg border-0 bg-transparent p-6 shadow-none ring-0"
                    title="未找到连接"
                    description="该连接不存在或当前角色无权查看。可返回列表重新选择。"
                />
            </PageScaffold>
        )
    }

    const isProd = conn.environment === "PRODUCTION"
    const actionBlocker = (action: string) =>
        conn.actionBlockers.find((blocker) => blocker.action === action)
    const canRunHealth = conn.allowedActions.includes("RUN_HEALTH_CHECK")
    const canEnable = conn.allowedActions.includes("ENABLE")
    const canDisable = conn.allowedActions.includes("DISABLE")
    const healthBlocker = actionBlocker("RUN_HEALTH_CHECK")
    const enableBlocker = actionBlocker("ENABLE")
    const disableBlocker = actionBlocker("DISABLE")

    return (
        <PageScaffold>
            <CenterHeader
                conn={conn}
                onBack={onBack}
                canRunHealth={canRunHealth}
                canEnable={canEnable}
                canDisable={canDisable}
                healthPending={runHealth.isPending}
                enablePending={enableMut.isPending}
                disablePending={disableMut.isPending}
                healthBlocker={healthBlocker}
                enableBlocker={enableBlocker}
                disableBlocker={disableBlocker}
                onRunHealth={() => setConfirmHealthOpen(true)}
                onEnable={() => setConfirmEnableOpen(true)}
                onDisable={() => setDisableOpen(true)}
            />

            <CenterAlerts conn={conn} />

            {result ? (
                <div className="space-y-2">
                    <FormalActionResult
                        status={
                            result.status === "failed"
                                ? "rejected"
                                : result.status === "processing"
                                  ? "processing"
                                  : result.status
                        }
                        title={result.title}
                        description={result.description}
                        reference={result.reference}
                        facts={result.facts}
                    />
                </div>
            ) : null}
            <div
                className={cn(surfacePanelClassName, "min-w-0 overflow-hidden")}
            >
                <Tabs
                    value={section}
                    onValueChange={(v) => {
                        if (v) patchUrl({ section: v as ConnectionSection })
                    }}
                >
                    <TabsList
                        variant="line"
                        className="sticky top-0 z-10 h-auto w-full flex-wrap justify-start gap-1 overflow-x-auto rounded-none border-b border-grid bg-card/95 px-3 py-1.5 backdrop-blur supports-backdrop-filter:bg-card/80"
                    >
                        {SECTIONS.map((s) => (
                            <TabsTrigger
                                key={s}
                                id={`supplier-api-connections-center-tab-${toAutomationIdSegment(s)}`}
                                value={s}
                                className="text-xs sm:text-sm"
                            >
                                {SECTION_LABEL[s]}
                            </TabsTrigger>
                        ))}
                    </TabsList>
                </Tabs>

                <div className="space-y-4 p-3 md:p-4">
                    {section === "overview" ? (
                        <OverviewSection conn={conn} />
                    ) : null}
                    {section === "capabilities" ? (
                        <CapabilitiesSection
                            conn={conn}
                            onOpenConfig={() => setCapConfigOpen(true)}
                        />
                    ) : null}
                    {section === "security" ? (
                        <SecuritySection
                            conn={conn}
                            onBind={() => {
                                setSelectedRef("")
                                setCredOpen(true)
                            }}
                            onBindEndpoint={() => {
                                setSelectedEndpointRef("")
                                setEndpointOpen(true)
                            }}
                        />
                    ) : null}
                    {section === "health" ? (
                        <HealthSection
                            records={conn.healthRecords}
                            last={conn.lastHealth}
                        />
                    ) : null}
                    {section === "catalog" ? (
                        <CatalogSection
                            conn={conn}
                            syncing={startCatalog.isPending}
                            onSync={async () => {
                                const outcome = await startCatalog.mutateAsync({
                                    connectionId: conn.connectionId,
                                    expectedVersion: conn.version,
                                    idempotencyKey:
                                        newIdempotencyKey("catalog"),
                                })
                                applyOutcome(outcome)
                            }}
                        />
                    ) : null}
                    {section === "related" ? (
                        <RelatedSection conn={conn} />
                    ) : null}
                    {section === "audit" ? <AuditSection conn={conn} /> : null}
                </div>
            </div>

            <DisableConnectionDialog
                open={disableOpen}
                onOpenChange={setDisableOpen}
                conn={conn}
                canDisable={canDisable}
                pending={disableMut.isPending}
                onSubmit={async () => {
                    const outcome = await disableMut.mutateAsync({
                        connectionId: conn.connectionId,
                        expectedVersion: conn.version,
                        reasonCode: "ADMIN_DISABLE",
                        idempotencyKey: newIdempotencyKey("disable"),
                    })
                    applyOutcome(outcome)
                    setDisableOpen(false)
                }}
            />

            <ReferenceBindDialog
                kind="credential"
                open={credOpen}
                onOpenChange={setCredOpen}
                conn={conn}
                optionsError={listQuery.isError ? listQuery.error : undefined}
                value={selectedRef}
                onValueChange={setSelectedRef}
                allowed={conn.allowedActions.includes(
                    "BIND_CREDENTIAL_REFERENCE",
                )}
                pending={bindCred.isPending}
                onSubmit={async () => {
                    const outcome = await bindCred.mutateAsync({
                        connectionId: conn.connectionId,
                        opaqueReferenceId: selectedRef,
                        expectedVersion: conn.version,
                        idempotencyKey: newIdempotencyKey("cred"),
                    })
                    applyOutcome(outcome)
                    if (outcome.status === "succeeded") setCredOpen(false)
                }}
            />

            <ReferenceBindDialog
                kind="endpoint"
                open={endpointOpen}
                onOpenChange={setEndpointOpen}
                conn={conn}
                optionsError={listQuery.isError ? listQuery.error : undefined}
                value={selectedEndpointRef}
                onValueChange={setSelectedEndpointRef}
                allowed={conn.allowedActions.includes(
                    "BIND_ENDPOINT_REFERENCE",
                )}
                pending={bindEndpoint.isPending}
                onSubmit={async () => {
                    const outcome = await bindEndpoint.mutateAsync({
                        connectionId: conn.connectionId,
                        opaqueReferenceId: selectedEndpointRef,
                        expectedVersion: conn.version,
                        idempotencyKey: newIdempotencyKey("endpoint"),
                    })
                    applyOutcome(outcome)
                    if (outcome.status === "succeeded") setEndpointOpen(false)
                }}
            />

            <RunHealthCheckDialog
                open={confirmHealthOpen}
                onOpenChange={setConfirmHealthOpen}
                isProd={isProd}
                canRunHealth={canRunHealth}
                pending={runHealth.isPending}
                onSubmit={async () => {
                    const outcome = await runHealth.mutateAsync({
                        connectionId: conn.connectionId,
                        expectedVersion: conn.version,
                        idempotencyKey: newIdempotencyKey("health"),
                        checkType: conn.healthCheckTypes[0]!,
                    })
                    applyOutcome(outcome)
                    setConfirmHealthOpen(false)
                }}
            />

            <EnableConnectionDialog
                open={confirmEnableOpen}
                onOpenChange={setConfirmEnableOpen}
                isProd={isProd}
                canEnable={canEnable}
                pending={enableMut.isPending}
                onSubmit={async () => {
                    const outcome = await enableMut.mutateAsync({
                        connectionId: conn.connectionId,
                        expectedVersion: conn.version,
                        idempotencyKey: newIdempotencyKey("enable"),
                    })
                    applyOutcome(outcome)
                    setConfirmEnableOpen(false)
                }}
            />

            <CapConfigDialog
                open={capConfigOpen}
                onOpenChange={setCapConfigOpen}
                conn={conn}
                pending={updateCaps.isPending}
                onSubmit={async (changes) => {
                    const expectedCapabilityVersions: Record<string, string> =
                        {}
                    for (const c of conn.capabilities) {
                        expectedCapabilityVersions[c.capabilityCode] = c.version
                    }
                    const outcome = await updateCaps.mutateAsync({
                        connectionId: conn.connectionId,
                        changes,
                        expectedConnectionVersion: conn.version,
                        expectedCapabilityVersions,
                        reasonCode: "ADMIN_CONFIG",
                        operationId: newIdempotencyKey("op_cap"),
                        idempotencyKey: newIdempotencyKey("cap"),
                    })
                    applyOutcome(outcome)
                    if (outcome.status === "succeeded") setCapConfigOpen(false)
                }}
            />
        </PageScaffold>
    )
}
