"use client"

import {
    BackgroundJobProgress,
    BatchImpactPreview,
    FormalActionResult,
    surfacePanelClassName,
} from "@/components/business"
import { LoaderCircleIcon } from "lucide-react"

import { Button } from "@/components/ui/button"
import type { ExportResultState } from "@/features/mall-consumption-orders/types"
import { formatDateTime } from "@/lib/datetime"
import { cn } from "@/lib/utils"

type ExportPreviewPanelProps = {
    filterSummary: string
    total: number
    isPending: boolean
    onConfirm: () => void
    onCancel: () => void
}

export function ExportPreviewPanel({
    filterSummary,
    total,
    isPending,
    onConfirm,
    onCancel,
}: ExportPreviewPanelProps) {
    return (
        <div className={cn(surfacePanelClassName, "space-y-3 p-4")}>
            <BatchImpactPreview
                title="导出当前筛选全部"
                description="按当前筛选结果导出，不限于当前页；下载时将重新校验权限。"
                filterSummary={filterSummary}
                selectionScope="当前筛选全部"
                estimated={total}
                processable={total}
                skipped={0}
                background
                sensitiveFields={[
                    "收货地址",
                    "手机号",
                    "完整支付流水号",
                    "卡号/卡密（永不导出）",
                    "未授权成本金额",
                ]}
                skippedReason="无权限字段以打码形式导出"
            />
            <div className="flex flex-wrap gap-2">
                <Button
                    id="mall-consumption-orders-export-confirm"
                    type="button"
                    size="sm"
                    disabled={isPending}
                    onClick={() => {
                        void onConfirm()
                    }}
                >
                    {isPending ? (
                        <LoaderCircleIcon
                            data-icon="inline-start"
                            aria-hidden="true"
                            className="animate-spin"
                        />
                    ) : null}
                    {isPending ? "导出中…" : "确认导出"}
                </Button>
                <Button
                    id="mall-consumption-orders-export-cancel"
                    type="button"
                    size="sm"
                    variant="outline"
                    disabled={isPending}
                    onClick={onCancel}
                >
                    取消
                </Button>
            </div>
        </div>
    )
}

type ExportResultPanelProps = {
    result: ExportResultState
}

export function ExportResultPanel({ result }: ExportResultPanelProps) {
    return (
        <div className="space-y-2">
            <FormalActionResult
                status="succeeded"
                title="导出任务已创建"
                description={result.maskDisclaimer}
                reference={result.jobId}
                facts={[
                    {
                        label: "行数",
                        value: String(result.rowCount),
                    },
                    {
                        label: "文件",
                        value: result.downloadLabel,
                    },
                    {
                        label: "到期",
                        value: formatDateTime(
                            result.expiresAt,
                            "monthDay",
                            "passthrough",
                        ),
                    },
                ]}
            />
            <BackgroundJobProgress
                mode="all-or-nothing"
                status="succeeded"
                label="导出作业"
                description={`筛选结果 · 字段打码 · ${result.jobId}`}
                total={result.rowCount}
                completed={result.rowCount}
                succeeded={result.rowCount}
            />
        </div>
    )
}
