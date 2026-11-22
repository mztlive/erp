"use client"

import Link from "next/link"
import { AlertCircleIcon } from "lucide-react"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Button } from "@/components/ui/button"
import { LoadingButton } from "@/components/ui/loading-button"
import {
    Progress,
    ProgressLabel,
    ProgressValue,
} from "@/components/ui/progress"
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
    const total = batch.selected_count
    const percent =
        total > 0
            ? Math.min(100, Math.round((state.completed / total) * 100))
            : 0
    const act = (action: "RETRY" | "CANCEL") =>
        control.mutate({
            connectionId: batch.connection_id,
            batchId: batch.id,
            action,
        })
    return (
        <div className="space-y-4 text-body-compact">
            <div role="status" aria-live="polite" className="space-y-2.5">
                <Progress value={percent} className="w-full max-w-md">
                    <ProgressLabel>{label}</ProgressLabel>
                    <ProgressValue className="num">
                        {() =>
                            `${state.completed} / ${total} 个 SKU（${percent}%）`
                        }
                    </ProgressValue>
                </Progress>
                {running ? (
                    <p className="text-xs text-muted-foreground">
                        可离开页面，稍后从“入库批次”继续。准备完成后可配置入库。
                    </p>
                ) : null}
            </div>
            {state.error ? (
                <Alert variant="destructive">
                    <AlertCircleIcon aria-hidden="true" />
                    <AlertTitle>准备失败</AlertTitle>
                    <AlertDescription>{state.error}</AlertDescription>
                </Alert>
            ) : null}
            <div className="flex flex-wrap items-center gap-2">
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
                    <LoadingButton
                        id={
                            inWorkspace
                                ? "supplier-batch-preparation-retry"
                                : "supplier-catalog-preparation-retry"
                        }
                        size="sm"
                        variant="outline"
                        loading={
                            control.isPending &&
                            control.variables?.action === "RETRY"
                        }
                        disabled={control.isPending}
                        onClick={() => act("RETRY")}
                    >
                        重试准备
                    </LoadingButton>
                ) : null}
                {running || state.status === "FAILED" ? (
                    <LoadingButton
                        id={
                            inWorkspace
                                ? "supplier-batch-preparation-cancel"
                                : "supplier-catalog-preparation-cancel"
                        }
                        size="sm"
                        variant="ghost"
                        loading={
                            control.isPending &&
                            control.variables?.action === "CANCEL"
                        }
                        disabled={control.isPending}
                        onClick={() => act("CANCEL")}
                    >
                        取消准备
                    </LoadingButton>
                ) : null}
            </div>
            {control.error ? (
                <Alert variant="destructive">
                    <AlertCircleIcon aria-hidden="true" />
                    <AlertDescription>{control.error.message}</AlertDescription>
                </Alert>
            ) : null}
        </div>
    )
}
