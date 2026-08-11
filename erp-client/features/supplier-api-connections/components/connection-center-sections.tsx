"use client"

import * as React from "react"
import Link from "next/link"
import type { ColumnDef } from "@tanstack/react-table"
import { ExternalLinkIcon, KeyRoundIcon } from "lucide-react"

import {
    BackgroundJobProgress,
    BusinessEmptyState,
    BusinessStatusBadge,
    BusinessTableFrame,
    DataTable,
    surfaceInsetClassName,
} from "@/components/business"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import {
    Card,
    CardContent,
    CardDescription,
    CardHeader,
    CardTitle,
} from "@/components/ui/card"
import {
    Dialog,
    DialogContent,
    DialogDescription,
    DialogFooter,
    DialogHeader,
    DialogTitle,
} from "@/components/ui/dialog"
import type {
    CapabilityCode,
    CapabilityView,
    ConnectionCenterView,
    HealthRecordView,
} from "@/features/supplier-api-connections/types"
import {
    AUDIT_ACTION_LABEL,
    REFERENCE_STATE_LABEL,
} from "@/features/supplier-api-connections/types"
import { formatDateTime } from "@/lib/datetime"
import { freshnessText } from "@/lib/ui-text"
import { cn } from "@/lib/utils"

function OverviewSection({ conn }: { conn: ConnectionCenterView }) {
    return (
        <div className="grid gap-3 lg:grid-cols-2">
            <Card
                size="sm"
                className={cn(surfaceInsetClassName, "shadow-none ring-0")}
            >
                <CardHeader className="rounded-t-lg border-b border-border/30 pb-2">
                    <CardTitle className="text-base">业务身份</CardTitle>
                    <CardDescription>采购主责供应商与业务影响</CardDescription>
                </CardHeader>
                <CardContent className="grid gap-2 text-sm">
                    <Row label="连接代码" value={conn.connectionCode} mono />
                    <Row label="供应商" value={conn.supplier.name} />
                    <Row
                        label="环境"
                        value={
                            <span
                                className={
                                    conn.environment === "PRODUCTION"
                                        ? "font-medium text-destructive"
                                        : undefined
                                }
                            >
                                {conn.environmentLabel}
                                {conn.environment === "PRODUCTION"
                                    ? "（生产）"
                                    : ""}
                            </span>
                        }
                    />
                    <Row
                        label="业务负责人"
                        value={conn.businessOwner?.label ?? "—"}
                    />
                    <Row label="下一步" value={conn.nextStep} />
                </CardContent>
            </Card>
            <Card
                size="sm"
                className={cn(surfaceInsetClassName, "shadow-none ring-0")}
            >
                <CardHeader className="rounded-t-lg border-b border-border/30 pb-2">
                    <CardTitle className="text-base">技术就绪</CardTitle>
                    <CardDescription>地址/密钥引用与适配器</CardDescription>
                </CardHeader>
                <CardContent className="grid gap-2 text-sm">
                    <Row
                        label="地址配置"
                        value={
                            <RefLabel
                                state={conn.safeReferences.endpoint.state}
                                alias={conn.safeReferences.endpoint.alias}
                                version={conn.safeReferences.endpoint.version}
                                visible={conn.safeReferences.endpoint.visible}
                            />
                        }
                    />
                    <Row
                        label="密钥配置"
                        value={
                            <RefLabel
                                state={conn.safeReferences.credential.state}
                                alias={conn.safeReferences.credential.alias}
                                version={conn.safeReferences.credential.version}
                                visible={conn.safeReferences.credential.visible}
                            />
                        }
                    />
                    {conn.adapter?.visible ? (
                        <Row
                            label="适配器"
                            value={`${conn.adapter.code} @ ${conn.adapter.version}`}
                            mono
                        />
                    ) : (
                        <Row label="适配器" value="—" />
                    )}
                    <Row
                        label="技术负责人"
                        value={conn.technicalOwner?.label ?? "—"}
                    />
                    <Row
                        label={freshnessText.catalogSyncAt}
                        value={`${conn.catalog.stateLabel}${
                            conn.catalog.lastSuccessfulAt
                                ? ` · ${formatDateTime(conn.catalog.lastSuccessfulAt, "default")}`
                                : ""
                        }`}
                    />
                </CardContent>
            </Card>
            <Card
                size="sm"
                className={cn(
                    surfaceInsetClassName,
                    "shadow-none ring-0 lg:col-span-2",
                )}
            >
                <CardHeader className="rounded-t-lg border-b border-border/30 pb-2">
                    <CardTitle className="text-base">能力与健康摘要</CardTitle>
                    <CardDescription>
                        连接级能力声明不等于每个商品可用 ·{" "}
                        <Link
                            href="/procurement/supplier-offerings"
                            className="text-primary underline-offset-2 hover:underline"
                        >
                            供应商供给
                        </Link>
                        {" · "}
                        <Link
                            href="/commerce/publications"
                            className="text-primary underline-offset-2 hover:underline"
                        >
                            商品发布
                        </Link>
                    </CardDescription>
                </CardHeader>
                <CardContent className="flex flex-wrap gap-2">
                    {conn.capabilities.map((c) => (
                        <Badge
                            key={c.capabilityCode}
                            variant={
                                c.status === "ENABLED" ? "default" : "secondary"
                            }
                        >
                            {c.capabilityLabel}
                            {c.status === "ENABLED" ? "" : "·停"}
                            {c.verification === "SUCCESS"
                                ? " ✓"
                                : c.verification === "FAILED"
                                  ? " !"
                                  : ""}
                        </Badge>
                    ))}
                    {conn.capabilities.length === 0 ? (
                        <span className="text-sm text-muted-foreground">
                            尚未配置能力
                        </span>
                    ) : null}
                    <p className="w-full text-tiny text-muted-foreground">
                        图例：✓ 验证成功 · ! 验证失败 · 停 能力停用
                    </p>
                </CardContent>
            </Card>
        </div>
    )
}

