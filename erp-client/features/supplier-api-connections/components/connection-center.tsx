"use client"

import * as React from "react"
import Link from "next/link"
import {
    ArrowLeftIcon,
    KeyRoundIcon,
    RefreshCwIcon,
    ShieldAlertIcon,
    TriangleAlertIcon,
} from "lucide-react"

import {
    BatchImpactPreview,
    BusinessEmptyState,
    BusinessFailureState,
    DocumentHeader,
    FormalActionResult,
    PageHeader,
    PageScaffold,
    surfacePanelClassName,
} from "@/components/business"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Button } from "@/components/ui/button"
import {
    Dialog,
    DialogContent,
    DialogDescription,
    DialogFooter,
    DialogHeader,
    DialogTitle,
} from "@/components/ui/dialog"
import { Label } from "@/components/ui/label"
import { Tabs, TabsList, TabsTrigger } from "@/components/ui/tabs"
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
import { OpaqueReferenceSearchCombobox } from "@/features/supplier-api-connections/components/opaque-reference-search-combobox"
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
import type {
    ConnectionSection,
    FormalOutcome,
} from "@/features/supplier-api-connections/types"
import {
    REFERENCE_STATE_LABEL,
    SECTION_LABEL,
    SECTIONS,
} from "@/features/supplier-api-connections/types"
import { getErrorMessage } from "@/lib/api/errors"
import { formatDateTime } from "@/lib/datetime"
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

    const applyOutcome = (outcome: FormalOutcome) => {
        setResult(outcomeToResult(outcome))
    }

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
    const authFailed = conn.lastHealth?.result === "AUTH_FAILED"
    const resultUnknown = conn.lastHealth?.result === "UNKNOWN"

    return (
        <PageScaffold>
            <PageHeader
                variant="object-chrome"
                breadcrumbs={[
                    {
                        id: "api",
                        label: "供应商 API",
                        href: "/supplier-api/connections",
                    },
                    {
                        id: "conn",
                        label: "API 连接",
                        href: "/supplier-api/connections",
                    },
                    {
                        id: "detail",
                        label: conn.connectionCode,
                        current: true,
                    },
                ]}
                actions={
                    <Button
                        type="button"
                        variant="outline"
                        size="sm"
                        onClick={onBack}
                    >
                        <ArrowLeftIcon className="size-4" aria-hidden="true" />
                        返回列表
                    </Button>
                }
            />

            <DocumentHeader
                density="compact"
                title={`${conn.connectionCode} · ${conn.supplier.name}`}
                documentNumber={conn.connectionCode}
                primaryStatus={{
                    label: conn.statusLabel,
                    tone: conn.statusTone,
                }}
                version={conn.version}
                meta={
                    <span className="inline-flex flex-wrap items-center gap-x-1.5 gap-y-0.5">
                        <span>
                            业务{" "}
                            <span className="font-medium text-foreground">
                                {conn.businessOwner?.label ?? "—"}
                            </span>
                        </span>
                        <span className="text-border" aria-hidden="true">
                            ·
                        </span>
                        <span>
                            技术{" "}
                            <span className="font-medium text-foreground">
                                {conn.technicalOwner?.label ?? "—"}
                            </span>
                        </span>
                        <span className="text-border" aria-hidden="true">
                            ·
                        </span>
                        <span className="text-muted-foreground">
                            配置 {formatDateTime(conn.updatedAt, "default")}
                        </span>
                    </span>
                }
                statuses={[
                    {
                        id: "env",
                        label: "环境",
                        status: {
                            label: conn.environmentLabel,
                            tone: isProd ? "destructive" : "neutral",
                        },
                    },
                    {
                        id: "health",
                        label: "最近健康",
                        status: {
                            label: conn.lastHealth?.resultLabel ?? "未检查",
                            tone:
                                conn.lastHealth?.result === "SUCCESS"
                                    ? "success"
                                    : conn.lastHealth?.result ===
                                            "AUTH_FAILED" ||
                                        conn.lastHealth?.result === "FAILED"
                                      ? "destructive"
                                      : "warning",
                        },
                    },
                ]}
                primaryAction={
                    <div className="flex flex-wrap gap-2">
                        <Button
                            type="button"
                            size="sm"
                            variant="outline"
                            disabled={runHealth.isPending}
                            onClick={() => setConfirmHealthOpen(true)}
                        >
                            <RefreshCwIcon
                                className="size-4"
                                aria-hidden="true"
                            />
                            健康检查
                        </Button>
                        {conn.status !== "ENABLED" ? (
                            <Button
                                type="button"
                                size="sm"
                                disabled={enableMut.isPending}
                                onClick={() => setConfirmEnableOpen(true)}
                            >
                                启用连接
                            </Button>
                        ) : null}
                        {conn.status === "ENABLED" ? (
                            <Button
                                type="button"
                                size="sm"
                                variant="destructive"
                                onClick={() => setDisableOpen(true)}
                            >
                                停用连接
                            </Button>
                        ) : null}
                    </div>
                }
            />

            {isProd ? (
                <Alert variant="warning" role="status">
                    <TriangleAlertIcon aria-hidden="true" />
                    <AlertTitle>生产环境</AlertTitle>
                    <AlertDescription>
                        当前连接运行在生产环境。启停、密钥轮换与全能力检查均需二次确认；检查不会创建真实业务订单。
                    </AlertDescription>
                </Alert>
            ) : null}

            {conn.alerts.map((al) => (
                <Alert
                    key={al.id}
                    variant={
                        al.severity === "destructive"
                            ? "destructive"
                            : al.severity === "warning"
                              ? "warning"
                              : "default"
                    }
                    role="alert"
                >
                    <ShieldAlertIcon aria-hidden="true" />
                    <AlertTitle>{al.title}</AlertTitle>
                    <AlertDescription>{al.description}</AlertDescription>
                </Alert>
            ))}

            {authFailed &&
            !conn.alerts.some((a) => a.title.includes("鉴权")) ? (
                <Alert variant="destructive" role="alert">
                    <ShieldAlertIcon aria-hidden="true" />
                    <AlertTitle>鉴权/签名失败 · 自动重试已停止</AlertTitle>
                    <AlertDescription>
                        {conn.lastHealth?.errorSummary ??
                            "高风险故障。请运维检查密钥引用与适配器；本页不展示密钥正文。"}
                    </AlertDescription>
                </Alert>
            ) : null}

            {resultUnknown ? (
                <Alert variant="warning" role="status" aria-live="polite">
                    <TriangleAlertIcon aria-hidden="true" />
                    <AlertTitle>处理结果待确认</AlertTitle>
                    <AlertDescription>
                        不得按成功或失败处理，不乐观改变启停或引用状态。请按原任务号查询最终结论。
                    </AlertDescription>
                </Alert>
            ) : null}

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
                        className="sticky top-0 z-10 h-auto w-full flex-wrap justify-start gap-1 overflow-x-auto rounded-none border-b border-border/30 bg-card/95 px-3 py-1.5 backdrop-blur supports-backdrop-filter:bg-card/80"
                    >
                        {SECTIONS.map((s) => (
                            <TabsTrigger
                                key={s}
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

            {/* 停用影响预览 */}
            <Dialog open={disableOpen} onOpenChange={setDisableOpen}>
                <DialogContent className="sm:max-w-lg">
                    <DialogHeader>
                        <DialogTitle>
                            {isProd ? "停用生产环境连接" : "停用连接"}
                        </DialogTitle>
                        <DialogDescription>
                            停用改变治理状态，不删除连接、版本和历史业务记录。
                        </DialogDescription>
                    </DialogHeader>
                    <BatchImpactPreview
                        title="停用影响预览"
                        description="请核对发布、待处理订单与同步任务影响。"
                        filterSummary={`${conn.connectionCode} · ${conn.environmentLabel}`}
                        selectionScope={`${conn.supplier.name} · 单一连接`}
                        estimated={
                            conn.relatedImpact.activePublications +
                            conn.relatedImpact.openSupplierOrders +
                            conn.relatedImpact.activeSyncJobs
                        }
                        estimatedLabel="受影响发布/订单/任务"
                        processable={1}
                        processableLabel="连接"
                        skipped={0}
                        background={false}
                        sensitiveFields={["密钥配置", "签名材料"]}
                        skippedReason={undefined}
                    />
                    <dl className="grid gap-2 text-sm sm:grid-cols-3">
                        <div className="rounded-lg border p-3">
                            <dt className="text-xs text-muted-foreground">
                                生效发布
                            </dt>
                            <dd className="num font-medium">
                                {conn.relatedImpact.activePublications}
                            </dd>
                        </div>
                        <div className="rounded-lg border p-3">
                            <dt className="text-xs text-muted-foreground">
                                待处理订单
                            </dt>
                            <dd className="num font-medium">
                                {conn.relatedImpact.openSupplierOrders}
                            </dd>
                        </div>
                        <div className="rounded-lg border p-3">
                            <dt className="text-xs text-muted-foreground">
                                同步任务
                            </dt>
                            <dd className="num font-medium">
                                {conn.relatedImpact.activeSyncJobs}
                            </dd>
                        </div>
                    </dl>
                    <div className="space-y-1 text-xs text-muted-foreground">
                        <p>历史版本与业务记录保留，不会删除任何数据。</p>
                        <p className="flex flex-wrap items-center gap-x-3">
                            替代方案：
                            <Link
                                href="/procurement/supplier-offerings"
                                className="text-primary underline-offset-2 hover:underline"
                            >
                                供应商供给
                            </Link>
                            <Link
                                href="/supplier-api/orders"
                                className="text-primary underline-offset-2 hover:underline"
                            >
                                供应商订单
                            </Link>
                            <Link
                                href="/governance/integration-errors"
                                className="text-primary underline-offset-2 hover:underline"
                            >
                                接口错误中心
                            </Link>
                        </p>
                    </div>
                    <DialogFooter>
                        <Button
                            type="button"
                            variant="outline"
                            onClick={() => setDisableOpen(false)}
                        >
                            取消
                        </Button>
                        <Button
                            type="button"
                            variant="destructive"
                            disabled={disableMut.isPending}
                            onClick={async () => {
                                const outcome = await disableMut.mutateAsync({
                                    connectionId: conn.connectionId,
                                    expectedVersion: conn.version,
                                    reasonCode: "ADMIN_DISABLE",
                                    idempotencyKey:
                                        newIdempotencyKey("disable"),
                                })
                                applyOutcome(outcome)
                                setDisableOpen(false)
                            }}
                        >
                            确认停用
                        </Button>
                    </DialogFooter>
                </DialogContent>
            </Dialog>

            {/* 密钥引用选择器 — 仅不透明引用 */}
            <Dialog open={credOpen} onOpenChange={setCredOpen}>
                <DialogContent>
                    <DialogHeader>
                        <DialogTitle>
                            {isProd
                                ? "轮换生产环境密钥引用"
                                : "绑定/轮换密钥引用"}
                        </DialogTitle>
                        <DialogDescription>
                            只能从密钥管理系统选择不透明引用。无明文密钥输入框；页面、URL
                            与结果均不返回正文。
                        </DialogDescription>
                    </DialogHeader>
                    <div className="space-y-3">
                        {listQuery.isError ? (
                            <Alert variant="destructive" role="alert">
                                <AlertTitle>引用选项加载失败</AlertTitle>
                                <AlertDescription>
                                    {getErrorMessage(
                                        listQuery.error,
                                        "无法取得密钥管理引用列表，请重试后再选择。",
                                    )}
                                </AlertDescription>
                            </Alert>
                        ) : null}
                        <Label htmlFor="opaque-ref">密钥管理引用</Label>
                        <OpaqueReferenceSearchCombobox
                            kind="credential"
                            id="opaque-ref"
                            value={selectedRef || null}
                            onValueChange={(v) => {
                                if (v) setSelectedRef(v)
                            }}
                            placeholder="选择不透明引用"
                            allowClear={false}
                        />
                        <p className="text-xs text-muted-foreground">
                            当前状态：
                            {
                                REFERENCE_STATE_LABEL[
                                    conn.safeReferences.credential.state
                                ]
                            }
                            {conn.safeReferences.credential.alias
                                ? ` · ${conn.safeReferences.credential.alias}`
                                : ""}
                        </p>
                    </div>
                    <DialogFooter>
                        <Button
                            type="button"
                            variant="outline"
                            onClick={() => setCredOpen(false)}
                        >
                            取消
                        </Button>
                        <Button
                            type="button"
                            disabled={!selectedRef || bindCred.isPending}
                            onClick={async () => {
                                const outcome = await bindCred.mutateAsync({
                                    connectionId: conn.connectionId,
                                    opaqueReferenceId: selectedRef,
                                    expectedVersion: conn.version,
                                    idempotencyKey: newIdempotencyKey("cred"),
                                })
                                applyOutcome(outcome)
                                if (outcome.status === "succeeded")
                                    setCredOpen(false)
                            }}
                        >
                            <KeyRoundIcon
                                className="size-4"
                                aria-hidden="true"
                            />
                            确认绑定引用
                        </Button>
                    </DialogFooter>
                </DialogContent>
            </Dialog>

            {/* 地址配置引用选择器 — 仅不透明引用 */}
            <Dialog open={endpointOpen} onOpenChange={setEndpointOpen}>
                <DialogContent>
                    <DialogHeader>
                        <DialogTitle>
                            {isProd
                                ? "轮换生产环境地址引用"
                                : "绑定/轮换地址引用"}
                        </DialogTitle>
                        <DialogDescription>
                            只能从系统提供的地址配置引用中选择，不能自由输入地址。
                        </DialogDescription>
                    </DialogHeader>
                    <div className="space-y-3">
                        {listQuery.isError ? (
                            <Alert variant="destructive" role="alert">
                                <AlertTitle>引用选项加载失败</AlertTitle>
                                <AlertDescription>
                                    {getErrorMessage(
                                        listQuery.error,
                                        "无法取得地址配置引用列表，请重试后再选择。",
                                    )}
                                </AlertDescription>
                            </Alert>
                        ) : null}
                        <Label htmlFor="endpoint-ref">地址配置引用</Label>
                        <OpaqueReferenceSearchCombobox
                            kind="endpoint"
                            id="endpoint-ref"
                            value={selectedEndpointRef || null}
                            onValueChange={(v) => {
                                if (v) setSelectedEndpointRef(v)
                            }}
                            placeholder="选择地址配置引用"
                            allowClear={false}
                        />
                        <p className="text-xs text-muted-foreground">
                            当前状态：
                            {
                                REFERENCE_STATE_LABEL[
                                    conn.safeReferences.endpoint.state
                                ]
                            }
                            {conn.safeReferences.endpoint.alias
                                ? ` · ${conn.safeReferences.endpoint.alias}`
                                : ""}
                        </p>
                    </div>
                    <DialogFooter>
                        <Button
                            type="button"
                            variant="outline"
                            onClick={() => setEndpointOpen(false)}
                        >
                            取消
                        </Button>
                        <Button
                            type="button"
                            disabled={
                                !selectedEndpointRef || bindEndpoint.isPending
                            }
                            onClick={async () => {
                                const outcome = await bindEndpoint.mutateAsync({
                                    connectionId: conn.connectionId,
                                    opaqueReferenceId: selectedEndpointRef,
                                    expectedVersion: conn.version,
                                    idempotencyKey:
                                        newIdempotencyKey("endpoint"),
                                })
                                applyOutcome(outcome)
                                if (outcome.status === "succeeded")
                                    setEndpointOpen(false)
                            }}
                        >
                            <KeyRoundIcon
                                className="size-4"
                                aria-hidden="true"
                            />
                            确认绑定地址
                        </Button>
                    </DialogFooter>
                </DialogContent>
            </Dialog>

            {/* 健康检查确认（生产环境二次确认） */}
            <Dialog
                open={confirmHealthOpen}
                onOpenChange={setConfirmHealthOpen}
            >
                <DialogContent className="sm:max-w-md">
                    <DialogHeader>
                        <DialogTitle>执行健康检查</DialogTitle>
                        <DialogDescription>
                            将对全能力执行健康检查并记录结果。
                            {isProd
                                ? "生产环境检查不会创建真实业务订单。"
                                : "结果可随时在本页健康记录中查看。"}
                        </DialogDescription>
                    </DialogHeader>
                    <DialogFooter>
                        <Button
                            type="button"
                            variant="outline"
                            onClick={() => setConfirmHealthOpen(false)}
                        >
                            取消
                        </Button>
                        <Button
                            type="button"
                            disabled={runHealth.isPending}
                            onClick={async () => {
                                const outcome = await runHealth.mutateAsync({
                                    connectionId: conn.connectionId,
                                    expectedVersion: conn.version,
                                    idempotencyKey: newIdempotencyKey("health"),
                                })
                                applyOutcome(outcome)
                                setConfirmHealthOpen(false)
                            }}
                        >
                            <RefreshCwIcon
                                className="size-4"
                                aria-hidden="true"
                            />
                            确认执行
                        </Button>
                    </DialogFooter>
                </DialogContent>
            </Dialog>

            {/* 启用连接确认（生产环境二次确认） */}
            <Dialog
                open={confirmEnableOpen}
                onOpenChange={setConfirmEnableOpen}
            >
                <DialogContent className="sm:max-w-md">
                    <DialogHeader>
                        <DialogTitle>
                            {isProd ? "启用生产环境连接" : "启用连接"}
                        </DialogTitle>
                        <DialogDescription>
                            启用后连接将恢复对外接口可用，后续下单、查询等业务请求将按能力声明放行。
                            {isProd ? " 生产环境操作需谨慎核对。" : ""}
                        </DialogDescription>
                    </DialogHeader>
                    <DialogFooter>
                        <Button
                            type="button"
                            variant="outline"
                            onClick={() => setConfirmEnableOpen(false)}
                        >
                            取消
                        </Button>
                        <Button
                            type="button"
                            disabled={enableMut.isPending}
                            onClick={async () => {
                                const outcome = await enableMut.mutateAsync({
                                    connectionId: conn.connectionId,
                                    expectedVersion: conn.version,
                                    idempotencyKey: newIdempotencyKey("enable"),
                                })
                                applyOutcome(outcome)
                                setConfirmEnableOpen(false)
                            }}
                        >
                            确认启用
                        </Button>
                    </DialogFooter>
                </DialogContent>
            </Dialog>

            {/* 管理员能力配置 */}
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
