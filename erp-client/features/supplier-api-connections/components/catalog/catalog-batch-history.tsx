"use client"

import { useState } from "react"
import Link from "next/link"
import { ClipboardListIcon } from "lucide-react"
import { listWorkspaceStyles } from "@/components/business/list-workspace"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import {
    Dialog,
    DialogContent,
    DialogDescription,
    DialogFooter,
    DialogHeader,
    DialogTitle,
    DialogTrigger,
} from "@/components/ui/dialog"
import { toAutomationIdSegment } from "@/lib/automation-id"
import type { BatchHistoryEntry, BatchHistoryStatus } from "../../api/batch"
import { useAdoptionBatchesQuery } from "../../hooks/batch"

const statuses = {
    PREPARING: { label: "准备中", variant: "info", priority: 0 },
    PREPARATION_FAILED: { label: "准备失败", variant: "warning", priority: 1 },
    CANCELLED: { label: "已取消", variant: "secondary", priority: 5 },
    PROCESSING: { label: "执行中", variant: "info", priority: 0 },
    PENDING: { label: "待处理", variant: "warning", priority: 1 },
    DRAFT: { label: "草稿", variant: "secondary", priority: 2 },
    SUBMITTED: { label: "已提交", variant: "secondary", priority: 3 },
    COMPLETED: { label: "已完成", variant: "success", priority: 4 },
} as const

function historyStatus(
    batch: BatchHistoryEntry,
): BatchHistoryStatus | "SUBMITTED" {
    // 旧服务未返回摘要时，不把已提交推断为已完成。
    return (
        batch.history_status ??
        (batch.execution_pending
            ? "PROCESSING"
            : batch.submitted
              ? "SUBMITTED"
              : "DRAFT")
    )
}

export function CatalogBatchHistory({
    connectionId,
    supplierName,
}: {
    connectionId: string
    supplierName: string
}) {
    const [open, setOpen] = useState(false)
    const query = useAdoptionBatchesQuery(connectionId, open)
    const batches = [...(query.data ?? [])].sort(
        (left, right) =>
            statuses[historyStatus(left)].priority -
            statuses[historyStatus(right)].priority,
    )
    return (
        <Dialog
            open={open}
            onOpenChange={(next) => {
                setOpen(next)
                if (next) void query.refetch()
            }}
        >
            <DialogTrigger
                id="supplier-catalog-batch-history-toggle"
                render={
                    <Button
                        variant="ghost"
                        className={listWorkspaceStyles.quietButton}
                    />
                }
            >
                <ClipboardListIcon
                    data-icon="inline-start"
                    aria-hidden="true"
                />
                入库批次{query.data ? `（${query.data.length}）` : ""}
            </DialogTrigger>
            <DialogContent
                forceOverlay
                closeButtonId="supplier-catalog-batch-history-close"
                className="flex max-h-[80dvh] flex-col sm:max-w-2xl"
            >
                <DialogHeader className="pr-8">
                    <DialogTitle>入库批次</DialogTitle>
                    <DialogDescription>
                        {supplierName} · 最近 20
                        个批次，执行中、待处理和草稿优先展示。
                    </DialogDescription>
                </DialogHeader>
                <div className="min-h-0 overflow-y-auto">
                    {query.isPending ? (
                        <p role="status" className="py-6 text-muted-foreground">
                            正在读取已保存的批次…
                        </p>
                    ) : null}
                    {query.isError ? (
                        <div
                            role="alert"
                            className="flex flex-wrap items-center justify-between gap-2 py-3"
                        >
                            <p className="text-destructive">
                                入库批次读取失败：{query.error.message}
                            </p>
                            <Button
                                id="supplier-catalog-batch-history-retry"
                                size="sm"
                                variant="outline"
                                onClick={() => void query.refetch()}
                            >
                                重新读取
                            </Button>
                        </div>
                    ) : null}
                    {query.isSuccess && !batches.length ? (
                        <p className="py-6 text-muted-foreground">
                            暂无入库批次。选品后保存的草稿和入库结果会保留在这里。
                        </p>
                    ) : null}
                    <ul className="divide-y">
                        {batches.map((batch) => {
                            const state = historyStatus(batch)
                            const status = statuses[state]
                            return (
                                <li
                                    key={batch.id}
                                    className="flex flex-wrap items-center justify-between gap-3 py-4 first:pt-0 last:pb-0"
                                >
                                    <div className="min-w-0 space-y-2">
                                        <div className="flex flex-wrap items-center gap-2">
                                            <p className="num font-medium break-all">
                                                {batch.batch_no}
                                            </p>
                                            <Badge variant={status.variant}>
                                                {batch.work?.status ===
                                                "RUNNING"
                                                    ? batch.work.phase ===
                                                      "SAVING"
                                                        ? "保存中"
                                                        : "核对中"
                                                    : batch.work?.status ===
                                                        "FAILED"
                                                      ? "处理已中断"
                                                      : status.label}
                                            </Badge>
                                        </div>
                                        <p className="text-xs text-muted-foreground">
                                            {batch.selected_count} 个 SKU
                                            {batch.work &&
                                            batch.work.status !== "COMPLETED"
                                                ? ` · 已处理 ${batch.work.completed} 个`
                                                : ""}
                                            {batch.preparation &&
                                            !batch.selection_complete
                                                ? ` · 已准备 ${batch.preparation.completed} 个`
                                                : ""}
                                        </p>
                                    </div>
                                    <Button
                                        id={`supplier-catalog-batch-history-open-${toAutomationIdSegment(batch.id)}`}
                                        variant="outline"
                                        size="sm"
                                        onClick={() => setOpen(false)}
                                        render={
                                            <Link
                                                href={`/supplier-api/catalog/adoptions/${encodeURIComponent(batch.id)}?connectionId=${encodeURIComponent(connectionId)}`}
                                            />
                                        }
                                    >
                                        {!batch.selection_complete &&
                                        batch.preparation
                                            ? "查看准备进度"
                                            : !batch.submitted
                                              ? "继续配置"
                                              : state === "COMPLETED"
                                                ? "查看结果"
                                                : state === "PENDING"
                                                  ? "查看并处理"
                                                  : "查看进度与结果"}
                                    </Button>
                                </li>
                            )
                        })}
                    </ul>
                </div>
                <DialogFooter>
                    <Button
                        id="supplier-catalog-batch-history-refresh"
                        variant="outline"
                        disabled={query.isFetching}
                        onClick={() => void query.refetch()}
                    >
                        {query.isFetching ? "正在刷新…" : "刷新批次"}
                    </Button>
                </DialogFooter>
            </DialogContent>
        </Dialog>
    )
}