function CapabilitiesSection({
    conn,
    onOpenConfig,
}: {
    conn: ConnectionCenterView
    onOpenConfig: () => void
}) {
    const columns = React.useMemo<ColumnDef<CapabilityView>[]>(
        () => [
            {
                id: "code",
                accessorFn: (r) => r.capabilityLabel,
                header: "能力",
                meta: { label: "能力", width: "reference" },
                cell: ({ row }) => (
                    <div className="text-sm font-medium">
                        {row.original.capabilityLabel}
                    </div>
                ),
            },
            {
                id: "status",
                header: "能力状态",
                meta: { label: "能力状态", width: "status" },
                cell: ({ row }) => (
                    <BusinessStatusBadge
                        context="list"
                        label={row.original.statusLabel}
                        tone={
                            row.original.status === "ENABLED"
                                ? "success"
                                : "neutral"
                        }
                    />
                ),
            },
            {
                id: "req",
                header: "业务需求确认",
                meta: { label: "业务需求" },
                cell: ({ row }) => (
                    <span className="text-sm">
                        {row.original.businessRequirementLabel}
                    </span>
                ),
            },
            {
                id: "verify",
                header: "验证",
                meta: { label: "验证" },
                cell: ({ row }) => (
                    <span className="text-sm">
                        {row.original.verificationLabel}
                    </span>
                ),
            },
            {
                id: "note",
                header: "边界说明",
                meta: { label: "边界" },
                cell: () => (
                    <span className="text-xs text-muted-foreground">
                        连接级 ≠ 供给级 · 见供应商供给/商品发布
                    </span>
                ),
            },
            {
                id: "actions",
                header: "动作",
                meta: { label: "动作" },
                cell: () => (
                    <span className="text-xs text-muted-foreground">—</span>
                ),
            },
        ],
        [],
    )

    return (
        <div className="space-y-3">
            <Alert>
                <AlertTitle>能力边界</AlertTitle>
                <AlertDescription>
                    下表为<strong>连接级</strong>
                    统一能力声明，不表示每条供给都可用。供给/发布级能力由供应商供给
                    / 商品发布返回。能力启停由系统管理员配置。
                </AlertDescription>
            </Alert>
            <div className="flex justify-end">
                <Button type="button" size="sm" onClick={onOpenConfig}>
                    配置能力
                </Button>
            </div>
            <BusinessTableFrame
                title="能力矩阵"
                description="连接级能力 × 状态 × 业务需求 × 验证；不等于商品级可用"
                table={
                    <DataTable
                        data={conn.capabilities}
                        columns={columns}
                        getRowId={(r) => r.capabilityCode}
                        rowCount={conn.capabilities.length}
                        caption="连接能力矩阵"
                        density="compact"
                        layout="flush"
                        showPagination={false}
                        defaultColumnPinning={{ left: ["code"] }}
                        emptyState={
                            <BusinessEmptyState
                                kind="no-data"
                                className="rounded-lg border-0 bg-transparent p-6 shadow-none ring-0"
                                title="尚未配置能力"
                                description="可配置能力启停；业务需求与验证状态随后端数据返回。"
                            />
                        }
                    />
                }
            />
        </div>
    )
}

