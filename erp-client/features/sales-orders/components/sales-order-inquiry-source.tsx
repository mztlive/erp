import Link from "next/link"
import { ArrowUpRightIcon } from "lucide-react"

export function SalesOrderInquirySource({
    source,
    compact = false,
}: {
    source: {
        inquiry_id: string
        inquiry_no: string
        requirement_version: number
        feedback_round: number
    }
    compact?: boolean
}) {
    return (
        <div
            className={
                compact
                    ? "text-sm text-muted-foreground"
                    : "rounded-lg border border-info-border bg-info-soft px-4 py-3 text-sm text-info-soft-foreground"
            }
        >
            <div className="flex flex-wrap items-baseline gap-x-3 gap-y-1">
                {!compact ? <span>来源询价</span> : null}
                <Link
                    id="sales-order-inquiry-source"
                    className="inline-flex items-center gap-1 font-medium underline underline-offset-4"
                    href={`/sales/inquiries/${encodeURIComponent(source.inquiry_id)}`}
                >
                    {compact ? "来源询价" : source.inquiry_no}
                    {compact ? (
                        <ArrowUpRightIcon
                            className="size-3.5"
                            aria-hidden="true"
                        />
                    ) : null}
                </Link>
                <span>
                    需求第 {source.requirement_version} 版 · 第{" "}
                    {source.feedback_round} 轮反馈
                </span>
            </div>
            {!compact ? (
                <p className="mt-1 text-xs">
                    商品、交期和配送范围沿用已接受的询价结果。成交价请逐项确认；商品或交付条件变化须先重新询价。
                </p>
            ) : null}
        </div>
    )
}
