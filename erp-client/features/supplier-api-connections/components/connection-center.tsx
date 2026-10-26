"use client"
import * as React from "react"
import {
    BusinessEmptyState,
    BusinessFailureState,
    FormalActionResult,
    PageScaffold,
    ObjectSectionTabs,
    ObjectSectionTabsPanel,
} from "@/components/business"
import { Button } from "@/components/ui/button"
import {
    Card,
    CardHeader,
    CardTitle,
    CardDescription,
    CardContent,
} from "@/components/ui/card"
import { CenterHeader } from "./center-header"
import { CatalogSection } from "./sections/catalog-section"
import { CatalogRecords } from "./sections/catalog-records"
import { CapabilitiesSection } from "./sections/capabilities-section"
import { HealthRecords } from "./sections/health-records"
import { RelatedSection } from "./sections/related-section"
import { CapConfigDialog } from "./dialogs/cap-config-dialog"
import { DisableConnectionDialog } from "./dialogs/disable-connection-dialog"
import { EnableConnectionDialog } from "./dialogs/enable-connection-dialog"
import {
    useConnectionCenterQuery,
    useCatalogQuery,
    useDisableConnectionMutation,
    useEnableConnectionMutation,
    useRunHealthCheckMutation,
    useStartCatalogSyncMutation,
    useUpdateCapabilitiesMutation,
} from "../hooks/queries"
import { catalogRunning } from "../api/catalog"
import { newIdempotencyKey, outcomeToResult } from "../lib/operations"
import {
    SECTION_LABEL,
    SECTIONS,
    type ConnectionSection,
    type FormalOutcome,
} from "../types"
import type { ResultState } from "@/components/business/feedback"

