import Link from "next/link"
import { ArrowUpRightIcon, ClipboardListIcon } from "lucide-react"

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
    if (compact) {
        return (
            <div className="flex flex-wrap items-baseline gap-x-2.5 gap-y-1 text-body-compact text-muted-foreground">
                <Link
                    id="sales-order-inquiry-source"
                    className="inline-flex items-center gap-1 font-medium text-primary underline-offset-4 hover:underline"
                    href={`/sales/inquiries/${encodeURIComponent(source.inquiry_id)}`}
                >
                    来源询价
                    <ArrowUpRightIcon className="size-3.5" aria-hidden="true" />
                </Link>
                <span className="num">
                    需求第 {source.requirement_version} 版 · 第{" "}
                    {source.feedback_round} 轮反馈
                </span>
            </div>
        )
    }

    return (
        <div className="flex items-start gap-2.5 rounded-lg border border-info-border/75 bg-info-soft/70 px-3.5 py-2.5 text-info-soft-foreground">
            <ClipboardListIcon
                className="mt-0.5 size-4 shrink-0"
                aria-hidden="true"
            />
            <div className="min-w-0 space-y-0.5">
                <div className="flex flex-wrap items-baseline gap-x-2.5 gap-y-0.5">
                    <span>来源询价</span>
                    <Link
                        id="sales-order-inquiry-source"
                        className="num inline-flex items-center gap-0.5 font-medium underline underline-offset-4"
                        href={`/sales/inquiries/${encodeURIComponent(source.inquiry_id)}`}
                    >
                        {source.inquiry_no}
                        <ArrowUpRightIcon
                            className="size-3.5"
                            aria-hidden="true"
                        />
                    </Link>
                    <span className="num text-xs">
                        需求第 {source.requirement_version} 版 · 第{" "}
                        {source.feedback_round} 轮反馈
                    </span>
                </div>
                <p className="text-xs opacity-90">
                    商品、交期和配送范围沿用已接受的询价结果；成交价请逐项确认，调整商品或交付条件请先重新询价。
                </p>
            </div>
        </div>
    )
}
