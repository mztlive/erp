"use client"

import {
    BackgroundJobProgress,
    BatchImpactPreview,
    FormalActionResult,
    surfacePanelClassName,
} from "@/components/business"
import { Button } from "@/components/ui/button"
import { getErrorMessage } from "@/lib/api/errors"
import { formatDateTime } from "@/lib/datetime"
import { cn } from "@/lib/utils"
import type { ExportJobResult } from "@/features/supplier-orders/types"

export function SupplierOrdersListExportResult({
    result,
}: {
    result: ExportJobResult
}) {
    return (
        <div className="space-y-2">
            <FormalActionResult
                status="succeeded"
                title="导出任务已创建"
                description={result.maskDisclaimer}
                facts={[
                    {
                        label: "行数",
                        value: String(result.rowCount),
                    },
                    {
                        label: "权限规则",
                        value: "已固定，下载时重新校验",
                    },
                    {
                        label: "文件",
                        value: result.downloadLabel,
                    },
                    {
                        label: "到期",
                        value: formatDateTime(
                            result.expiresAt,
                            "monthDayIntl",
                            "passthrough",
                        ),
                    },
                ]}
            />
            <BackgroundJobProgress
                mode="all-or-nothing"
                status="succeeded"
                label="导出作业"
                description="筛选快照 · 字段打码 · 结果 7 天内可下载"
                total={result.rowCount}
                completed={result.rowCount}
                succeeded={result.rowCount}
            />
        </div>
    )
}

export type SupplierOrdersListExportPreviewProps = {
    total: number
    filterSummary: string
    isPending: boolean
    isError: boolean
    error: unknown
    isRetry: boolean
    onConfirm: () => void
    onCancel: () => void
}

export function SupplierOrdersListExportPreview({
    total,
    filterSummary,
    isPending,
    isError,
    error,
    isRetry,
    onConfirm,
    onCancel,
}: SupplierOrdersListExportPreviewProps) {
    return (
        <div className={cn(surfacePanelClassName, "space-y-3 p-4")}>
            <BatchImpactPreview
                title="导出当前筛选全部"
                description="按当前筛选快照导出，不限于当前页；结果 7 天内可下载，下载时将重新校验权限。"
                filterSummary={filterSummary}
                selectionScope="当前筛选全部"
                estimated={total}
                processable={total}
                skipped={0}
                background
                sensitiveFields={["收货地址", "手机号", "未授权成本金额"]}
                skippedReason="无权限字段以打码形式导出，默认列不含收货地址"
            />
            <div className="flex flex-wrap gap-2">
                {isError ? (
                    <p
                        className="w-full text-sm text-destructive"
                        aria-live="polite"
                    >
                        {getErrorMessage(
                            error,
                            "导出任务创建失败，可按原筛选快照重试。",
                        )}
                    </p>
                ) : null}
                <Button
                    type="button"
                    size="sm"
                    disabled={isPending}
                    onClick={onConfirm}
                >
                    {isRetry ? "按原快照重试" : "确认导出"}
                </Button>
                <Button
                    type="button"
                    size="sm"
                    variant="ghost"
                    onClick={onCancel}
                >
                    取消
                </Button>
            </div>
        </div>
    )
}