function SecuritySection({
    conn,
    onBind,
    onBindEndpoint,
}: {
    conn: ConnectionCenterView
    onBind: () => void
    onBindEndpoint: () => void
}) {
    return (
        <div className="space-y-3">
            <Alert>
                <KeyRoundIcon aria-hidden="true" />
                <AlertTitle>安全配置引用</AlertTitle>
                <AlertDescription>
                    仅显示绑定状态、安全别名与版本。永不展示、复制或导出密钥正文。轮换只能选择密钥管理系统不透明引用。
                </AlertDescription>
            </Alert>
            <div className="grid gap-3 sm:grid-cols-2">
                <Card
                    size="sm"
                    className={cn(surfaceInsetClassName, "shadow-none ring-0")}
                >
                    <CardHeader className="rounded-t-lg border-b border-border/30 pb-2">
                        <CardTitle className="text-base">
                            地址配置引用
                        </CardTitle>
                    </CardHeader>
                    <CardContent className="space-y-2 text-sm">
                        <RefLabel
                            state={conn.safeReferences.endpoint.state}
                            alias={conn.safeReferences.endpoint.alias}
                            version={conn.safeReferences.endpoint.version}
                            visible={conn.safeReferences.endpoint.visible}
                        />
                        <Button
                            type="button"
                            size="sm"
                            onClick={onBindEndpoint}
                        >
                            绑定/轮换地址
                        </Button>
                    </CardContent>
                </Card>
                <Card
                    size="sm"
                    className={cn(surfaceInsetClassName, "shadow-none ring-0")}
                >
                    <CardHeader className="rounded-t-lg border-b border-border/30 pb-2">
                        <CardTitle className="text-base">
                            密钥配置引用
                        </CardTitle>
                    </CardHeader>
                    <CardContent className="space-y-2 text-sm">
                        <RefLabel
                            state={conn.safeReferences.credential.state}
                            alias={conn.safeReferences.credential.alias}
                            version={conn.safeReferences.credential.version}
                            visible={conn.safeReferences.credential.visible}
                        />
                        <Button type="button" size="sm" onClick={onBind}>
                            绑定/轮换引用
                        </Button>
                    </CardContent>
                </Card>
            </div>
        </div>
    )
}

