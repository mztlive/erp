"use client"

import type { ReactNode } from "react"
import { Alert, AlertDescription } from "@/components/ui/alert"
import { Label } from "@/components/ui/label"
import {
    Select,
    SelectContent,
    SelectItem,
    SelectTrigger,
    SelectValue,
} from "@/components/ui/select"
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
}: {
    inquiry: SalesInquiry
    chosenRound: number | null
    onRoundChange: (round: number | null) => void
    demand: ReactNode
    revision?: ReactNode
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
    if (editing) return <div className="min-w-0">{demand}</div>

    const availableCount =
        feedback?.data.lines.filter((item) => item.conclusion === "available")
            .length ?? 0
    const partialCount =
        feedback?.data.lines.filter(
            (item) => item.conclusion === "partially_available",
        ).length ?? 0
    const unavailableCount =
        feedback?.data.lines.filter((item) => item.conclusion === "unavailable")
            .length ?? 0
    const currentRoundLabel = `当前需求${waiting ? "（等待反馈）" : ""}`
    const formatRoundLabel = (item: (typeof rounds)[number]) =>
        `第 ${item.round} 轮 · 需求第 ${item.requirement_version} 版${item === currentFeedback ? "（最新）" : ""}`

    return (
        <div className="min-w-0 space-y-4">
            <div className="flex flex-wrap items-center justify-between gap-3">
                <div className="flex flex-wrap items-center gap-x-2.5 gap-y-1 text-body-compact">
                    <h2 className="text-section-title font-semibold tracking-tight text-foreground">
                        {feedback ? (
                            <span className="num">
                                第 {feedback.round} 轮反馈
                            </span>
                        ) : (
                            "商品需求"
                        )}
                    </h2>
                    <span className="num text-xs text-muted-foreground">
                        {feedback
                            ? `对应需求第 ${feedback.requirement_version} 版 · ${inquiryDate(feedback.published_at)}`
                            : `需求第 ${inquiry.requirement_version} 版`}
                    </span>
                    {historical && (
                        <StatusBadge label="历史反馈" tone="neutral" />
                    )}
                    {accepted && <StatusBadge label="已接受" tone="success" />}
                    {deliveryAttentionCount > 0 && (
                        <StatusBadge
                            tone="warning"
                            label={`配送待核对 ${deliveryAttentionCount} 项`}
                        />
                    )}
                </div>

                <div className="flex flex-wrap items-center gap-3">
                    {feedback && (
                        <div className="flex flex-wrap items-center gap-3 text-xs">
                            <span className="inline-flex items-center gap-1.5 text-foreground">
                                <span
                                    aria-hidden="true"
                                    className="size-1.5 rounded-full bg-success"
                                />
                                可供{" "}
                                <span className="num">{availableCount}</span>
                            </span>
                            {partialCount > 0 && (
                                <span className="inline-flex items-center gap-1.5 text-warning-soft-foreground">
                                    <span
                                        aria-hidden="true"
                                        className="size-1.5 rounded-full bg-warning"
                                    />
                                    部分可供{" "}
                                    <span className="num">{partialCount}</span>
                                </span>
                            )}
                            {unavailableCount > 0 && (
                                <span className="inline-flex items-center gap-1.5 text-muted-foreground">
                                    <span
                                        aria-hidden="true"
                                        className="size-1.5 rounded-full bg-muted-foreground/55"
                                    />
                                    不供应{" "}
                                    <span className="num">
                                        {unavailableCount}
                                    </span>
                                </span>
                            )}
                        </div>
                    )}
                    {(rounds.length > 1 ||
                        (!currentFeedback && rounds.length > 0)) && (
                        <div className="flex items-center gap-2">
                            <Label
                                htmlFor="sales-inquiry-products-round"
                                className="shrink-0 text-xs text-muted-foreground"
                            >
                                轮次
                            </Label>
                            <Select
                                value={
                                    feedback
                                        ? String(feedback.round)
                                        : "current"
                                }
                                onValueChange={(value) => {
                                    if (!value) return
                                    const next = rounds.find(
                                        (item) => String(item.round) === value,
                                    )
                                    onRoundChange(next?.round ?? null)
                                }}
                            >
                                <SelectTrigger
                                    id="sales-inquiry-products-round"
                                    size="sm"
                                >
                                    <SelectValue>
                                        {feedback
                                            ? formatRoundLabel(feedback)
                                            : currentRoundLabel}
                                    </SelectValue>
                                </SelectTrigger>
                                <SelectContent
                                    align="end"
                                    alignItemWithTrigger={false}
                                >
                                    {!currentFeedback && (
                                        <SelectItem
                                            id="sales-inquiry-products-round-current"
                                            value="current"
                                        >
                                            {currentRoundLabel}
                                        </SelectItem>
                                    )}
                                    {rounds.map((item) => (
                                        <SelectItem
                                            key={item.round}
                                            id={`sales-inquiry-products-round-${item.round}`}
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
            </div>

            {waiting && !feedback && (
                <Alert variant="info">
                    <AlertDescription>
                        {rounds.length
                            ? "当前需求正在重新询价，等待采购发布新反馈。上一轮报价可通过右上角“轮次”切换查阅。"
                            : "需求已提交，等待采购处理并发布报价反馈。"}
                    </AlertDescription>
                </Alert>
            )}

            {historical && (
                <p className="text-xs text-muted-foreground">
                    正在查看第{" "}
                    <span className="num">{feedback.requirement_version}</span>{" "}
                    版需求及反馈；最新需求为第{" "}
                    <span className="num">{inquiry.requirement_version}</span>{" "}
                    版。
                    {requirement?.note
                        ? `本版需求说明：${requirement.note}`
                        : ""}
                </p>
            )}

            {requirement ? (
                <InquiryProductsTable
                    key={feedback ? `round-${feedback.round}` : "current"}
                    requirement={requirement}
                    feedback={feedback}
                    waiting={waiting}
                />
            ) : (
                <Alert variant="warning">
                    <AlertDescription>
                        该轮报价对应的历史需求明细加载失败，请刷新页面重试。
                    </AlertDescription>
                </Alert>
            )}
        </div>
    )
}
