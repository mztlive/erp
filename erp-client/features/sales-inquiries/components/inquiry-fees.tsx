"use client"

import { ReceiptTextIcon } from "lucide-react"
import { MoneyValue, RateValue } from "@/components/business"
import { Button } from "@/components/ui/button"
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
import { multiplyFixed, sumFixed } from "@/lib/fixed-decimal"
import { inquiryFeeBasisRound } from "../lib/fee-basis"
import { inquiryDate } from "../lib/format"
import type {
    InquiryFeedback,
    ProcurementInquiry,
    SalesInquiry,
} from "../types"
import { InquiryFeesPanel } from "./inquiry-fees-panel"

/** 查看轮次仅切换报价展示；办理面始终使用服务端确定的费用依据。 */
export function InquiryFees({
    inquiry,
    selectedRound,
    onRoundChange,
    userId,
    permissions,
    onChanged,
    procurement = false,
    handlingEnabled = true,
}: {
    inquiry: SalesInquiry | ProcurementInquiry
    selectedRound: number
    onRoundChange: (round: number) => void
    userId: string
    permissions: readonly string[]
    onChanged: () => void
    procurement?: boolean
    handlingEnabled?: boolean
}) {
    const rounds = [...inquiry.feedbacks].sort((a, b) => b.round - a.round)
    const feedback =
        rounds.find((item) => item.round === selectedRound) ?? rounds[0]
    if (!feedback) {
        return (
            <div className="flex flex-col items-center gap-2 rounded-lg border border-dashed border-border/80 bg-surface-sunken/50 px-6 py-10 text-center">
                <div className="flex size-9 items-center justify-center rounded-full bg-muted text-muted-foreground">
                    <ReceiptTextIcon aria-hidden="true" className="size-4" />
                </div>
                <p className="text-body-compact font-medium text-foreground">
                    暂无额外费用
                </p>
                <p className="text-xs text-muted-foreground">
                    采购发布反馈后，物流、打样等额外费用与办理进度将展示在此处。
                </p>
            </div>
        )
    }
    const acceptances = "acceptances" in inquiry ? inquiry.acceptances : []
    const basisRound = procurement
        ? inquiryFeeBasisRound(inquiry)
        : acceptances.at(-1)?.feedback_round
    const basis = rounds.find((item) => item.round === basisRound)
    const accepted = acceptances.some(
        (item) => item.feedback_round === feedback.round,
    )
    const waiting = ["awaiting_procurement", "procurement_processing"].includes(
        inquiry.status,
    )
    const isCurrent = (item: InquiryFeedback) =>
        ["awaiting_sales", "confirmed"].includes(inquiry.status) &&
        item.round === inquiry.feedback_round &&
        item.requirement_version === inquiry.requirement_version
    const historical = !isCurrent(feedback)
    const hasHandling = handlingEnabled && Boolean(basis?.data.fees.length)
    const showHandling = hasHandling && basis?.round === feedback.round
    let feeTotal: string | null = null
    if (feedback.data.fees.length > 0) {
        try {
            feeTotal = sumFixed(
                feedback.data.fees.map((fee) => fee.amount_gross),
                { maxScale: 4, outputScale: 2 },
            )
        } catch {
            feeTotal = null
        }
    }
    const context =
        inquiry.status === "closed"
            ? "询价已关闭，费用及办理记录仅供核对。"
            : procurement
              ? showHandling
                  ? `办理依据为第 ${basis.round} 轮反馈。`
                  : "本轮费用仅供核对，不改变当前费用办理依据。"
              : showHandling
                ? `${waiting ? `当前需求第 ${inquiry.requirement_version} 版正在询价，` : ""}办理依据为第 ${basis.round} 轮已接受结果。`
                : waiting
                  ? `当前需求第 ${inquiry.requirement_version} 版正在询价，本轮费用仅供参考。${accepted ? "" : "本轮尚未接受，暂不能分摊或办理。"}`
                  : !accepted
                    ? "本轮结果尚未接受，暂不能分摊或办理。"
                    : historical
                      ? "本轮费用为历史反馈，仅供核对。"
                      : "本轮结果已接受。"
    return (
        <section className="space-y-4" aria-label="询价费用">
            <div className="flex flex-wrap items-center justify-between gap-3">
                <div className="space-y-1">
                    <div className="flex flex-wrap items-center gap-2 text-body-compact">
                        <h2 className="text-section-title font-semibold tracking-tight text-foreground">
                            <span className="num">
                                第 {feedback.round} 轮反馈
                            </span>
                        </h2>
                        <span className="num text-xs text-muted-foreground">
                            对应需求第 {feedback.requirement_version} 版 ·{" "}
                            {inquiryDate(feedback.published_at)}
                        </span>
                        {historical && (
                            <StatusBadge tone="neutral" label="历史反馈" />
                        )}
                        {accepted && (
                            <StatusBadge tone="success" label="已接受" />
                        )}
                    </div>
                    <div className="flex flex-wrap items-center gap-x-3 gap-y-1 text-xs text-muted-foreground">
                        <span>{context}</span>
                        {feeTotal !== null && (
                            <span>
                                合计{" "}
                                <MoneyValue
                                    value={feeTotal}
                                    display="exact"
                                    showHint={false}
                                    className="font-medium text-foreground"
                                />
                            </span>
                        )}
                    </div>
                </div>
                <div className="flex flex-wrap items-center gap-2">
                    {hasHandling && !showHandling && basis && (
                        <Button
                            id="sales-inquiry-fees-view-basis"
                            variant="link"
                            size="sm"
                            onClick={() => onRoundChange(basis.round)}
                        >
                            查看第 {basis.round} 轮{procurement ? "" : "已接受"}
                            费用及办理进度
                        </Button>
                    )}
                    {rounds.length > 1 && (
                        <div className="flex items-center gap-2">
                            <Label
                                htmlFor="sales-inquiry-fees-round"
                                className="shrink-0 text-xs text-muted-foreground"
                            >
                                轮次
                            </Label>
                            <NativeSelect
                                id="sales-inquiry-fees-round"
                                className="h-control-sm"
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
                                        key={item.round}
                                        id={`sales-inquiry-fees-round-${item.round}`}
                                        value={String(item.round)}
                                    >
                                        第 {item.round} 轮 · 需求第{" "}
                                        {item.requirement_version} 版
                                        {isCurrent(item)
                                            ? "（当前）"
                                            : "（历史）"}
                                    </NativeSelectOption>
                                ))}
                            </NativeSelect>
                        </div>
                    )}
                </div>
            </div>
            {!showHandling && <FeeQuoteTable feedback={feedback} />}
            {hasHandling && (
                <div hidden={!showHandling}>
                    <InquiryFeesPanel
                        inquiry={inquiry}
                        userId={userId}
                        permissions={permissions}
                        onChanged={onChanged}
                        procurement={procurement}
                        showHeading={false}
                    />
                </div>
            )}
        </section>
    )
}