function HealthSection({
    records,
    last,
}: {
    records: HealthRecordView[]
    last?: ConnectionCenterView["lastHealth"]
}) {
    const columns = React.useMemo<ColumnDef<HealthRecordView>[]>(
        () => [
            {
                id: "at",
                accessorFn: (r) => r.at,
                header: "时间",
                meta: { label: "时间" },
                cell: ({ row }) => (
                    <span className="text-sm">
                        {formatDateTime(row.original.at, "default")}
                    </span>
                ),
            },
            {
                id: "type",
                accessorFn: (r) => r.checkType,
                header: "检查类型",
                meta: { label: "检查类型" },
            },
            {
                id: "result",
                header: "结果",
                meta: { label: "结果", width: "status" },
                cell: ({ row }) => (
                    <div className="space-y-0.5">
                        <BusinessStatusBadge
                            context="list"
                            label={row.original.resultLabel}
                            tone={row.original.resultTone}
                        />
                        {row.original.autoRetryStopped ? (
                            <div
                                className="text-tiny text-destructive"
                                role="status"
                            >
                                自动重试已停止
                            </div>
                        ) : null}
                        {row.original.result === "UNKNOWN" ? (
                            <div
                                className="text-tiny text-warning-soft-foreground"
                                role="status"
                            >
                                结果未知 · 不按失败播报
                            </div>
                        ) : null}
                    </div>
                ),
            },
            {
                id: "latency",
                header: "耗时",
                meta: { label: "耗时", numeric: true },
                cell: ({ row }) => (
                    <span className="num text-sm">
                        {row.original.latencyMs != null
                            ? `${row.original.latencyMs} ms`
                            : "—"}
                    </span>
                ),
            },
            {
                id: "job",
                header: "任务号",
                meta: { label: "任务号" },
                cell: ({ row }) => (
                    <span className="font-mono text-xs">
                        {row.original.jobNo ?? "—"}
                    </span>
                ),
            },
            {
                id: "trace",
                header: "追踪号",
                meta: { label: "追踪号" },
                cell: ({ row }) => (
                    <span className="font-mono text-xs">
                        {row.original.traceId ?? "—"}
                    </span>
                ),
            },
            {
                id: "summary",
                header: "摘要",
                meta: { label: "摘要" },
                cell: ({ row }) => (
                    <span className="text-xs text-muted-foreground">
                        {row.original.errorSummary ?? "—"}
                    </span>
                ),
            },
        ],
        [],
    )

    return (
        <div className="space-y-3">
            {last ? (
                <p className="text-sm text-muted-foreground">
                    最近：{formatDateTime(last.at, "default")} ·{" "}
                    {last.resultLabel}
                    {last.autoRetryStopped ? " · 自动重试已停止" : ""}
                </p>
            ) : null}
            <BusinessTableFrame
                title="健康检查记录"
                description="不展示原始密钥与敏感消息内容；结果未知单独文字说明"
                table={
                    <DataTable
                        data={records}
                        columns={columns}
                        getRowId={(r) => r.recordId}
                        rowCount={records.length}
                        caption="健康检查记录"
                        density="compact"
                        layout="flush"
                        manualPagination={false}
                        emptyState={
                            <BusinessEmptyState
                                kind="no-data"
                                className="rounded-lg border-0 bg-transparent p-6 shadow-none ring-0"
                                title="暂无健康记录"
                                description="技术角色可在页头执行健康检查，结果会记录在本页。"
                            />
                        }
                    />
                }
            />
        </div>
    )
}

function CatalogSection({
    conn,
    syncing,
    onSync,
}: {
    conn: ConnectionCenterView
    syncing: boolean
    onSync: () => Promise<void>
}) {
    const progress = conn.catalog.progress
    return (
        <div className="space-y-3">
            <Card
                size="sm"
                className={cn(surfaceInsetClassName, "shadow-none ring-0")}
            >
                <CardHeader className="rounded-t-lg border-b border-border/30 pb-2">
                    <CardTitle className="text-base">目录同步进度</CardTitle>
                    <CardDescription>
                        与连接状态分开展示 ·{" "}
                        <Link
                            href={`/procurement/supplier-offerings?connectionId=${conn.connectionId}`}
                            className="inline-flex items-center gap-1 text-primary underline-offset-2 hover:underline"
                        >
                            打开供应商供给
                            <ExternalLinkIcon
                                className="size-3"
                                aria-hidden="true"
                            />
                        </Link>
                    </CardDescription>
                </CardHeader>
                <CardContent className="space-y-3 text-sm">
                    <Row label="同步状态" value={conn.catalog.stateLabel} />
                    <Row
                        label="最近成功"
                        value={formatDateTime(
                            conn.catalog.lastSuccessfulAt,
                            "default",
                        )}
                    />
                    <Row
                        label="当前任务"
                        value={conn.catalog.activeJobNo ?? "—"}
                        mono
                    />
                    {progress ? (
                        <BackgroundJobProgress
                            mode="partialAllowed"
                            status={progress.status}
                            total={progress.total}
                            completed={progress.completed}
                            succeeded={progress.succeeded}
                            failed={progress.failed}
                            label={`目录同步 ${conn.catalog.activeJobNo ?? ""}`}
                            description="目录同步在后台执行；同来源批次不会重复处理。"
                        />
                    ) : null}
                    <div className="flex flex-wrap gap-2">
                        <Button
                            type="button"
                            size="sm"
                            disabled={syncing}
                            onClick={() => void onSync()}
                        >
                            触发目录同步
                        </Button>
                    </div>
                </CardContent>
            </Card>
        </div>
    )
}

