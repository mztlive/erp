"use client"

import { Fragment, useState } from "react"
import { ChevronDownIcon, ChevronRightIcon, InfoIcon } from "lucide-react"
import { MoneyValue } from "@/components/business"
import { Button } from "@/components/ui/button"
import { Badge } from "@/components/ui/badge"
import {
    Table,
    TableBody,
    TableCell,
    TableHead,
    TableHeader,
    TableRow,
} from "@/components/ui/table"
import { toAutomationIdSegment } from "@/lib/automation-id"
import type { PaymentRequest, RequestLine } from "../types"

const progressLabel: Record<string, string> = {
    NONE: "尚未履约",
    PARTIAL: "部分履约",
    COMPLETED: "已完成履约",
    PENDING: "待履约",
    NOT_STARTED: "尚未履约",
    COMPLETE: "已完成履约",
}

function BasisAmount({ title, value }: { title: string; value?: string }) {
    return (
        <div className="min-w-0 space-y-1">
            <dt className="text-xs leading-5 text-muted-foreground">{title}</dt>
            <dd>
                <MoneyValue value={value} />
                {value == null && (
                    <span className="ml-1 text-xs text-muted-foreground">
                        待核对
                    </span>
                )}
            </dd>
        </div>
    )
}

function OrderBasis({ line, draft }: { line: RequestLine; draft: boolean }) {
    return (
        <div className="grid gap-3 xl:grid-cols-[1.2fr_1fr]">
            <section
                className="space-y-3 rounded-md bg-info-soft/35 p-4"
                aria-label={draft ? "保存时依据" : "提交时依据"}
            >
                <h4 className="text-sm font-semibold">
                    {draft ? "保存时依据" : "提交时依据"}
                </h4>
                <dl className="grid grid-cols-2 gap-x-5 gap-y-3 2xl:grid-cols-4">
                    <BasisAmount title="采购总额" value={line.total_amount} />
                    <BasisAmount
                        title="有效已付"
                        value={line.source_paid_amount}
                    />
                    <BasisAmount title="剩余应付" value={line.open_amount} />
                    <BasisAmount
                        title="其他申请占用"
                        value={line.reserved_amount}
                    />
                </dl>
            </section>
            <section
                className="space-y-3 rounded-md bg-success-soft/35 p-4"
                aria-label="当前进度"
            >
                <h4 className="text-sm font-semibold">当前进度</h4>
                <dl className="grid grid-cols-2 gap-x-5 gap-y-3 sm:grid-cols-3">
                    <BasisAmount
                        title="采购剩余应付"
                        value={line.current_open_amount}
                    />
                    <BasisAmount
                        title="申请占用"
                        value={line.current_reserved_amount}
                    />
                    <div className="space-y-1">
                        <dt className="text-xs leading-5 text-muted-foreground">
                            履约进度
                        </dt>
                        <dd>
                            <Badge variant="secondary">
                                {progressLabel[
                                    line.current_fulfillment ?? ""
                                ] ?? "待核对"}
                            </Badge>
                        </dd>
                    </div>
                </dl>
            </section>
        </div>
    )
}