export function ConnectionCenter({
    connectionId,
    section,
    onSectionChange,
    onBack,
}: {
    connectionId: string
    section: ConnectionSection
    onSectionChange: (section: ConnectionSection) => void
    onBack: () => void
}) {
    const centerQuery = useConnectionCenterQuery(connectionId)
    const catalogQuery = useCatalogQuery(connectionId, 1)
    const [result, setResult] = React.useState<ResultState>(null)
    const [disableOpen, setDisableOpen] = React.useState(false)
    const [confirmEnableOpen, setConfirmEnableOpen] = React.useState(false)
    const [capConfigOpen, setCapConfigOpen] = React.useState(false)
    const updateCaps = useUpdateCapabilitiesMutation()
    const runHealth = useRunHealthCheckMutation()
    const startCatalog = useStartCatalogSyncMutation()
    const disableMut = useDisableConnectionMutation()
    const enableMut = useEnableConnectionMutation()
    const conn = centerQuery.data
    const applyOutcome = (outcome: FormalOutcome) =>
        setResult(outcomeToResult(outcome))
    const latestStatus = catalogQuery.data?.jobs[0]?.status
    const isCatalogRunning = catalogRunning(catalogQuery.data?.jobs[0])
    const refreshCenter = centerQuery.refetch
    React.useEffect(() => {
        if (latestStatus && !isCatalogRunning) void refreshCenter()
    }, [latestStatus, isCatalogRunning, refreshCenter])
    if (centerQuery.isPending)
        return (
            <PageScaffold>
                <p role="status">正在读取连接…</p>
            </PageScaffold>
        )
    if (centerQuery.isError)
        return (
            <PageScaffold>
                <BusinessFailureState
                    title="连接加载失败"
                    description={centerQuery.error.message}
                    onRetry={() => void centerQuery.refetch()}
                />
            </PageScaffold>
        )
    if (!conn)
        return (
            <PageScaffold>
                <BusinessEmptyState
                    kind="no-data"
                    title="未找到连接"
                    description="该连接不存在或当前角色无权查看。"
                />
            </PageScaffold>
        )
    const canRunHealth = conn.allowedActions.includes("RUN_HEALTH_CHECK")
    const canEnable = conn.allowedActions.includes("ENABLE")
    const canDisable = conn.allowedActions.includes("DISABLE")
    const enableBlocker = conn.actionBlockers.find((v) => v.action === "ENABLE")
    const disableBlocker = conn.actionBlockers.find(
        (v) => v.action === "DISABLE",
    )
    const canSync =
        conn.allowedActions.includes("START_CATALOG_SYNC") &&
        !catalogRunning(catalogQuery.data?.jobs[0])
    const syncBlocker = conn.actionBlockers.find(
        (v) => v.action === "START_CATALOG_SYNC",
    )?.message
    const checkConnection = async () => {
        const outcome = await runHealth.mutateAsync({
            connectionId,
            expectedVersion: conn.version,
            checkType: "AUTHENTICATION",
            idempotencyKey: newIdempotencyKey("health"),
        })
        applyOutcome(outcome)
    }
    return (
        <PageScaffold className="gap-6">
            <CenterHeader conn={conn} onBack={onBack} />
            {startCatalog.error || runHealth.error ? (
                <p role="alert" className="text-sm text-destructive">
                    {(startCatalog.error ?? runHealth.error)?.message}
                </p>
            ) : null}
            {result && result.status !== "processing" ? (
                <FormalActionResult
                    status={
                        result.status === "failed" ? "rejected" : result.status
                    }
                    title={result.title}
                    description={result.description}
                    reference={result.reference}
                    facts={result.facts}
                />
            ) : null}
            <ObjectSectionTabs
                idPrefix="supplier-api-connections-center"
                value={section}
                onValueChange={(value) =>
                    onSectionChange(value as ConnectionSection)
                }
                items={SECTIONS.map((id) => ({ id, label: SECTION_LABEL[id] }))}
                listLabel="连接详情分区"
            >
                <ObjectSectionTabsPanel value={section}>
                    {section === "catalog" ? (
                        <CatalogSection
                            conn={conn}
                            query={catalogQuery}
                            syncing={
                                startCatalog.isPending ||
                                catalogRunning(catalogQuery.data?.jobs[0])
                            }
                            canSync={canSync}
                            blocker={syncBlocker}
                            onSettings={() => onSectionChange("settings")}
                            onSync={async () => {
                                const outcome = await startCatalog.mutateAsync({
                                    connectionId,
                                    expectedVersion: conn.version,
                                    idempotencyKey:
                                        newIdempotencyKey("catalog"),
                                })
                                applyOutcome(outcome)
                            }}
                        />
                    ) : null}
                    {section === "settings" ? (
                        <div className="space-y-5">
                            <Card>
                                <CardHeader>
                                    <CardTitle>接口配置</CardTitle>
                                    <CardDescription>
                                        {conn.deploymentMessage}
                                    </CardDescription>
                                </CardHeader>
                                <CardContent className="space-y-3">
                                    <p className="text-sm text-muted-foreground">
                                        接口地址和凭据由服务端配置统一管理。配置就绪后可直接同步商品，也可先测试连接。
                                    </p>
                                    <Button
                                        id="supplier-api-connections-center-run-health"
                                        variant="outline"
                                        disabled={
                                            !canRunHealth || runHealth.isPending
                                        }
                                        onClick={() =>
                                            void checkConnection().catch(
                                                () => undefined,
                                            )
                                        }
                                    >
                                        {runHealth.isPending
                                            ? "正在提交…"
                                            : "测试连接"}
                                    </Button>
                                    {conn.lastHealth ? (
                                        <p className="text-sm" role="status">
                                            最近测试：
                                            {conn.lastHealth.resultLabel}
                                            {conn.lastHealth.errorSummary
                                                ? ` · ${conn.lastHealth.errorSummary}`
                                                : ""}
                                        </p>
                                    ) : null}
                                </CardContent>
                            </Card>
                            <Card>
                                <CardHeader>
                                    <CardTitle>业务使用</CardTitle>
                                    <CardDescription>
                                        当前状态：{conn.statusLabel}
                                        。同步商品目录无需启用业务连接；用于业务前需完成能力确认和连接测试。
                                    </CardDescription>
                                </CardHeader>
                                <CardContent className="space-y-3">
                                    <CapabilitiesSection
                                        conn={conn}
                                        onOpenConfig={() =>
                                            setCapConfigOpen(true)
                                        }
                                    />
                                    <div className="flex gap-2">
                                        {canEnable || enableBlocker ? (
                                            <Button
                                                id="supplier-api-connections-center-enable"
                                                disabled={
                                                    !canEnable ||
                                                    enableMut.isPending
                                                }
                                                onClick={() =>
                                                    setConfirmEnableOpen(true)
                                                }
                                            >
                                                启用业务连接
                                            </Button>
                                        ) : null}
                                        {canDisable || disableBlocker ? (
                                            <Button
                                                id="supplier-api-connections-center-disable"
                                                variant="outline"
                                                disabled={
                                                    !canDisable ||
                                                    disableMut.isPending
                                                }
                                                onClick={() =>
                                                    setDisableOpen(true)
                                                }
                                            >
                                                停用业务连接
                                            </Button>
                                        ) : null}
                                    </div>
                                    {conn.status !== "ENABLED" &&
                                    enableBlocker ? (
                                        <p className="text-sm text-muted-foreground">
                                            {enableBlocker.message}
                                        </p>
                                    ) : null}
                                    {conn.status === "ENABLED" &&
                                    disableBlocker ? (
                                        <p className="text-sm text-muted-foreground">
                                            {disableBlocker.message}
                                        </p>
                                    ) : null}
                                </CardContent>
                            </Card>
                            <RelatedSection conn={conn} />
                        </div>
                    ) : null}
                    {section === "records" ? (
                        <div className="space-y-6">
                            <CatalogRecords query={catalogQuery} />
                            <HealthRecords records={conn.healthRecords} />
                        </div>
                    ) : null}
                </ObjectSectionTabsPanel>
            </ObjectSectionTabs>
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

            <EnableConnectionDialog
                open={confirmEnableOpen}
                onOpenChange={setConfirmEnableOpen}
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
