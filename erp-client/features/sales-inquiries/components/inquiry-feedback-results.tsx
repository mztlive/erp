"use client"

import { MoneyValue } from "@/components/business"
import { Badge } from "@/components/ui/badge"
import { Label } from "@/components/ui/label"
import { NativeSelect, NativeSelectOption } from "@/components/ui/native-select"
import { StatusBadge } from "@/components/ui/status-badge"
import { InquiryFeedbackTable } from "./inquiry-feedback-table"
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
}: {
    inquiry: SalesInquiry | ProcurementInquiry
    selectedRound?: number
    onRoundChange?: (round: number) => void
    idPrefix?: string
    content?: "all" | "products" | "fees"
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
            <p className="rounded-lg border border-dashed p-6 text-sm text-muted-foreground">
                采购尚未发布反馈。发布后可查看商品报价及当轮费用。
            </p>
        )
    const accepted =
        "acceptances" in inquiry &&
        inquiry.acceptances.some(
            (item) => item.feedback_round === feedback.round,
        )
    return (
        <section
            className="space-y-5"
            aria-label={content === "fees" ? "反馈费用结果" : "采购反馈结果"}
        >
            <div className="flex flex-wrap items-center justify-between gap-3">
                <div className="space-y-1">
                    <div className="flex flex-wrap items-center gap-3">
                        <h2 className="text-lg font-semibold">
                            第 {feedback.round} 轮反馈
                        </h2>
                        {accepted && (
                            <StatusBadge label="已接受" tone="success" />
                        )}
                        {feedback.round !== inquiry.feedback_round && (
                            <Badge variant="outline">历史轮次</Badge>
                        )}
                    </div>
                    <p className="text-sm text-muted-foreground">
                        需求第 {feedback.requirement_version} 版 ·{" "}
                        {inquiryDate(feedback.published_at)}
                    </p>
                </div>
                {onRoundChange && (
                    <div className="flex items-center gap-2">
                        <Label htmlFor={`${idPrefix}-feedback-round`}>
                            反馈轮次
                        </Label>
                        <NativeSelect
                            id={`${idPrefix}-feedback-round`}
                            value={String(feedback.round)}
                            onChange={(event) => {
                                const next = rounds.find(
                                    (item) =>
                                        String(item.round) ===
                                        event.target.value,
                                )
                                if (next) onRoundChange(next.round)
                            }}
                        >
                            {rounds.map((item) => (
                                <NativeSelectOption
                                    id={`${idPrefix}-feedback-round-option-${item.round}`}
                                    key={item.round}
                                    value={String(item.round)}
                                >
                                    第 {item.round} 轮
                                    {item.round === inquiry.feedback_round
                                        ? "（最新）"
                                        : ""}
                                </NativeSelectOption>
                            ))}
                        </NativeSelect>
                    </div>
                )}
            </div>
            {content === "fees" ? (
                <FeedbackFees feedback={feedback} />
            ) : !requirement ||
              feedback.data.lines.some(
                  (result) =>
                      !requirement.lines.some(
                          (line) => line.line_id === result.line_id,
                      ),
              ) ? (
                <p className="text-sm text-muted-foreground">
                    本轮需求资料缺失或不完整，暂无法展示完整反馈，请联系管理员核对。
                </p>
            ) : (
                <InquiryFeedbackTable
                    key={`${feedback.round}-${feedback.requirement_version}`}
                    readOnly
                    idPrefix={`${idPrefix}-feedback-${feedback.round}`}
                    lines={feedback.data.lines}
                    demand={requirement.lines}
                />
            )}
            {content === "all" && <FeedbackFees feedback={feedback} />}
        </section>
    )
}

function FeedbackFees({ feedback }: { feedback: InquiryFeedback }) {
    return (
        <section className="space-y-3" aria-label="本轮额外费用">
            <h3 className="text-base font-medium">
                本轮额外费用{" "}
                <span className="text-sm font-normal text-muted-foreground">
                    {feedback.data.fees.length} 项
                </span>
            </h3>
            {feedback.data.fees.length === 0 ? (
                <p className="text-sm text-muted-foreground">
                    本轮未提出额外费用。
                </p>
            ) : (
                <div className="divide-y rounded-lg border px-4">
                    {feedback.data.fees.map((fee) => (
                        <div key={fee.fee_id} className="space-y-2 py-4">
                            <div className="flex flex-wrap items-center gap-3">
                                <span className="font-medium">{fee.name}</span>
                                <MoneyValue value={fee.amount_gross} />
                            </div>
                            <p className="text-xs text-muted-foreground">
                                有效至 {inquiryDate(fee.valid_until)}
                            </p>
                            {fee.public_note && (
                                <p className="whitespace-pre-wrap text-sm text-muted-foreground">
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