function RelatedSection({ conn }: { conn: ConnectionCenterView }) {
    return (
        <div className="grid gap-3 sm:grid-cols-2 lg:grid-cols-4">
            {[
                {
                    label: "活跃供给",
                    value: conn.relatedImpact.activeOfferings,
                    href: "/procurement/supplier-offerings",
                },
                {
                    label: "生效发布",
                    value: conn.relatedImpact.activePublications,
                    href: "/commerce/publications",
                },
                {
                    label: "待处理订单",
                    value: conn.relatedImpact.openSupplierOrders,
                    href: "/supplier-api/orders",
                },
                {
                    label: "同步任务",
                    value: conn.relatedImpact.activeSyncJobs,
                    href: "/procurement/supplier-offerings",
                },
            ].map((item) => (
                <Card
                    key={item.label}
                    size="sm"
                    className={cn(surfaceInsetClassName, "shadow-none ring-0")}
                >
                    <CardHeader className="pb-1">
                        <CardDescription>{item.label}</CardDescription>
                        <CardTitle className="num text-2xl">
                            {item.value}
                        </CardTitle>
                    </CardHeader>
                    <CardContent>
                        <Link
                            href={item.href}
                            className="text-xs text-primary underline-offset-2 hover:underline"
                        >
                            打开关联页面
                        </Link>
                    </CardContent>
                </Card>
            ))}
            <p className="text-xs text-muted-foreground sm:col-span-2 lg:col-span-4">
                进入相关页面时将重新获取最新状态。
            </p>
        </div>
    )
}

function AuditSection({ conn }: { conn: ConnectionCenterView }) {
    const [expanded, setExpanded] = React.useState(false)
    const events = expanded ? conn.auditEvents : conn.auditEvents.slice(0, 10)
    return (
        <div className="space-y-3">
            <p className="text-sm text-muted-foreground">
                配置变更与业务确认均保留审计记录 ·{" "}
                <Link
                    href={`/system/access-audit?objectId=${conn.connectionId}`}
                    className="text-primary underline-offset-2 hover:underline"
                >
                    打开权限与审计
                </Link>
            </p>
            <ul className="space-y-2">
                {events.map((e) => (
                    <li
                        key={e.eventId}
                        className={cn(
                            surfaceInsetClassName,
                            "px-3 py-2 text-sm",
                        )}
                    >
                        <div className="flex flex-wrap items-center justify-between gap-2">
                            <span className="font-medium">
                                {AUDIT_ACTION_LABEL[e.action] ??
                                    e.summary.split("·")[0]}
                            </span>
                            <span className="text-xs text-muted-foreground">
                                {formatDateTime(e.at, "default")}
                            </span>
                        </div>
                        <p className="text-muted-foreground">{e.summary}</p>
                        <p className="text-xs text-muted-foreground">
                            {e.actor}
                            {e.auditNo ? ` · 审计号 ${e.auditNo}` : ""}
                        </p>
                    </li>
                ))}
                {conn.auditEvents.length === 0 ? (
                    <BusinessEmptyState
                        kind="no-data"
                        className="rounded-lg border-0 bg-transparent p-6 shadow-none ring-0"
                        title="暂无审计事件"
                        description="配置与确认动作会追加审计记录。"
                    />
                ) : null}
            </ul>
            {conn.auditEvents.length > 10 ? (
                <Button
                    type="button"
                    size="sm"
                    variant="ghost"
                    className="text-muted-foreground hover:text-foreground"
                    onClick={() => setExpanded((v) => !v)}
                >
                    {expanded
                        ? "收起"
                        : `查看更多（共 ${conn.auditEvents.length} 条）`}
                </Button>
            ) : null}
        </div>
    )
}

