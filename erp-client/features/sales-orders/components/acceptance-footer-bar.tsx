"use client"

import { Button } from "@/components/ui/button"
import { OVERALL_RESULT_LABEL } from "@/features/sales-orders/lib/acceptance-types"
import type { AcceptanceOverallResult } from "@/features/sales-orders/lib/acceptance-types"

export function AcceptanceFooterBar({
    salesOrderNo,
    selectedCount,
    overallPreview,
    hasExceptionResult,
    canSave,
    canPost,
    savePending,
    postPending,
    onExit,
    onSaveDraft,
}: {
    salesOrderNo: string
    selectedCount: number
    overallPreview: AcceptanceOverallResult
    hasExceptionResult: boolean
    canSave: boolean
    canPost: boolean
    savePending: boolean
    postPending: boolean
    onExit: () => void
    onSaveDraft: () => void
}) {
    return (
        <div className="sticky bottom-0 z-10 flex flex-wrap items-center justify-between gap-2 border-t border-border/30 bg-card/95 px-4 py-3 backdrop-blur supports-backdrop-filter:bg-card/80">
            <p className="text-sm text-muted-foreground">
                {salesOrderNo} · 已选 {selectedCount} 个来源 · 结果{" "}
                {OVERALL_RESULT_LABEL[overallPreview]}
                {hasExceptionResult ? " · 仅记录结果" : ""}
                <span className="ms-2 hidden text-xs md:inline">
                    ⌘S 保存草稿 · ⌘Enter 提交
                </span>
            </p>
            <div className="flex flex-wrap gap-2">
                <Button
                    type="button"
                    variant="outline"
                    size="sm"
                    onClick={onExit}
                >
                    退出登记
                </Button>
                <Button
                    type="button"
                    variant="outline"
                    size="sm"
                    disabled={!canSave || savePending}
                    onClick={onSaveDraft}
                >
                    保存草稿
                </Button>
                <Button
                    type="submit"
                    form="acceptance-form"
                    size="sm"
                    disabled={!canPost || postPending}
                >
                    确认并完成验收
                </Button>
            </div>
        </div>
    )
}
