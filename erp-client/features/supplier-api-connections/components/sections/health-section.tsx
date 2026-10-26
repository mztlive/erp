"use client"

import {
    CircleAlertIcon,
    CircleCheckIcon,
    CircleHelpIcon,
    Clock3Icon,
    FileTextIcon,
    LayersIcon,
    PackageIcon,
    DatabaseIcon,
} from "lucide-react"

import { BusinessEmptyState } from "@/components/business"
import { Button } from "@/components/ui/button"
import {
    Table,
    TableBody,
    TableCell,
    TableHead,
    TableHeader,
    TableRow,
} from "@/components/ui/table"
import { HealthRecords } from "@/features/supplier-api-connections/components/sections/health-records"
import {
    connectionHealthAdvice,
    HEALTH_PRESENTATION,
} from "@/features/supplier-api-connections/lib/health-presentation"
import type {
    CapabilityView,
    ConnectionCenterView,
    ConnectionSection,
} from "@/features/supplier-api-connections/types"
import { formatDateTime } from "@/lib/datetime"
import { cn } from "@/lib/utils"

const VERIFICATION_PRESENTATION = {
    SUCCESS: {
        label: "验证通过",
        description: "当前能力已通过技术验证。",
        Icon: CircleCheckIcon,
        className: "text-success-soft-foreground",
    },
    FAILED: {
        label: "验证失败",
        description: "请核对能力配置及供应商接口权限。",
        Icon: CircleAlertIcon,
        className: "text-destructive",
    },
    UNVERIFIED: {
        label: "尚未验证",
        description: "当前能力尚无通过验证的结果。",
        Icon: CircleHelpIcon,
        className: "text-muted-foreground",
    },
    STALE: {
        label: "需要重新验证",
        description: "已有验证结果已过期，请重新检查。",
        Icon: Clock3Icon,
        className: "text-warning-soft-foreground",
    },
}

function CapabilityResult({ capability }: { capability: CapabilityView }) {
    const state = VERIFICATION_PRESENTATION[capability.verification]
    const Icon =
        capability.capabilityCode === "CATALOG"
            ? PackageIcon
            : capability.capabilityCode === "STOCK"
              ? DatabaseIcon
              : LayersIcon
    return (
        <TableRow>
            <TableCell className="py-5">
                <span className="flex items-center gap-3 font-medium">
                    <Icon
                        className="size-5 text-muted-foreground"
                        aria-hidden="true"
                    />
                    {capability.capabilityLabel}
                </span>
                {capability.status === "DISABLED" ? (
                    <span className="mt-1 block pl-8 text-xs text-muted-foreground">
                        已停用
                    </span>
                ) : null}
            </TableCell>
            <TableCell className="py-5">
                <span
                    className={cn(
                        "flex items-center gap-2 font-medium",
                        state.className,
                    )}
                >
                    <state.Icon className="size-4" aria-hidden="true" />
                    {state.label}
                </span>
                {capability.verifiedAt ? (
                    <span className="mt-1 block pl-6 text-xs text-muted-foreground">
                        {formatDateTime(capability.verifiedAt, "default")}
                    </span>
                ) : null}
            </TableCell>
            <TableCell className="min-w-48 whitespace-normal py-5 text-muted-foreground">
                {capability.status === "DISABLED"
                    ? "此能力已停用，已有验证结果不代表当前可用。"
                    : state.description}
            </TableCell>
        </TableRow>
    )
}