/** 本申请金额、历史依据与当前余额分别展示，不以历史值补齐当前值。 */
export function RequestOrderDetails({ request }: { request: PaymentRequest }) {
    const [collapsed, setCollapsed] = useState<Set<string>>(() => new Set())
    const draft = request.status === "draft"
    return (
        <section className="space-y-4" aria-label="逐采购申请与付款进度">
            <div className="flex items-center justify-between gap-3">
                <h3 className="text-base font-semibold">采购单明细</h3>
                <span className="text-xs text-muted-foreground">
                    共 {request.lines.length} 笔
                </span>
            </div>
            <p className="flex items-start gap-2 rounded-md bg-muted/40 px-4 py-3 text-xs leading-5 text-muted-foreground">
                <InfoIcon className="mt-0.5 size-4 shrink-0" />
                {draft
                    ? "以下为保存草稿时核对的生效来源，提交时将再次校验。"
                    : "以下保留提交时的付款依据，当前余额与履约进度单独展示。"}
            </p>
            <div className="overflow-hidden rounded-lg border border-border">
                <Table className="min-w-[680px] [--table-cell-inline:1rem] [--table-cell-block:1rem]">
                    <TableHeader>
                        <TableRow>
                            <TableHead className="w-[42%]">采购单</TableHead>
                            <TableHead>付款条件</TableHead>
                            <TableHead data-align="end">本次申请</TableHead>
                            <TableHead data-align="end">本申请已付</TableHead>
                            <TableHead data-align="end">本申请待付</TableHead>
                        </TableRow>
                    </TableHeader>
                    <TableBody>
                        {request.lines.map((line) => {
                            const key = toAutomationIdSegment(
                                line.payable_account_id,
                            )
                            const expanded = !collapsed.has(
                                line.payable_account_id,
                            )
                            return (
                                <Fragment key={line.payable_account_id}>
                                    <TableRow>
                                        <TableCell className="whitespace-normal">
                                            <Button
                                                id={`ppr-order-${key}-toggle`}
                                                variant="ghost"
                                                className="h-auto max-w-full justify-start px-0 py-0 text-left hover:bg-transparent"
                                                aria-expanded={expanded}
                                                aria-controls={`ppr-order-${key}-basis`}
                                                onClick={() =>
                                                    setCollapsed((previous) => {
                                                        const next = new Set(
                                                            previous,
                                                        )
                                                        if (
                                                            next.has(
                                                                line.payable_account_id,
                                                            )
                                                        )
                                                            next.delete(
                                                                line.payable_account_id,
                                                            )
                                                        else
                                                            next.add(
                                                                line.payable_account_id,
                                                            )
                                                        return next
                                                    })
                                                }
                                            >
                                                {expanded ? (
                                                    <ChevronDownIcon />
                                                ) : (
                                                    <ChevronRightIcon />
                                                )}
                                                <span className="num break-all whitespace-normal">
                                                    {line.purchase_order_no} · V
                                                    {line.purchase_version}
                                                </span>
                                                <span className="sr-only">
                                                    {expanded ? "收起" : "展开"}
                                                    付款依据
                                                </span>
                                            </Button>
                                        </TableCell>
                                        <TableCell className="max-w-40 whitespace-normal">
                                            {line.payment_term_name}
                                        </TableCell>
                                        <TableCell data-align="end">
                                            <MoneyValue
                                                value={line.requested_amount}
                                            />
                                        </TableCell>
                                        <TableCell data-align="end">
                                            <MoneyValue
                                                value={line.paid_amount}
                                            />
                                        </TableCell>
                                        <TableCell data-align="end">
                                            <MoneyValue
                                                value={line.unpaid_amount}
                                            />
                                        </TableCell>
                                    </TableRow>
                                    <TableRow
                                        id={`ppr-order-${key}-basis`}
                                        hidden={!expanded}
                                        className="hover:bg-transparent"
                                    >
                                        <TableCell
                                            colSpan={5}
                                            className="p-2 whitespace-normal"
                                        >
                                            <OrderBasis
                                                line={line}
                                                draft={draft}
                                            />
                                        </TableCell>
                                    </TableRow>
                                    {line.source_changed && (
                                        <TableRow className="hover:bg-transparent">
                                            <TableCell
                                                colSpan={5}
                                                className="whitespace-normal pt-0"
                                            >
                                                <p
                                                    role="alert"
                                                    className="text-xs text-destructive"
                                                >
                                                    {request.status === "paid"
                                                        ? "采购来源已变更，如需付款冲正，请先终止原付款授权。"
                                                        : "采购来源已变更，请关闭未付部分后重新申请。"}
                                                </p>
                                            </TableCell>
                                        </TableRow>
                                    )}
                                </Fragment>
                            )
                        })}
                    </TableBody>
                </Table>
            </div>
        </section>
    )
}
