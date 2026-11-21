"use client"

import type { ReactNode } from "react"
import {
    BanknoteIcon,
    ChevronRightIcon,
    FileTextIcon,
    TruckIcon,
} from "lucide-react"

import { MoneyValue } from "@/components/business"
import { Button } from "@/components/ui/button"
import { StatusBadge } from "@/components/ui/status-badge"
import type { SalesOrderDetailView } from "../api/sales-orders"
import { getProcurementAttention } from "../lib/procurement-attention"
import type { NavSectionId } from "../lib/sales-order-detail-model"
import { remainingReceivableAmount } from "../lib/sales-order-receivable"

/**
 * 右侧摘要栏（无外框，由 DetailWorkspaceBody 的 1px 竖线与正文分栏）：
 * 汇总交付、回款、开票跟进入口及结案条件。
 */
export function SalesOrderDetailSidebar({
    order,
    onSelectSection,
}: {
    order: SalesOrderDetailView
    onSelectSection: (section: NavSectionId) => void
}) {
    const progress = order.related.procurementProgress
    const needsAttention = getProcurementAttention(progress).needsAttention
    const effective =
        order.currentRevisionNo != null && order.primaryStatus.code !== "voided"
    const closed = order.primaryStatus.code === "closed"
    return (
        <div className="min-w-0 space-y-5">
            <section className="min-w-0">
                <h2 className="text-section-title font-semibold tracking-tight text-foreground">
                    履约与票款进度
                </h2>
                {order.activeChangeOrder ? (
                    <p className="mt-1.5 text-xs text-muted-foreground">
                        以下为当前生效版本的执行情况，尚未包含本次改单。
                    </p>
                ) : null}
                <div className="mt-2 divide-y divide-border/60">
                    <FollowUp
                        id="sales-order-follow-up-delivery"
                        icon={<TruckIcon aria-hidden="true" />}
                        title={
                            order.nature === "card_voucher"
                                ? "履约跟进"
                                : "采购与交付"
                        }
                        onClick={() => onSelectSection("fulfillment")}
                    >
                        <StatusBadge
                            tone={
                                needsAttention && effective
                                    ? "warning"
                                    : order.fulfillment.tone
                            }
                            label={
                                needsAttention && effective
                                    ? "采购待处理"
                                    : order.fulfillment.label
                            }
                        />
                        <p className="mt-1 text-xs text-muted-foreground">
                            {order.nature === "card_voucher"
                                ? order.fulfillmentDeadline ||
                                  "以卡券履约期限为准"
                                : progress.label}
                        </p>
                    </FollowUp>
                    <FollowUp
                        id="sales-order-follow-up-receivable"
                        icon={<BanknoteIcon aria-hidden="true" />}
                        title="回款跟进"
                        onClick={() => onSelectSection("receivable")}
                    >
                        {effective ? (
                            <span className="text-body-compact">
                                待回款{" "}
                                <MoneyValue
                                    value={remainingReceivableAmount(
                                        order.amountGross,
                                        order.receivedAmount,
                                    )}
                                />
                            </span>
                        ) : (
                            <span className="text-body-compact text-muted-foreground">
                                生效后形成应收
                            </span>
                        )}
                        <p className="mt-1 text-xs text-muted-foreground">
                            {order.collection.label}
                            {order.receivableDueDate
                                ? ` · ${order.receivableDueDate} 到期`
                                : " · 到期安排见应收"}
                        </p>
                    </FollowUp>
                    <FollowUp
                        id="sales-order-follow-up-invoice"
                        icon={<FileTextIcon aria-hidden="true" />}
                        title="开票进度"
                        onClick={() => onSelectSection("invoice")}
                    >
                        <span className="text-body-compact">
                            已开 <MoneyValue value={order.invoicedAmount} />
                        </span>
                        <p className="mt-1 text-xs text-muted-foreground">
                            {order.invoicing.label}
                        </p>
                    </FollowUp>
                </div>
            </section>

            <section
                className="border-t border-border/60 pt-5"
                aria-labelledby="sales-order-close-heading"
            >
                <h3
                    id="sales-order-close-heading"
                    className="font-semibold text-foreground"
                >
                    {closed ? "已结案" : effective ? "未结案原因" : "办理状态"}
                </h3>
                {!effective ? (
                    <p className="mt-2.5 text-xs text-muted-foreground">
                        {order.primaryStatus.code === "voided"
                            ? "本单已作废。"
                            : "审批通过、生效后进入交付与票款处理。"}
                    </p>
                ) : closed ? (
                    <p className="mt-2.5 text-xs text-muted-foreground">
                        履约已完成，应收已结清。
                    </p>
                ) : (
                    <div className="mt-2.5 space-y-1.5 text-body-compact text-muted-foreground">
                        {order.closeEligibility.blockers.length ? (
                            <ul className="list-disc space-y-1.5 pl-4">
                                {order.closeEligibility.blockers.map(
                                    (reason) => (
                                        <li key={reason}>{reason}</li>
                                    ),
                                )}
                            </ul>
                        ) : (
                            <p>
                                {order.closeEligibility.eligibleToClose
                                    ? "已满足结案条件，等待状态更新。"
                                    : order.closeEligibility.note ||
                                      "结案条件待确认，请刷新后查看。"}
                            </p>
                        )}
                    </div>
                )}
                {effective ? (
                    <p className="mt-4 border-t border-border/60 pt-3 text-xs text-muted-foreground">
                        全部履约完成且应收结清后自动结案；未完成的开票事项继续保留。
                    </p>
                ) : null}
            </section>
        </div>
    )
}

function FollowUp({
    id,
    icon,
    title,
    children,
    onClick,
}: {
    id: string
    icon: ReactNode
    title: string
    children: ReactNode
    onClick: () => void
}) {
    return (
        <div className="group -mx-2 flex items-start gap-2.5 rounded-lg px-2 py-3.5 transition-colors hover:bg-muted/40">
            <span className="mt-0.5 shrink-0 text-muted-foreground [&>svg]:size-4">
                {icon}
            </span>
            <div className="min-w-0 flex-1 space-y-1">
                <h3 className="font-medium text-foreground">{title}</h3>
                <div>{children}</div>
            </div>
            <Button
                id={id}
                type="button"
                variant="ghost"
                size="icon-sm"
                className="shrink-0 text-muted-foreground group-hover:text-foreground"
                aria-label={`查看${title}`}
                onClick={onClick}
            >
                <ChevronRightIcon aria-hidden="true" />
            </Button>
        </div>
    )
}
