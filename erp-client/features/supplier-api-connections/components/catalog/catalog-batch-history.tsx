"use client"

import Link from "next/link"
import { Button } from "@/components/ui/button"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { useAdoptionBatchesQuery } from "../../hooks/batch"

export function CatalogBatchHistory({
    connectionId,
}: {
    connectionId: string
}) {
    const query = useAdoptionBatchesQuery(connectionId)
    return (
        <details className="rounded-lg border p-4">
            <summary
                id="supplier-catalog-batch-history-toggle"
                className="cursor-pointer text-sm font-medium"
            >
                入库批次{query.data ? `（${query.data.length}）` : ""}
            </summary>
            <div className="mt-3 space-y-3 text-sm">
                {query.isPending ? (
                    <p role="status" className="text-muted-foreground">
                        正在读取已保存的批次…
                    </p>
                ) : null}
                {query.isError ? (
                    <div
                        role="alert"
                        className="flex flex-wrap items-center justify-between gap-2"
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
                {query.isSuccess && !query.data.length ? (
                    <p className="text-muted-foreground">
                        暂无入库批次。选品后保存的草稿和入库结果会保留在这里。
                    </p>
                ) : null}
                {query.data?.map((batch) => (
                    <div
                        key={batch.id}
                        className="flex flex-wrap items-center justify-between gap-3 border-b pb-3 last:border-0 last:pb-0"
                    >
                        <div>
                            <p className="num font-medium">{batch.batch_no}</p>
                            <p className="mt-1 text-xs text-muted-foreground">
                                {batch.selected_count} 个 SKU ·{" "}
                                {batch.submitted
                                    ? "已提交，可查看进度及结果"
                                    : "草稿已保存，继续配置后重新核对"}
                            </p>
                        </div>
                        <Button
                            id={`supplier-catalog-batch-history-open-${toAutomationIdSegment(batch.id)}`}
                            variant="outline"
                            size="sm"
                            render={
                                <Link
                                    href={`/supplier-api/catalog/adoptions/${encodeURIComponent(batch.id)}?connectionId=${encodeURIComponent(connectionId)}`}
                                />
                            }
                        >
                            {batch.submitted ? "查看进度与结果" : "继续配置"}
                        </Button>
                    </div>
                ))}
            </div>
        </details>
    )
}
