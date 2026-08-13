"use client"

import * as React from "react"
import type { ColumnDef } from "@tanstack/react-table"

import { BusinessStatusBadge } from "@/components/business"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { formatObjectSet } from "@/features/import-opening/lib/labels"
import type { ImportBatchListItem } from "@/features/import-opening/types"
import {
    BATCH_STATUS_LABEL,
    BATCH_STATUS_TONE,
    ENVIRONMENT_LABEL,
    PIPELINE_STAGE_LABEL,
} from "@/features/import-opening/types"
import { formatDateTime } from "@/lib/datetime"

export function useBatchListColumns({
    onOpenBatch,
}: {
    onOpenBatch: (batchId: string) => void
}) {
    return React.useMemo<ColumnDef<ImportBatchListItem>[]>(
        () => [
            {
                id: "batchNo",
                header: "批次号",
                cell: ({ row }) => (
                    <Button
                        variant="link"
                        className="h-auto p-0 font-mono text-sm"
                        onClick={() => onOpenBatch(row.original.batchId)}
                    >
                        {row.original.batchNo}
                    </Button>
                ),
            },
            {
                id: "environment",
                header: "环境",
                cell: ({ row }) => (
                    <Badge
                        variant={
                            row.original.environment === "PRODUCTION"
                                ? "destructive"
                                : "secondary"
                        }
                    >
                        {ENVIRONMENT_LABEL[row.original.environment]}
                    </Badge>
                ),
            },
            {
                id: "objects",
                header: "对象集合",
                cell: ({ row }) => (
                    <span className="text-sm">
                        {formatObjectSet(row.original.sourceObjectSet)}
                    </span>
                ),
            },
            {
                id: "baseline",
                header: "基准日",
                cell: ({ row }) => (
                    <span className="num text-sm">
                        {row.original.baselineDate}
                    </span>
                ),
            },
            {
                id: "stage",
                header: "阶段",
                cell: ({ row }) => (
                    <span className="text-sm">
                        {PIPELINE_STAGE_LABEL[row.original.stage]}
                    </span>
                ),
            },
            {
                id: "rule",
                header: "规则版本",
                cell: ({ row }) => (
                    <span className="num font-mono text-xs">
                        {row.original.importRuleVersion}
                    </span>
                ),
            },
            {
                id: "progress",
                header: "进度",
                cell: ({ row }) => (
                    <span className="text-sm">
                        {row.original.progressLabel}
                    </span>
                ),
            },
            {
                id: "confirm",
                header: "责任确认",
                cell: ({ row }) => (
                    <span className="text-sm">
                        {row.original.confirmationSummary}
                    </span>
                ),
            },
            {
                id: "status",
                header: "状态",
                cell: ({ row }) => (
                    <BusinessStatusBadge
                        context="list"
                        label={BATCH_STATUS_LABEL[row.original.status]}
                        tone={BATCH_STATUS_TONE[row.original.status]}
                    />
                ),
            },
            {
                id: "updated",
                header: "更新时间",
                cell: ({ row }) => (
                    <span className="num text-xs text-muted-foreground">
                        {formatDateTime(
                            row.original.updatedAt,
                            "dateStyle",
                            "passthrough",
                        )}
                    </span>
                ),
            },
        ],
        [onOpenBatch],
    )
}
