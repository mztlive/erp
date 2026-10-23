"use client"

import { MoneyValue, RateValue } from "@/components/business"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { Label } from "@/components/ui/label"
import { NativeSelect, NativeSelectOption } from "@/components/ui/native-select"
import {
    Table,
    TableBody,
    TableCell,
    TableHead,
    TableHeader,
    TableRow,
} from "@/components/ui/table"
import { multiplyFixed } from "@/lib/fixed-decimal"
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
    if (!feedback)
        return (
            <p className="text-sm text-muted-foreground">
                采购尚未发布费用结果。
            </p>
        )
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
    const context =
        inquiry.status === "closed"
            ? "询价已关闭，费用及办理记录仅供核对，不能新增分摊或办理。"
            : procurement
              ? showHandling
                  ? `办理依据为第 ${basis.round} 轮反馈，各项费用的办理条件和进度见下表。`
                  : "本轮费用仅供核对，不改变当前费用办理依据。"
              : showHandling
                ? `${waiting ? `当前需求第 ${inquiry.requirement_version} 版正在询价。` : ""}办理依据为第 ${basis.round} 轮已接受结果，可办理范围以各项费用状态为准。`
                : waiting
                  ? `当前需求第 ${inquiry.requirement_version} 版正在询价，本轮费用仅供参考。${accepted ? "" : "本轮结果尚未接受，暂不能分摊或办理。"}`
                  : !accepted
                    ? "本轮结果尚未接受，暂不能分摊或办理。"
                    : historical
                      ? "本轮费用为历史反馈，仅供核对；查看历史不会改变当前办理依据。"
                      : "本轮结果已接受。"
    return (
        <section className="space-y-4" aria-label="询价费用">
            <div className="flex flex-wrap items-center justify-between gap-3">
                <div className="space-y-1">
                    <div className="flex flex-wrap items-center gap-2 text-sm">
                        <span className="font-medium">
                            第 {feedback.round} 轮反馈
                        </span>
                        <span className="text-muted-foreground">
                            对应需求第 {feedback.requirement_version} 版
                        </span>
                        {historical && (
                            <Badge variant="outline">历史反馈</Badge>
                        )}
                        {accepted && <Badge variant="success">已接受</Badge>}
                        <span className="text-muted-foreground">
                            额外费用 {feedback.data.fees.length} 项
                        </span>
                    </div>
                    <p className="text-xs text-muted-foreground">
                        发布于 {inquiryDate(feedback.published_at)}
                    </p>
                </div>
                {rounds.length > 1 && (
                    <div className="flex items-center gap-2">
                        <Label htmlFor="sales-inquiry-fees-round">
                            反馈轮次
                        </Label>
                        <NativeSelect
                            id="sales-inquiry-fees-round"
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
                                    {isCurrent(item) ? "（当前）" : "（历史）"}
                                </NativeSelectOption>
                            ))}
                        </NativeSelect>
                    </div>
                )}
            </div>
            <div className="flex flex-wrap items-center justify-between gap-2 text-sm text-muted-foreground">
                <p>{context}</p>
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
            <p className="text-sm text-muted-foreground">
                本轮未提出额外费用。
            </p>
        )
    return (
        <div className="overflow-hidden rounded-lg border">
            <Table>
                <TableHeader>
                    <TableRow>
                        <TableHead>费用名称</TableHead>
                        <TableHead className="text-right">含税金额</TableHead>
                        <TableHead className="text-right">税率</TableHead>
                    </TableRow>
                </TableHeader>
                <TableBody>
                    {feedback.data.fees.map((fee) => (
                        <TableRow key={fee.fee_id}>
                            <TableCell className="max-w-96 whitespace-normal">
                                <p className="font-medium">{fee.name}</p>
                                {fee.public_note && (
                                    <p className="mt-1 whitespace-pre-wrap text-xs text-muted-foreground">
                                        {fee.public_note}
                                    </p>
                                )}
                            </TableCell>
                            <TableCell className="text-right">
                                <MoneyValue value={fee.amount_gross} />
                            </TableCell>
                            <TableCell className="text-right">
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
