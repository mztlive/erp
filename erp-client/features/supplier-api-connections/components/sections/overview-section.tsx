"use client"

import Link from "next/link"

import { surfaceInsetClassName } from "@/components/business"
import { Badge } from "@/components/ui/badge"
import {
    Card,
    CardContent,
    CardDescription,
    CardHeader,
    CardTitle,
} from "@/components/ui/card"
import { Row } from "@/features/supplier-api-connections/components/detail-row"
import { RefLabel } from "@/features/supplier-api-connections/components/reference-label"
import type { ConnectionCenterView } from "@/features/supplier-api-connections/types"
import { formatDateTime } from "@/lib/datetime"
import { freshnessText } from "@/lib/ui-text"
import { cn } from "@/lib/utils"

export function OverviewSection({ conn }: { conn: ConnectionCenterView }) {
    return (
        <div className="grid gap-3 lg:grid-cols-2">
            <Card
                size="sm"
                className={cn(surfaceInsetClassName, "shadow-none ring-0")}
            >
                <CardHeader className="rounded-t-lg border-b border-grid pb-2">
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
                <CardHeader className="rounded-t-lg border-b border-grid pb-2">
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
                <CardHeader className="rounded-t-lg border-b border-grid pb-2">
                    <CardTitle className="text-base">能力与健康摘要</CardTitle>
                    <CardDescription>
                        连接级能力声明不等于每个商品可用 ·{" "}
                        <Link
                            id="supplier-api-connections-overview-offerings"
                            href="/procurement/supplier-offerings"
                            className="text-primary underline-offset-2 hover:underline"
                        >
                            供应商供给
                        </Link>
                        {" · "}
                        <Link
                            id="supplier-api-connections-overview-publications"
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
