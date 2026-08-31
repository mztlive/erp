"use client"

import { surfaceInsetClassName } from "@/components/business"
import { Button } from "@/components/ui/button"
import { Spinner } from "@/components/ui/spinner"
import { BULK_SELECTION_LIMIT } from "@/features/execution-projections/api/projections"
import { cn } from "@/lib/utils"

export function ExecutionProjectionBulkBar({
    selectedCount,
    bulkOverLimit,
    bulkPending,
    onClear,
    onBulkQuery,
    onBulkRetry,
}: {
    selectedCount: number
    bulkOverLimit: boolean
    bulkPending: boolean
    onClear: () => void
    onBulkQuery: () => void
    onBulkRetry: () => void
}) {
    return (
        <div
            role="region"
            aria-label="批量选择"
            className={cn(
                surfaceInsetClassName,
                "flex flex-wrap items-center justify-between gap-2 px-3 py-2 text-sm",
            )}
        >
            <span>
                已选择 <span className="num font-medium">{selectedCount}</span>{" "}
                项（批量操作仅作用于显式选择，不含当前筛选全部）
                {bulkOverLimit ? (
                    <span className="ml-2 text-destructive">
                        批量最多 {BULK_SELECTION_LIMIT} 条，超出部分请分批
                    </span>
                ) : null}
            </span>
            <div className="flex flex-wrap gap-2">
                <Button
                    id="execution-projections-bulk-bar-clear"
                    type="button"
                    size="sm"
                    variant="outline"
                    onClick={onClear}
                >
                    清除选择
                </Button>
                <Button
                    id="execution-projections-bulk-bar-bulk-query"
                    type="button"
                    size="sm"
                    variant="outline"
                    disabled={bulkOverLimit || bulkPending}
                    onClick={onBulkQuery}
                >
                    {bulkPending ? (
                        <Spinner
                            data-icon="inline-start"
                            aria-hidden="true"
                        />
                    ) : null}
                    {bulkPending ? "处理中…" : "批量查询"}
                </Button>
                <Button
                    id="execution-projections-bulk-bar-bulk-retry"
                    type="button"
                    size="sm"
                    disabled={bulkOverLimit || bulkPending}
                    onClick={onBulkRetry}
                >
                    {bulkPending ? (
                        <Spinner
                            data-icon="inline-start"
                            aria-hidden="true"
                        />
                    ) : null}
                    {bulkPending ? "处理中…" : "批量重试"}
                </Button>
            </div>
        </div>
    )
}
