"use client"

import Link from "next/link"
import { ExternalLinkIcon } from "lucide-react"

import {
    BackgroundJobProgress,
    surfaceInsetClassName,
} from "@/components/business"
import { Button } from "@/components/ui/button"
import {
    Card,
    CardContent,
    CardDescription,
    CardHeader,
    CardTitle,
} from "@/components/ui/card"
import { Row } from "@/features/supplier-api-connections/components/detail-row"
import type { ConnectionCenterView } from "@/features/supplier-api-connections/types"
import { formatDateTime } from "@/lib/datetime"
import { cn } from "@/lib/utils"

export function CatalogSection({
    conn,
    syncing,
    onSync,
}: {
    conn: ConnectionCenterView
    syncing: boolean
    onSync: () => Promise<void>
}) {
    const progress = conn.catalog.progress
    const canSync = conn.allowedActions.includes("START_CATALOG_SYNC")
    const syncBlocker = conn.actionBlockers.find(
        (blocker) => blocker.action === "START_CATALOG_SYNC",
    )
    return (
        <div className="space-y-3">
            <Card
                size="sm"
                className={cn(surfaceInsetClassName, "shadow-none ring-0")}
            >
                <CardHeader className="rounded-t-lg border-b border-grid pb-2">
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
                    {canSync || syncBlocker ? (
                        <div className="flex flex-wrap gap-2">
                            <Button
                                type="button"
                                size="sm"
                                disabled={!canSync || syncing}
                                title={syncBlocker?.message}
                                onClick={() => void onSync()}
                            >
                                触发目录同步
                            </Button>
                        </div>
                    ) : null}
                </CardContent>
            </Card>
        </div>
    )
}