function CapConfigDialog({
    open,
    onOpenChange,
    conn,
    pending,
    onSubmit,
}: {
    open: boolean
    onOpenChange: (o: boolean) => void
    conn: ConnectionCenterView
    pending: boolean
    onSubmit: (
        changes: Array<{ code: CapabilityCode; enabled: boolean }>,
    ) => Promise<void>
}) {
    const [draft, setDraft] = React.useState<Record<string, boolean>>({})

    React.useEffect(() => {
        if (open) {
            const next: Record<string, boolean> = {}
            for (const c of conn.capabilities) {
                next[c.capabilityCode] = c.status === "ENABLED"
            }
            setDraft(next)
        }
    }, [open, conn.capabilities])

    return (
        <Dialog open={open} onOpenChange={onOpenChange}>
            <DialogContent>
                <DialogHeader>
                    <DialogTitle>配置连接能力</DialogTitle>
                    <DialogDescription>
                        由系统管理员统一配置，配置后能力需重新验证；不复用采购确认写入口。
                    </DialogDescription>
                </DialogHeader>
                <div className="max-h-72 space-y-2 overflow-y-auto">
                    {conn.capabilities.map((c) => (
                        <label
                            key={c.capabilityCode}
                            className="flex items-center justify-between gap-2 rounded-lg border px-3 py-2 text-sm"
                        >
                            <span>{c.capabilityLabel}</span>
                            <input
                                type="checkbox"
                                checked={draft[c.capabilityCode] ?? false}
                                onChange={(e) =>
                                    setDraft((d) => ({
                                        ...d,
                                        [c.capabilityCode]: e.target.checked,
                                    }))
                                }
                                aria-label={`${
                                    (draft[c.capabilityCode] ?? false)
                                        ? "停用"
                                        : "启用"
                                } ${c.capabilityLabel}`}
                            />
                        </label>
                    ))}
                </div>
                <DialogFooter>
                    <Button
                        type="button"
                        variant="outline"
                        onClick={() => onOpenChange(false)}
                    >
                        取消
                    </Button>
                    <Button
                        type="button"
                        disabled={pending}
                        onClick={() => {
                            const changes = conn.capabilities
                                .filter(
                                    (c) =>
                                        (draft[c.capabilityCode] ?? false) !==
                                        (c.status === "ENABLED"),
                                )
                                .map((c) => ({
                                    code: c.capabilityCode,
                                    enabled: draft[c.capabilityCode] ?? false,
                                }))
                            if (changes.length === 0) {
                                onOpenChange(false)
                                return
                            }
                            void onSubmit(changes)
                        }}
                    >
                        提交能力配置
                    </Button>
                </DialogFooter>
            </DialogContent>
        </Dialog>
    )
}

function Row({
    label,
    value,
    mono,
}: {
    label: string
    value: React.ReactNode
    mono?: boolean
}) {
    return (
        <div className="flex items-start justify-between gap-3">
            <dt className="shrink-0 text-muted-foreground">{label}</dt>
            <dd className={mono ? "font-mono text-right" : "text-right"}>
                {value}
            </dd>
        </div>
    )
}

function RefLabel({
    state,
    alias,
    version,
    visible,
}: {
    state: "MISSING" | "BOUND" | "ROTATION_DUE"
    alias?: string
    version?: string
    visible: boolean
}) {
    const label = REFERENCE_STATE_LABEL[state]
    return (
        <div
            className="space-y-0.5"
            aria-label={`引用状态 ${label}${
                visible && alias ? ` 别名 ${alias} 版本 ${version}` : ""
            }`}
        >
            <BusinessStatusBadge
                context="list"
                label={label}
                tone={
                    state === "BOUND"
                        ? "success"
                        : state === "ROTATION_DUE"
                          ? "warning"
                          : "neutral"
                }
            />
            {visible && alias ? (
                <div className="font-mono text-xs text-muted-foreground">
                    {alias}
                    {version ? ` · ${version}` : ""}
                </div>
            ) : (
                <div className="text-xs text-muted-foreground">
                    {state === "BOUND"
                        ? "配置已绑定"
                        : state === "ROTATION_DUE"
                          ? "需轮换"
                          : "待绑定"}
                </div>
            )}
        </div>
    )
}

export {
    AuditSection,
    CapabilitiesSection,
    CapConfigDialog,
    CatalogSection,
    HealthSection,
    OverviewSection,
    RelatedSection,
    SecuritySection,
}
