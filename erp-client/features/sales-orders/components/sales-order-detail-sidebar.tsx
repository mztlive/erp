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

/** 侧栏只汇总本单跟进事项，金额总览统一放在页头。 */
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
        <aside
            aria-label="本单待跟进"
            className="min-w-0 rounded-lg border border-border/70 p-5"
        >
            <h2 className="text-lg font-semibold">本单待跟进</h2>
            <div className="mt-2 divide-y divide-border/70">
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
                            ? order.fulfillmentDeadline || "以卡券履约期限为准"
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
                        <span className="text-sm">
                            待回款{" "}
                            <MoneyValue
                                value={remainingReceivableAmount(
                                    order.amountGross,
                                    order.receivedAmount,
                                )}
                            />
                        </span>
                    ) : (
                        <span className="text-sm text-muted-foreground">
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
                    <span className="text-sm">
                        已开 <MoneyValue value={order.invoicedAmount} />
                    </span>
                    <p className="mt-1 text-xs text-muted-foreground">
                        {order.invoicing.label}
                    </p>
                </FollowUp>
                {order.activeChangeOrder ? (
                    <FollowUp
                        id="sales-order-follow-up-change"
                        icon={<FileTextIcon aria-hidden="true" />}
                        title="改单跟进"
                        onClick={() => onSelectSection("versions")}
                    >
                        <StatusBadge
                            tone={order.activeChangeOrder.statusTone}
                            label={order.activeChangeOrder.statusLabel}
                        />
                    </FollowUp>
                ) : null}
            </div>
            <section
                className="mt-3 border-t border-border/70 pt-5"
                aria-labelledby="sales-order-close-heading"
            >
                <h3 id="sales-order-close-heading" className="font-semibold">
                    {closed ? "已结案" : effective ? "未结案原因" : "办理状态"}
                </h3>
                {!effective ? (
                    <p className="mt-3 text-sm text-muted-foreground">
                        {order.primaryStatus.code === "voided"
                            ? "本单已作废。"
                            : "审批通过、生效后进入交付与票款处理。"}
                    </p>
                ) : closed ? (
                    <p className="mt-3 text-sm text-muted-foreground">
                        履约已完成，应收已结清。
                    </p>
                ) : (
                    <div className="mt-3 space-y-2 text-sm text-muted-foreground">
                        {order.closeEligibility.blockers.length ? (
                            order.closeEligibility.blockers.map((reason) => (
                                <p key={reason}>{reason}</p>
                            ))
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
                    <p className="mt-4 border-t border-border/70 pt-3 text-xs leading-5 text-muted-foreground">
                        全部履约完成且应收结清后自动结案；未完成的开票事项继续保留。
                    </p>
                ) : null}
            </section>
        </aside>
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
        <div className="flex items-start gap-3 py-4">
            <span className="mt-1 rounded-md bg-muted p-2 text-muted-foreground [&>svg]:size-4">
                {icon}
            </span>
            <div className="min-w-0 flex-1 space-y-1">
                <h3 className="text-sm font-medium">{title}</h3>
                <div>{children}</div>
            </div>
            <Button
                id={id}
                variant="ghost"
                size="icon-sm"
                aria-label={`查看${title}`}
                onClick={onClick}
            >
                <ChevronRightIcon aria-hidden="true" />
            </Button>
        </div>
    )
}
