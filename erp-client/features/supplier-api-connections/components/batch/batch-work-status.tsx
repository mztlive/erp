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
import { useControlBatchWorkMutation } from "../../hooks/batch"

export function BatchWorkStatus({
    batch,
    inCatalog = false,
}: {
    batch: AdoptionBatch
    inCatalog?: boolean
}) {
    const control = useControlBatchWorkMutation()
    const work = batch.work
    if (!work || work.status === "COMPLETED") return null
    const label =
        work.phase === "SAVING"
            ? "正在保存批量设置"
            : work.phase === "REVIEWING"
              ? "正在核对商品与供给"
              : "正在检查全批一致性"
    const percent =
        work.total > 0
            ? Math.min(100, Math.round((work.completed / work.total) * 100))
            : 0
    return (
        <div
            className={
                inCatalog
                    ? "space-y-4 rounded-lg border p-5"
                    : "flex flex-col gap-4"
            }
        >
            <div role="status" aria-live="polite" className="space-y-2.5">
                <Progress value={percent} className="w-full max-w-md">
                    <ProgressLabel>
                        {work.status === "FAILED" ? "本次处理已中断" : label}
                    </ProgressLabel>
                    <ProgressValue className="num">
                        {() =>
                            `${work.completed} / ${work.total} 个 SKU（${percent}%）`
                        }
                    </ProgressValue>
                </Progress>
                <p className="text-xs text-muted-foreground">
                    本次设置已受理。可离开页面，稍后从“入库批次”查看进度；完成后会继续到对应步骤。
                </p>
            </div>
            {work.error ? (
                <Alert variant="destructive">
                    <AlertCircleIcon aria-hidden="true" />
                    <AlertTitle>处理中断</AlertTitle>
                    <AlertDescription>{work.error}</AlertDescription>
                </Alert>
            ) : null}
            <div className="flex flex-wrap gap-2">
                <Button
                    id="supplier-catalog-batch-work-return"
                    variant="outline"
                    size="sm"
                    render={
                        <Link
                            href={
                                inCatalog
                                    ? `/supplier-api/catalog/adoptions/${encodeURIComponent(batch.id)}?connectionId=${encodeURIComponent(batch.connection_id)}`
                                    : `/supplier-api/catalog?connectionId=${encodeURIComponent(batch.connection_id)}`
                            }
                        />
                    }
                >
                    {inCatalog ? "查看批次" : "返回商品库"}
                </Button>
                {work.status === "FAILED" ? (
                    <>
                        <LoadingButton
                            id="supplier-catalog-batch-work-retry"
                            size="sm"
                            loading={
                                control.isPending &&
                                control.variables?.retry === true
                            }
                            disabled={control.isPending}
                            onClick={() =>
                                control.mutate({
                                    connectionId: batch.connection_id,
                                    batchId: batch.id,
                                    work_id: work.id,
                                    retry: true,
                                })
                            }
                        >
                            继续处理
                        </LoadingButton>
                        {work.phase !== "SAVING" || work.completed === 0 ? (
                            <LoadingButton
                                id="supplier-catalog-batch-work-edit"
                                size="sm"
                                variant="ghost"
                                loading={
                                    control.isPending &&
                                    control.variables?.retry === false
                                }
                                disabled={control.isPending}
                                onClick={() =>
                                    control.mutate({
                                        connectionId: batch.connection_id,
                                        batchId: batch.id,
                                        work_id: work.id,
                                        retry: false,
                                    })
                                }
                            >
                                结束本次处理并返回编辑
                            </LoadingButton>
                        ) : null}
                    </>
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
