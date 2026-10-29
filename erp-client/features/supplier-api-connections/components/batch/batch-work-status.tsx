"use client"

import Link from "next/link"
import { Button } from "@/components/ui/button"
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
    return (
        <div className="space-y-4 rounded-lg border p-5">
            <div role="status" aria-live="polite" className="space-y-2">
                <p className="font-medium">
                    {work.status === "FAILED" ? "本次处理已中断" : label}
                </p>
                <p>
                    已处理 {work.completed} / {work.total} 个 SKU
                </p>
                <p className="text-sm text-muted-foreground">
                    本次设置已受理。可离开页面，稍后从“入库批次”查看进度；完成后会继续到对应步骤。
                </p>
                {work.error ? (
                    <p className="text-sm text-destructive">{work.error}</p>
                ) : null}
            </div>
            <div className="flex flex-wrap gap-2">
                <Button
                    id="supplier-catalog-batch-work-return"
                    variant="outline"
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
                        <Button
                            id="supplier-catalog-batch-work-retry"
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
                        </Button>
                        {work.phase !== "SAVING" || work.completed === 0 ? (
                            <Button
                                id="supplier-catalog-batch-work-edit"
                                variant="ghost"
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
                            </Button>
                        ) : null}
                    </>
                ) : null}
            </div>
            {control.error ? (
                <p role="alert" className="text-destructive">
                    {control.error.message}
                </p>
            ) : null}
        </div>
    )
}
