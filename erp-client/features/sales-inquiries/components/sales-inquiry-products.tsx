"use client"

import type { ReactNode } from "react"
import { Alert, AlertDescription } from "@/components/ui/alert"
import { Badge } from "@/components/ui/badge"
import { Label } from "@/components/ui/label"
import { NativeSelect, NativeSelectOption } from "@/components/ui/native-select"
import { StatusBadge } from "@/components/ui/status-badge"
import { deliveryAttention } from "../lib/delivery-progress"
import { salesInquiryFeedback } from "../lib/feedback-view"
import { inquiryDate } from "../lib/format"
import type { SalesInquiry } from "../types"
import { InquiryProductsTable } from "./inquiry-products-table"

export function SalesInquiryProducts({
    inquiry,
    chosenRound,
    onRoundChange,
    demand,
    revision,
    footer,
}: {
    inquiry: SalesInquiry
    chosenRound: number | null
    onRoundChange: (round: number | null) => void
    demand: ReactNode
    revision?: ReactNode
    footer: ReactNode
}) {
    const editing = ["draft", "awaiting_sales_supplement"].includes(
        inquiry.status,
    )
    const waiting = ["awaiting_procurement", "procurement_processing"].includes(
        inquiry.status,
    )
    const feedback = salesInquiryFeedback(inquiry, chosenRound)
    const currentFeedback = salesInquiryFeedback(inquiry, null)
    const rounds = [...inquiry.feedbacks].sort(
        (left, right) => right.round - left.round,
    )
    const requirement = feedback
        ? inquiry.requirements.find(
              (item) => item.version === feedback.requirement_version,
          )?.data
        : inquiry.draft
    const historical =
        feedback &&
        (feedback.round !== inquiry.feedback_round ||
            waiting ||
            feedback.requirement_version !== inquiry.requirement_version)
    const deliveryAttentionCount =
        !editing && requirement
            ? requirement.lines.filter((line) => deliveryAttention(line)).length
            : 0
    const accepted =
        feedback &&
        inquiry.acceptances.some(
            (item) => item.feedback_round === feedback.round,
        )
    if (revision) return revision
    return (
        <div className="flex h-full min-h-0 min-w-0 flex-col gap-3">
            <div className="flex shrink-0 flex-wrap items-center justify-between gap-3">
                <div className="flex flex-wrap items-center gap-x-3 gap-y-1 text-sm">
                    <span className="font-medium">
                        {feedback
                            ? `第 ${feedback.round} 轮反馈`
                            : "当前商品需求"}
                    </span>
                    <span className="text-muted-foreground">
                        {feedback
                            ? `对应需求第 ${feedback.requirement_version} 版 · 发布于 ${inquiryDate(feedback.published_at)}`
                            : `需求第 ${inquiry.requirement_version} 版`}
                    </span>
                    {historical && (
                        <StatusBadge label="历史反馈" tone="neutral" />
                    )}
                    {accepted && <StatusBadge label="已接受" tone="success" />}
                </div>
                {!editing &&
                    (rounds.length > 1 ||
                        (!currentFeedback && rounds.length > 0)) && (
                        <div className="flex items-center gap-2">
                            <Label htmlFor="sales-inquiry-products-round">
                                查看轮次
                            </Label>
                            <NativeSelect
                                id="sales-inquiry-products-round"
                                value={
                                    feedback
                                        ? String(feedback.round)
                                        : "current"
                                }
                                onChange={(event) => {
                                    const next = rounds.find(
                                        (item) =>
                                            String(item.round) ===
                                            event.target.value,
                                    )
                                    onRoundChange(next?.round ?? null)
                                }}
                            >
                                {!currentFeedback && (
                                    <NativeSelectOption
                                        id="sales-inquiry-products-round-current"
                                        value="current"
                                    >
                                        当前需求{waiting ? "（等待反馈）" : ""}
                                    </NativeSelectOption>
                                )}
                                {rounds.map((item) => (
                                    <NativeSelectOption
                                        key={item.round}
                                        id={`sales-inquiry-products-round-${item.round}`}
                                        value={String(item.round)}
                                    >
                                        第 {item.round} 轮 · 需求第{" "}
                                        {item.requirement_version} 版
                                        {item === currentFeedback
                                            ? "（最新）"
                                            : ""}
                                    </NativeSelectOption>
                                ))}
                            </NativeSelect>
                        </div>
                    )}
                <div className="flex flex-wrap items-center gap-2">
                    {feedback && (
                        <p className="text-xs text-muted-foreground">
                            可供{" "}
                            {
                                feedback.data.lines.filter(
                                    (item) => item.conclusion === "available",
                                ).length
                            }{" "}
                            项 · 部分可供{" "}
                            {
                                feedback.data.lines.filter(
                                    (item) =>
                                        item.conclusion ===
                                        "partially_available",
                                ).length
                            }{" "}
                            项 · 不供应{" "}
                            {
                                feedback.data.lines.filter(
                                    (item) => item.conclusion === "unavailable",
                                ).length
                            }{" "}
                            项
                        </p>
                    )}
                    {deliveryAttentionCount > 0 && (
                        <Badge variant="warning">
                            配送待核对 {deliveryAttentionCount} 项
                        </Badge>
                    )}
                </div>
            </div>
            {waiting && !feedback && (
                <Alert variant="info" className="shrink-0">
                    <AlertDescription>
                        {rounds.length
                            ? "当前需求正在重新询价，等待采购发布新反馈。上一轮报价可通过“查看轮次”查阅。"
                            : "需求已提交，等待采购反馈。"}
                    </AlertDescription>
                </Alert>
            )}
            {historical && (
                <p className="shrink-0 text-sm text-muted-foreground">
                    正在查看第 {feedback.requirement_version}{" "}
                    版需求及其报价；最新需求为第 {inquiry.requirement_version}{" "}
                    版。
                    {requirement?.note
                        ? `本版需求说明：${requirement.note}`
                        : ""}
                </p>
            )}
            {editing ? (
                <div className="min-h-0 flex-1 overflow-auto">{demand}</div>
            ) : requirement ? (
                <InquiryProductsTable
                    key={feedback ? `round-${feedback.round}` : "current"}
                    requirement={requirement}
                    feedback={feedback}
                    waiting={waiting}
                />
            ) : (
                <Alert variant="warning">
                    <AlertDescription>
                        本轮需求快照暂不可用，无法核对商品和配送要求。请刷新后重试，不能用当前需求替代历史版本。
                    </AlertDescription>
                </Alert>
            )}
            {!editing && (
                <div className="shrink-0 space-y-2 border-t pt-3">
                    <p className="text-xs text-muted-foreground">
                        交付日期与配送范围沿用对应需求，可在“需求详情”中核对。
                    </p>
                    {footer}
                </div>
            )}
        </div>
    )
}