function FeeQuoteTable({ feedback }: { feedback: InquiryFeedback }) {
    if (feedback.data.fees.length === 0)
        return (
            <div className="rounded-lg border border-dashed border-border/80 bg-surface-sunken/50 px-6 py-8 text-center text-xs text-muted-foreground">
                本轮未提出额外费用。
            </div>
        )
    return (
        <div
            data-slot="detail-inline-table"
            className="min-w-0 border-y border-border/70"
        >
            <Table>
                <TableHeader>
                    <TableRow>
                        <TableHead>费用名称</TableHead>
                        <TableHead data-align="end" className="w-40 text-right">
                            含税金额
                        </TableHead>
                        <TableHead data-align="end" className="w-28 text-right">
                            税率
                        </TableHead>
                    </TableRow>
                </TableHeader>
                <TableBody>
                    {feedback.data.fees.map((fee) => (
                        <TableRow key={fee.fee_id}>
                            <TableCell className="max-w-96 whitespace-normal py-3">
                                <p className="font-medium text-foreground">
                                    {fee.name}
                                </p>
                                {fee.public_note && (
                                    <p className="mt-1 whitespace-pre-wrap text-xs text-muted-foreground">
                                        {fee.public_note}
                                    </p>
                                )}
                            </TableCell>
                            <TableCell
                                data-align="end"
                                className="num py-3 text-right"
                            >
                                <MoneyValue
                                    value={fee.amount_gross}
                                    display="exact"
                                    showHint={false}
                                />
                            </TableCell>
                            <TableCell
                                data-align="end"
                                className="num py-3 text-right"
                            >
                                <RateValue
                                    value={multiplyFixed(fee.tax_rate, "100", {
                                        leftMaxScale: 6,
                                        rightMaxScale: 0,
                                        outputScale: 2,
                                    })}
                                    precision={2}
                                />
                            </TableCell>
                        </TableRow>
                    ))}
                </TableBody>
            </Table>
        </div>
    )
}
