"use client"

import { MoneyValue, QuantityValue, RateValue } from "@/components/business"
import { Badge } from "@/components/ui/badge"
import { Label } from "@/components/ui/label"
import { NativeSelect, NativeSelectOption } from "@/components/ui/native-select"
import { StatusBadge } from "@/components/ui/status-badge"
import {
    Table,
    TableBody,
    TableCell,
    TableHead,
    TableHeader,
    TableRow,
} from "@/components/ui/table"
import { multiplyFixed } from "@/lib/fixed-decimal"
import { inquiryDate } from "../lib/format"
import {
    INQUIRY_CONCLUSION_LABEL,
    type InquiryFeedback,
    type InquiryFeedbackLine,
    type InquiryLine,
    type ProcurementInquiry,
    type SalesInquiry,
} from "../types"

export function InquiryFeedbackResults({
    inquiry,
    selectedRound,
    onRoundChange,
    idPrefix = "sales-inquiry",
}: {
    inquiry: SalesInquiry | ProcurementInquiry
    selectedRound?: number
    onRoundChange?: (round: number) => void
    idPrefix?: string
}) {
    const rounds = [...inquiry.feedbacks].sort(
        (left, right) => right.round - left.round,
    )
    const feedback =
        rounds.find((item) => item.round === selectedRound) ?? rounds[0]
    if (!feedback)
        return (
            <p className="rounded-lg border border-dashed p-6 text-sm text-muted-foreground">
                采购尚未发布反馈。发布后可在此核对商品供给和当轮费用。
            </p>
        )
    const requirement = inquiry.requirements.find(
        (item) => item.version === feedback.requirement_version,
    )?.data
    const accepted =
        "acceptances" in inquiry &&
        inquiry.acceptances.some(
            (item) => item.feedback_round === feedback.round,
        )
    return (
        <section className="space-y-5" aria-label="采购反馈结果">
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
            {!requirement && (
                <p className="text-sm text-muted-foreground">
                    本轮需求快照暂不可用，需求数量及地址不以当前草稿替代。
                </p>
            )}
            <div className="overflow-hidden rounded-lg border">
                <Table className="min-w-[1120px]">
                    <TableHeader>
                        <TableRow>
                            <TableHead>商品 / 供给结论</TableHead>
                            <TableHead className="text-right">
                                需求数量
                            </TableHead>
                            <TableHead className="text-right">
                                可供数量
                            </TableHead>
                            <TableHead className="text-right">
                                含税供给单价
                            </TableHead>
                            <TableHead>税率</TableHead>
                            <TableHead>交付日期</TableHead>
                            <TableHead>有效期</TableHead>
                        </TableRow>
                    </TableHeader>
                    <TableBody>
                        {feedback.data.lines.map((result) => (
                            <FeedbackRows
                                key={result.line_id}
                                result={result}
                                line={requirement?.lines.find(
                                    (line) => line.line_id === result.line_id,
                                )}
                            />
                        ))}
                    </TableBody>
                </Table>
            </div>
            <FeedbackFees feedback={feedback} />
        </section>
    )
}

function FeedbackRows({
    result,
    line,
}: {
    result: InquiryFeedbackLine
    line?: InquiryLine
}) {
    const prices = result.prices.length ? result.prices : [null]
    return (
        <>
            {prices.map((price, index) => (
                <TableRow key={price?.price_id ?? result.line_id}>
                    {index === 0 && (
                        <>
                            <TableCell
                                rowSpan={prices.length}
                                className="min-w-56 max-w-80 whitespace-normal align-top py-4"
                            >
                                <p className="font-medium">
                                    {line?.product_name ?? "商品需求"}
                                </p>
                                {line && (
                                    <p className="mt-1 text-xs text-muted-foreground">
                                        SKU {line.sku_code}
                                    </p>
                                )}
                                <div className="mt-2">
                                    <StatusBadge
                                        label={
                                            INQUIRY_CONCLUSION_LABEL[
                                                result.conclusion
                                            ]
                                        }
                                        tone={
                                            result.conclusion === "unavailable"
                                                ? "warning"
                                                : result.conclusion ===
                                                    "pending"
                                                  ? "neutral"
                                                  : "success"
                                        }
                                    />
                                </div>
                                {result.public_note && (
                                    <p className="mt-2 whitespace-pre-wrap text-sm text-muted-foreground">
                                        {result.public_note}
                                    </p>
                                )}
                            </TableCell>
                            <TableCell
                                rowSpan={prices.length}
                                className="text-right align-top py-4"
                            >
                                {line ? (
                                    <QuantityValue
                                        unit=""
                                        value={line.quantity}
                                    />
                                ) : (
                                    "—"
                                )}
                            </TableCell>
                            <TableCell
                                rowSpan={prices.length}
                                className="text-right align-top py-4"
                            >
                                <QuantityValue
                                    unit=""
                                    value={result.available_quantity}
                                />
                            </TableCell>
                        </>
                    )}
                    <TableCell className="min-w-36 text-right align-top py-4">
                        {price ? (
                            <>
                                <MoneyValue value={price.unit_price_gross} />
                            </>
                        ) : (
                            "—"
                        )}
                    </TableCell>
                    <TableCell className="align-top py-4">
                        {price?.tax_rate ? (
                            <RateValue
                                value={multiplyFixed(price.tax_rate, "100", {
                                    leftMaxScale: 6,
                                    rightMaxScale: 0,
                                    outputScale: 2,
                                })}
                                precision={2}
                            />
                        ) : (
                            "—"
                        )}
                    </TableCell>
                    <TableCell className="align-top py-4">
                        {price && line
                            ? inquiryDate(line.delivery_due_at)
                            : "—"}
                    </TableCell>
                    <TableCell className="align-top py-4">
                        {price ? inquiryDate(price.valid_until) : "—"}
                    </TableCell>
                </TableRow>
            ))}
        </>
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
                                <Badge variant="outline">
                                    {fee.confirmation === "estimated"
                                        ? "暂估"
                                        : "已确认"}
                                </Badge>
                                {fee.included_in_goods_price && (
                                    <Badge variant="outline">已含商品价</Badge>
                                )}
                            </div>
                            <p className="text-xs text-muted-foreground">
                                有效至 {inquiryDate(fee.valid_until)}
                                {fee.valid_conditions
                                    ? ` · ${fee.valid_conditions}`
                                    : ""}
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
