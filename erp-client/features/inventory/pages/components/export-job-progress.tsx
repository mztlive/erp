"use client"

import { BackgroundJobProgress } from "@/components/business"
import { Button } from "@/components/ui/button"
import type { InventoryExportJob } from "@/features/inventory/api/inventory"

interface ExportJobProgressProps {
    job: InventoryExportJob
    onClose: () => void
}

export function ExportJobProgress({ job, onClose }: ExportJobProgressProps) {
    return (
        <BackgroundJobProgress
            mode="all-or-nothing"
            status={
                job.status === "queued"
                    ? "queued"
                    : job.status === "running"
                      ? "running"
                      : job.status === "succeeded"
                        ? "succeeded"
                        : "failed"
            }
            total={job.total}
            completed={job.completed}
            succeeded={job.status === "succeeded" ? job.total : undefined}
            label="库存台账导出"
            description={
                <>
                    范围：{job.filterSummary}
                    。导出文件由系统生成，完成后可在此下载。
                    {job.downloadLabel ? (
                        <span className="mt-1 block font-medium">
                            可下载：{job.downloadLabel}
                        </span>
                    ) : null}
                </>
            }
            action={
                <Button
                    id="inventory-export-job-close"
                    type="button"
                    size="sm"
                    variant="ghost"
                    onClick={onClose}
                >
                    关闭
                </Button>
            }
        />
    )
}
