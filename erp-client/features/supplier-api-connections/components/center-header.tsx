"use client"

import { ArrowLeftIcon, RefreshCwIcon } from "lucide-react"

import { DocumentHeader, PageHeader } from "@/components/business"
import { Button } from "@/components/ui/button"
import type { ConnectionCenterView } from "@/features/supplier-api-connections/types"
import { formatDateTime } from "@/lib/datetime"

/** 连接中心页头：文档头与启停/健康动作。 */
export function CenterHeader({
    conn,
    onBack,
    canRunHealth,
    canEnable,
    canDisable,
    healthPending,
    enablePending,
    disablePending,
    healthBlocker,
    enableBlocker,
    disableBlocker,
    onRunHealth,
    onEnable,
    onDisable,
}: {
    conn: ConnectionCenterView
    onBack: () => void
    canRunHealth: boolean
    canEnable: boolean
    canDisable: boolean
    healthPending: boolean
    enablePending: boolean
    disablePending: boolean
    healthBlocker?: { message: string }
    enableBlocker?: { message: string }
    disableBlocker?: { message: string }
    onRunHealth: () => void
    onEnable: () => void
    onDisable: () => void
}) {
    const isProd = conn.environment === "PRODUCTION"
    return (
        <>
            <PageHeader
                variant="object-chrome"
                actions={
                    <Button
                        id="supplier-api-connections-center-back"
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
                        {canRunHealth || healthBlocker ? (
                            <Button
                                id="supplier-api-connections-center-run-health"
                                type="button"
                                size="sm"
                                variant="outline"
                                disabled={!canRunHealth || healthPending}
                                title={healthBlocker?.message}
                                onClick={onRunHealth}
                            >
                                <RefreshCwIcon
                                    className="size-4"
                                    aria-hidden="true"
                                />
                                健康检查
                            </Button>
                        ) : null}
                        {canEnable || enableBlocker ? (
                            <Button
                                id="supplier-api-connections-center-enable"
                                type="button"
                                size="sm"
                                disabled={!canEnable || enablePending}
                                title={enableBlocker?.message}
                                onClick={onEnable}
                            >
                                启用连接
                            </Button>
                        ) : null}
                        {canDisable || disableBlocker ? (
                            <Button
                                id="supplier-api-connections-center-disable"
                                type="button"
                                size="sm"
                                variant="destructive"
                                disabled={!canDisable || disablePending}
                                title={disableBlocker?.message}
                                onClick={onDisable}
                            >
                                停用连接
                            </Button>
                        ) : null}
                    </div>
                }
            />
        </>
    )
}
