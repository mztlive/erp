"use client"

import { MoneyValue } from "@/components/business"
import { Label } from "@/components/ui/label"
import {
    Select,
    SelectContent,
    SelectItem,
    SelectTrigger,
    SelectValue,
} from "@/components/ui/select"
import { StatusBadge } from "@/components/ui/status-badge"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { cn } from "@/lib/utils"
import { InquiryFeedbackTable } from "./inquiry-feedback-table"
import { sumInquiryFeesGross } from "../lib/feedback-summary"
import { inquiryDate } from "../lib/format"
import {
    type InquiryFeedback,
    type ProcurementInquiry,
    type SalesInquiry,
} from "../types"

export function InquiryFeedbackResults({
    inquiry,
    selectedRound,
    onRoundChange,
    idPrefix = "sales-inquiry",
    content = "all",
    workbench = false,
}: {
    inquiry: SalesInquiry | ProcurementInquiry
    selectedRound?: number
    onRoundChange?: (round: number) => void
    idPrefix?: string
    content?: "all" | "products" | "fees"
    workbench?: boolean
}) {
    const rounds = [...inquiry.feedbacks].sort(
        (left, right) => right.round - left.round,
    )
    const feedback =
        rounds.find((item) => item.round === selectedRound) ?? rounds[0]
    const requirement = inquiry.requirements.find(
        (item) => item.version === feedback?.requirement_version,
    )?.data
    if (!feedback)
        return (
            <p
                className={cn(
                    "text-xs text-muted-foreground",
                    workbench ? "py-4" : "rounded-lg border border-dashed p-6",
                )}
            >
                采购尚未发布反馈。发布后可查看商品报价及当轮费用。
            </p>
        )
    const accepted =
        "acceptances" in inquiry &&
        inquiry.acceptances.some(
            (item) => item.feedback_round === feedback.round,
        )
    const formatRoundLabel = (item: InquiryFeedback) =>
        `第 ${item.round} 轮${item.round === inquiry.feedback_round ? "（最新）" : ""}`
    return (
        <section
            className={workbench ? "space-y-3" : "space-y-5"}
            aria-label={content === "fees" ? "反馈费用结果" : "采购反馈结果"}
        >
            <div className="flex flex-wrap items-center justify-between gap-3">
                <div className="space-y-1">
                    <div className="flex flex-wrap items-center gap-3">
                        <h2 className="text-section-title font-semibold tracking-tight">
                            第 {feedback.round} 轮反馈
                        </h2>
                        {accepted && (
                            <StatusBadge label="已接受" tone="success" />
                        )}
                        {feedback.round !== inquiry.feedback_round && (
                            <StatusBadge label="历史轮次" tone="neutral" />
                        )}
                    </div>
                    <p className="text-xs text-muted-foreground">
                        需求第 {feedback.requirement_version} 版 ·{" "}
                        {inquiryDate(feedback.published_at)}
                    </p>
                </div>
                {onRoundChange && (
                    <div className="flex items-center gap-2">
                        <Label htmlFor={`${idPrefix}-feedback-round`}>
                            反馈轮次
                        </Label>
                        <Select
                            value={String(feedback.round)}
                            onValueChange={(value) => {
                                if (!value) return
                                const next = rounds.find(
                                    (item) => String(item.round) === value,
                                )
                                if (next) onRoundChange(next.round)
                            }}
                        >
                            <SelectTrigger id={`${idPrefix}-feedback-round`}>
                                <SelectValue>
                                    {formatRoundLabel(feedback)}
                                </SelectValue>
                            </SelectTrigger>
                            <SelectContent
                                align="end"
                                alignItemWithTrigger={false}
                            >
                                {rounds.map((item) => (
                                    <SelectItem
                                        id={`${idPrefix}-feedback-round-option-${toAutomationIdSegment(item.round)}`}
                                        key={item.round}
                                        value={String(item.round)}
                                    >
                                        {formatRoundLabel(item)}
                                    </SelectItem>
                                ))}
                            </SelectContent>
                        </Select>
                    </div>
                )}
            </div>
            {content === "fees" ? (
                <FeedbackFees feedback={feedback} workbench={workbench} />
            ) : !requirement ||
              feedback.data.lines.some(
                  (result) =>
                      !requirement.lines.some(
                          (line) => line.line_id === result.line_id,
                      ),
              ) ? (
                <p className="text-xs text-muted-foreground">
                    本轮需求资料缺失或不完整，暂无法展示完整反馈，请联系管理员核对。
                </p>
            ) : (
                <InquiryFeedbackTable
                    key={`${feedback.round}-${feedback.requirement_version}`}
                    readOnly
                    workbench={workbench}
                    idPrefix={`${idPrefix}-feedback-${feedback.round}`}
                    lines={feedback.data.lines}
                    demand={requirement.lines}
                    feeCount={feedback.data.fees.length}
                    feesTotal={sumInquiryFeesGross(feedback.data.fees)}
                />
            )}
            {content === "all" && (
                <FeedbackFees feedback={feedback} workbench={workbench} />
            )}
        </section>
    )
}

function FeedbackFees({
    feedback,
    workbench = false,
}: {
    feedback: InquiryFeedback
    workbench?: boolean
}) {
    return (
        <section className="space-y-3" aria-label="本轮额外费用">
            <h3 className="text-body-compact font-medium">
                本轮额外费用{" "}
                <span className="num text-body-compact font-normal text-muted-foreground">
                    {feedback.data.fees.length} 项
                </span>
            </h3>
            {feedback.data.fees.length === 0 ? (
                <p className="text-xs text-muted-foreground">
                    本轮未提出额外费用。
                </p>
            ) : (
                <div
                    className={
                        workbench
                            ? "divide-y divide-border/60 border-t border-border/60"
                            : "divide-y rounded-lg border px-4"
                    }
                >
                    {feedback.data.fees.map((fee) => (
                        <div key={fee.fee_id} className="space-y-2 py-4">
                            <div className="flex flex-wrap items-center gap-3">
                                <span className="font-medium">{fee.name}</span>
                                <MoneyValue value={fee.amount_gross} />
                            </div>

                            {fee.public_note && (
                                <p className="whitespace-pre-wrap text-xs text-muted-foreground">
                                    {fee.public_note}
                                </p>
                            )}
                        </div>
                    ))}
                </div>
            )}
        </section>
    )
}