export function HealthSection({
    conn,
    onSectionChange,
    onRefresh,
}: {
    conn: ConnectionCenterView
    onSectionChange: (section: ConnectionSection) => void
    onRefresh: () => void
}) {
    const result = conn.lastHealth?.result ?? "UNCHECKED"
    const presentation = HEALTH_PRESENTATION[result]
    const advice = connectionHealthAdvice(conn)
    const Icon =
        result === "SUCCESS"
            ? CircleCheckIcon
            : result === "UNCHECKED" || result === "UNKNOWN"
              ? CircleHelpIcon
              : CircleAlertIcon
    const tone =
        result === "SUCCESS"
            ? "text-success-soft-foreground"
            : result === "FAILED" || result === "AUTH_FAILED"
              ? "text-destructive"
              : result === "UNCHECKED"
                ? "text-muted-foreground"
                : "text-warning-soft-foreground"

    return (
        <div className="space-y-6">
            <section
                aria-labelledby="supplier-api-connections-health-title"
                className="space-y-4 py-2"
            >
                <div className="flex items-start gap-3">
                    <Icon
                        className={cn("mt-0.5 size-9 shrink-0", tone)}
                        aria-hidden="true"
                    />
                    <div className="min-w-0 space-y-1">
                        <h2
                            id="supplier-api-connections-health-title"
                            className="text-2xl font-semibold tracking-tight"
                        >
                            {presentation.title}
                        </h2>
                        <p className="text-sm text-muted-foreground">
                            {conn.lastHealth
                                ? `上次检查：${formatDateTime(conn.lastHealth.at, "default")}`
                                : "暂无检查记录"}
                        </p>
                    </div>
                </div>
                <p className="max-w-3xl text-sm leading-7 text-muted-foreground">
                    {conn.lastHealth?.errorSummary ?? presentation.description}
                </p>
            </section>

            <div className="grid min-w-0 gap-4 lg:grid-cols-[minmax(0,1.7fr)_minmax(280px,1fr)]">
                <section
                    aria-labelledby="supplier-api-connections-health-results-title"
                    className="min-w-0 rounded-lg border border-border p-5"
                >
                    <h3
                        id="supplier-api-connections-health-results-title"
                        className="mb-4 text-lg font-semibold"
                    >
                        检查结果
                    </h3>
                    {conn.capabilities.length ? (
                        <Table>
                            <TableHeader>
                                <TableRow>
                                    <TableHead>能力项</TableHead>
                                    <TableHead>验证状态</TableHead>
                                    <TableHead>说明</TableHead>
                                </TableRow>
                            </TableHeader>
                            <TableBody>
                                {conn.capabilities.map((capability) => (
                                    <CapabilityResult
                                        key={capability.capabilityCode}
                                        capability={capability}
                                    />
                                ))}
                            </TableBody>
                        </Table>
                    ) : (
                        <BusinessEmptyState
                            kind="no-data"
                            title="尚未配置能力"
                            description="完成能力配置后，此处展示各项能力的验证状态。"
                            className="border-0 bg-transparent py-8 shadow-none ring-0"
                        />
                    )}
                    <p className="mt-4 text-xs leading-5 text-muted-foreground">
                        各项验证时间独立记录；连接能力验证不代表每件商品均可供应。
                    </p>
                </section>
                <aside
                    aria-labelledby="supplier-api-connections-health-advice-title"
                    className="min-w-0 rounded-lg border border-border bg-muted/40 p-6"
                >
                    <h3
                        id="supplier-api-connections-health-advice-title"
                        className="flex items-center gap-3 text-lg font-semibold"
                    >
                        <FileTextIcon
                            className="size-6 text-muted-foreground"
                            aria-hidden="true"
                        />
                        处理建议
                    </h3>
                    <div className="mt-6 space-y-4">
                        <h4 className="text-base font-semibold">
                            {advice.title}
                        </h4>
                        <p className="text-sm leading-7 text-muted-foreground">
                            {advice.description}
                        </p>
                        {advice.section ? (
                            <Button
                                id="supplier-api-connections-health-advice-open"
                                variant="outline"
                                onClick={() => onSectionChange(advice.section!)}
                            >
                                {advice.actionLabel}
                            </Button>
                        ) : (
                            <Button
                                id="supplier-api-connections-health-refresh"
                                variant="outline"
                                onClick={onRefresh}
                            >
                                刷新检查结果
                            </Button>
                        )}
                    </div>
                </aside>
            </div>
            <div
                id="supplier-api-connections-health-records"
                className="min-w-0 rounded-lg border border-border p-5"
            >
                <HealthRecords records={conn.healthRecords} />
            </div>
        </div>
    )
}
