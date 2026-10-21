import { StatusBadge } from "@/components/ui/status-badge"
import type { InquirySalesProgress } from "../types"

export const TRANSFER_LABEL = {
    not_started: "未转",
    partial: "部分已转",
    complete: "全部已转",
}
export const VALIDITY_LABEL = {
    unavailable: "暂无商品报价",
    valid: "商品报价有效",
    partially_expired: "部分商品报价已过期",
    expired: "商品报价已过期",
}

export function InquiryProgress({
    progress,
    compact = false,
}: {
    progress: InquirySalesProgress
    compact?: boolean
}) {
    if (compact)
        return (
            <p
                className="text-sm text-muted-foreground"
                aria-label="转单进度与商品报价有效性"
            >
                已全部转单 {progress.completed_lines} 项（含销售草稿占用）
                <span className="mx-2" aria-hidden="true">
                    ·
                </span>
                {VALIDITY_LABEL[progress.quote_validity]}
            </p>
        )
    return (
        <div
            className="flex flex-wrap items-center gap-3 text-sm"
            aria-label="转单进度与商品报价有效性"
        >
            <StatusBadge
                label={TRANSFER_LABEL[progress.transfer_status]}
                tone={
                    progress.transfer_status === "complete"
                        ? "success"
                        : "neutral"
                }
            />
            <span className="text-muted-foreground">
                {progress.total_lines} 项商品，{progress.completed_lines}{" "}
                项已全部转单（含销售草稿占用）
            </span>
            <StatusBadge
                label={VALIDITY_LABEL[progress.quote_validity]}
                tone={
                    progress.quote_validity === "expired" ||
                    progress.quote_validity === "partially_expired"
                        ? "warning"
                        : "neutral"
                }
            />
        </div>
    )
}
