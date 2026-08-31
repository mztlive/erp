"use client"

import { DataFreshness, PageHeader } from "@/components/business"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { RefreshCwIcon } from "lucide-react"
import type { MallSyncPageView } from "@/features/mall-sync/types"
import { formatDateTime } from "@/lib/datetime"

type MallSyncPageHeaderProps = {
    context: MallSyncPageView["context"] | undefined
    canManualSync: boolean
    manualSyncDisabledReason: string | null
    onOpenIncremental: () => void
    onOpenPull: () => void
    onRefresh: () => void
}

export function MallSyncPageHeader({
    context,
    canManualSync,
    manualSyncDisabledReason,
    onOpenIncremental,
    onOpenPull,
    onRefresh,
}: MallSyncPageHeaderProps) {
    return (
        <PageHeader
            title="商城同步与映射"
            metadata={
                <div className="flex flex-wrap items-center gap-3">
                    <DataFreshness
                        updatedAt={
                            context?.freshness.latestSuccessfulJobAt
                                ? formatDateTime(
                                      context.freshness.latestSuccessfulJobAt,
                                      "default",
                                  )
                                : "—"
                        }
                        dateTime={context?.freshness.latestSuccessfulJobAt}
                        state={context?.sourceUnavailable ? "stale" : "fresh"}
                        label="同步数据"
                    />
                    <Badge variant="outline">
                        {context?.sourceSystem.name} ·{" "}
                        {context?.sourceSystem.environmentLabel}
                    </Badge>
                </div>
            }
            actions={
                <div className="flex flex-wrap items-center gap-2">
                    <Button
                        id="mall-sync-header-incremental"
                        type="button"
                        variant="secondary"
                        size="sm"
                        className="rounded-lg shadow-none"
                        disabled={!canManualSync}
                        title={manualSyncDisabledReason ?? "立即增量（按策略）"}
                        onClick={onOpenIncremental}
                    >
                        立即增量
                    </Button>
                    <Button
                        id="mall-sync-header-pull"
                        type="button"
                        size="sm"
                        disabled={!canManualSync}
                        title={manualSyncDisabledReason ?? "按单号补拉"}
                        onClick={onOpenPull}
                    >
                        按单补拉
                    </Button>
                    <Button
                        id="mall-sync-header-refresh"
                        type="button"
                        variant="ghost"
                        size="sm"
                        className="text-muted-foreground"
                        onClick={onRefresh}
                    >
                        <RefreshCwIcon className="size-4" aria-hidden />
                        刷新
                    </Button>
                </div>
            }
        />
    )
}
