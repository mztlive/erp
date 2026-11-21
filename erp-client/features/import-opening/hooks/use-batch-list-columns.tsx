"use client"

import * as React from "react"
import type { ColumnDef } from "@tanstack/react-table"

import {
    BusinessStatusBadge,
    TableIdentityCell,
    TableTextCell,
} from "@/components/business"
import { Button } from "@/components/ui/button"
import { formatObjectSet } from "@/features/import-opening/lib/labels"
import type { ImportBatchListItem } from "@/features/import-opening/types"
import {
    BATCH_STATUS_LABEL,
    BATCH_STATUS_TONE,
    ENVIRONMENT_LABEL,
    PIPELINE_STAGE_LABEL,
} from "@/features/import-opening/types"
import { toAutomationIdSegment } from "@/lib/automation-id"
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
                meta: { width: "flex" },
                cell: ({ row: { original: batch } }) => (
                    <TableIdentityCell
                        primary={
                            <Button
                                id={`operations-import-batches-row-${toAutomationIdSegment(batch.batchId)}-open`}
                                type="button"
                                variant="link"
                                className="num h-auto max-w-full justify-start p-0"
                                onClick={(event) => {
                                    event.stopPropagation()
                                    onOpenBatch(batch.batchId)
                                }}
                            >
                                <span className="truncate">
                                    {batch.batchNo}
                                </span>
                            </Button>
                        }
                        primaryTitle={batch.batchNo}
                        secondary={ENVIRONMENT_LABEL[batch.environment]}
                    />
                ),
            },
            {
                id: "environment",
                header: "环境",
                meta: { width: "status" },
                cell: ({ row: { original: batch } }) => (
                    <BusinessStatusBadge
                        context="list"
                        tone={
                            batch.environment === "PRODUCTION"
                                ? "destructive"
                                : "neutral"
                        }
                        label={ENVIRONMENT_LABEL[batch.environment]}
                    />
                ),
            },
            {
                id: "objects",
                header: "导入数据类型",
                meta: { width: "default" },
                cell: ({ row: { original: batch } }) => (
                    <TableTextCell
                        value={formatObjectSet(batch.sourceObjectSet)}
                        clamp={2}
                        className="whitespace-normal"
                    />
                ),
            },
            {
                id: "baseline",
                header: "基准日",
                meta: { width: "default" },
                cell: ({ row: { original: batch } }) => (
                    <TableTextCell value={batch.baselineDate} numeric />
                ),
            },
            {
                id: "stage",
                header: "阶段",
                meta: { width: "status" },
                cell: ({ row: { original: batch } }) => (
                    <TableTextCell value={PIPELINE_STAGE_LABEL[batch.stage]} />
                ),
            },
            {
                id: "rule",
                header: "规则版本",
                meta: { width: "default" },
                cell: ({ row: { original: batch } }) => (
                    <TableTextCell
                        value={batch.importRuleVersion}
                        clamp={1}
                        numeric
                    />
                ),
            },
            {
                id: "progress",
                header: "进度",
                meta: { align: "end", numeric: true, width: "quantity" },
                cell: ({ row: { original: batch } }) => (
                    <TableTextCell value={batch.progressLabel} />
                ),
            },
            {
                id: "confirm",
                header: "责任确认",
                meta: { width: "default" },
                cell: ({ row: { original: batch } }) => (
                    <TableTextCell
                        value={
                            batch.confirmationSummary === "—"
                                ? null
                                : batch.confirmationSummary
                        }
                        clamp={2}
                        className="whitespace-normal"
                    />
                ),
            },
            {
                id: "status",
                header: "状态",
                meta: { width: "status" },
                cell: ({ row: { original: batch } }) => (
                    <div className="flex flex-col items-start">
                        <BusinessStatusBadge
                            context="list"
                            label={BATCH_STATUS_LABEL[batch.status]}
                            tone={BATCH_STATUS_TONE[batch.status]}
                            appearance={
                                batch.status === "SUCCEEDED" ? "quiet" : "badge"
                            }
                        />
                        <span className="text-xs text-muted-foreground">
                            {PIPELINE_STAGE_LABEL[batch.stage]}
                        </span>
                    </div>
                ),
            },
            {
                id: "updated",
                header: "更新时间",
                meta: { width: "default" },
                cell: ({ row: { original: batch } }) => (
                    <TableTextCell
                        value={formatDateTime(
                            batch.updatedAt,
                            "dateStyle",
                            "passthrough",
                        )}
                        numeric
                        className="text-muted-foreground"
                    />
                ),
            },
        ],
        [onOpenBatch],
    )
}
