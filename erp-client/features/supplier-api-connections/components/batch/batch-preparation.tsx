"use client"

import Link from "next/link"
import { Button } from "@/components/ui/button"
import type { AdoptionBatch } from "../../api/batch"
import { useControlBatchPreparationMutation } from "../../hooks/batch"

export function BatchPreparationStatus({
    batch,
    inWorkspace = false,
}: {
    batch: AdoptionBatch
    inWorkspace?: boolean
}) {
    const control = useControlBatchPreparationMutation()
    const state = batch.preparation
    if (!state || state.status === "READY") return null
    const running = state.status === "QUEUED" || state.status === "PREPARING"
    const label =
        state.status === "FAILED"
            ? "准备失败"
            : state.status === "CANCELLED"
              ? "已取消准备"
              : state.status === "QUEUED"
                ? "等待后台准备"
                : "正在准备商品"
    const act = (action: "RETRY" | "CANCEL") =>
        control.mutate({
            connectionId: batch.connection_id,
            batchId: batch.id,
            action,
        })
    return (
        <div className="flex flex-wrap items-center gap-3 text-body-compact">
            <div role="status" aria-live="polite" className="space-y-1">
                <p className="font-medium">
                    {label} · 已准备 {state.completed} / {batch.selected_count}{" "}
                    个 SKU
                </p>
                {running ? (
                    <p className="text-muted-foreground">
                        可离开页面，稍后从“入库批次”继续。准备完成后可配置入库。
                    </p>
                ) : null}
                {state.error ? (
                    <p className="text-destructive">{state.error}</p>
                ) : null}
            </div>
            <Button
                id={
                    inWorkspace
                        ? "supplier-batch-preparation-return"
                        : "supplier-catalog-preparation-open"
                }
                variant="outline"
                size="sm"
                render={
                    <Link
                        href={
                            inWorkspace
                                ? `/supplier-api/catalog?connectionId=${encodeURIComponent(batch.connection_id)}`
                                : `/supplier-api/catalog/adoptions/${encodeURIComponent(batch.id)}?connectionId=${encodeURIComponent(batch.connection_id)}`
                        }
                    />
                }
            >
                {inWorkspace ? "返回商品库" : "查看批次"}
            </Button>
            {state.status === "FAILED" ? (
                <Button
                    id={
                        inWorkspace
                            ? "supplier-batch-preparation-retry"
                            : "supplier-catalog-preparation-retry"
                    }
                    size="sm"
                    variant="outline"
                    disabled={control.isPending}
                    onClick={() => act("RETRY")}
                >
                    重试准备
                </Button>
            ) : null}
            {running || state.status === "FAILED" ? (
                <Button
                    id={
                        inWorkspace
                            ? "supplier-batch-preparation-cancel"
                            : "supplier-catalog-preparation-cancel"
                    }
                    size="sm"
                    variant="ghost"
                    disabled={control.isPending}
                    onClick={() => act("CANCEL")}
                >
                    取消准备
                </Button>
            ) : null}
            {control.error ? (
                <p role="alert" className="text-destructive">
                    {control.error.message}
                </p>
            ) : null}
        </div>
    )
}
