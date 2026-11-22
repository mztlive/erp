"use client"

import type { ReactNode } from "react"
import {
    BanknoteIcon,
    ChevronRightIcon,
    ClipboardCheckIcon,
    FileTextIcon,
    TruckIcon,
} from "lucide-react"

import { MoneyValue } from "@/components/business"
import { StatusBadge, type StatusTone } from "@/components/ui/status-badge"
import {
    displayInstanceStatus,
    instanceStatusTone,
} from "@/features/approval-workflow/display"
import type { PurchaseOrderDetailSectionId } from "@/features/purchase-orders/pages/purchase-order-detail-helpers"
import type { PurchaseOrderCenterView } from "@/features/purchase-orders/types"
import { formatDateTime } from "@/lib/datetime"
import { cn } from "@/lib/utils"

function fulfillmentDisplay(raw: string): { label: string; tone: StatusTone } {
    if (raw === "完成") return { label: "已完成", tone: "success" }
    if (raw === "部分") return { label: "部分履约", tone: "info" }
    return { label: raw || "未开始", tone: "neutral" }
}

function paymentDisplay(raw: string): { label: string; tone: StatusTone } {
    if (raw === "已付") return { label: "已付清", tone: "success" }
    if (raw === "部分付款") return { label: "部分付款", tone: "info" }
    return { label: raw || "未付", tone: "neutral" }
}

function invoiceDisplay(raw: string): { label: string; tone: StatusTone } {
    if (raw === "完成") return { label: "已收齐", tone: "success" }
    if (raw === "部分") return { label: "部分收票", tone: "info" }
    if (raw === "未收") return { label: "未收票", tone: "neutral" }
    return { label: raw || "未收票", tone: "neutral" }
}

function approvalDisplay(order: PurchaseOrderCenterView): {
    label: string
    tone: StatusTone
} {
    if (order.approval?.instance) {
        return {
            label: displayInstanceStatus(order.approval.instance.status),
            tone: instanceStatusTone(order.approval.instance.status),
        }
    }
    const status = order.identity.reviewStatus
    const tone: StatusTone =
        status === "APPROVED"
            ? "success"
            : status === "REJECTED"
              ? "destructive"
              : status === "PENDING"
                ? "warning"
                : "neutral"
    return { label: order.identity.reviewLabel, tone }
}

/**
 * 右侧摘要栏（无外框，由 DetailWorkspaceBody 的 1px 竖线与正文分栏）：
 * 聚焦审批、履约、付款与进项票办理进度；整行可点，与销售单侧栏保持一致层级。
 */
export function PurchaseOrderDetailSidebar({
    order,
    costMasked,
    onSelectSection,
}: {
    order: PurchaseOrderCenterView
    costMasked: boolean
    onSelectSection?: (section: PurchaseOrderDetailSectionId) => void
}) {
    const gate = order.progress.prepaymentGate
    const approval = approvalDisplay(order)
    const fulfillment = fulfillmentDisplay(order.progress.fulfillment)
    const payment = paymentDisplay(order.progress.payment)
    const invoice = invoiceDisplay(order.progress.invoice)

    const amount = (value?: string) =>
        costMasked ? "•••" : value == null ? "—" : <MoneyValue value={value} />

    return (
        <div className="min-w-0 space-y-5">
            <section
                aria-labelledby="purchase-order-progress-heading"
                className="min-w-0"
            >
                <h2
                    id="purchase-order-progress-heading"
                    className="text-section-title font-semibold tracking-tight text-foreground"
                >
                    办理进度
                </h2>
                {order.activeChangeOrder ? (
                    <p className="mt-1.5 text-xs text-muted-foreground">
                        以下为当前生效版本的执行进度，尚未包含在途改单。
                    </p>
                ) : null}

                <div className="mt-2 divide-y divide-border/60">
                    <SidebarTrackRow
                        id="purchase-order-sidebar-approval"
                        icon={<ClipboardCheckIcon aria-hidden="true" />}
                        title="审批"
                        onClick={
                            onSelectSection
                                ? () => onSelectSection("approval")
                                : undefined
                        }
                    >
                        <StatusBadge
                            tone={approval.tone}
                            label={approval.label}
                            appearance="text"
                        />
                        {order.approval?.instance?.currentAssigneeName ? (
                            <p className="mt-1 text-xs text-muted-foreground">
                                当前审批人{" "}
                                <span className="font-medium text-foreground">
                                    {
                                        order.approval.instance
                                            .currentAssigneeName
                                    }
                                </span>
                            </p>
                        ) : null}
                    </SidebarTrackRow>

                    <SidebarTrackRow
                        id="purchase-order-sidebar-fulfillment"
                        icon={<TruckIcon aria-hidden="true" />}
                        title="履约"
                        onClick={
                            onSelectSection
                                ? () => onSelectSection("fulfillment")
                                : undefined
                        }
                    >
                        <StatusBadge
                            tone={fulfillment.tone}
                            label={fulfillment.label}
                            appearance="text"
                        />
                        {order.header.expectedDate ? (
                            <p className="num mt-1 text-xs text-muted-foreground">
                                预计交期 {order.header.expectedDate}
                            </p>
                        ) : order.header.targetWarehouseName ? (
                            <p className="mt-1 text-xs text-muted-foreground">
                                目标仓 {order.header.targetWarehouseName}
                            </p>
                        ) : null}
                    </SidebarTrackRow>

                    <SidebarTrackRow
                        id="purchase-order-sidebar-payment"
                        icon={<BanknoteIcon aria-hidden="true" />}
                        title="付款"
                        onClick={
                            onSelectSection
                                ? () => onSelectSection("payable")
                                : undefined
                        }
                    >
                        <StatusBadge
                            tone={payment.tone}
                            label={payment.label}
                            appearance="text"
                        />
                        {order.payableSummary ? (
                            <div className="mt-1 flex flex-wrap items-baseline justify-between gap-x-2 text-xs text-muted-foreground">
                                <span>应付未结</span>
                                <span className="num font-medium text-foreground">
                                    {amount(
                                        order.payableSummary.payableOpenAmount,
                                    )}
                                </span>
                            </div>
                        ) : (
                            <p className="mt-1 text-xs text-muted-foreground">
                                尚未形成应付（需审批通过）。
                            </p>
                        )}
                    </SidebarTrackRow>

                    <SidebarTrackRow
                        id="purchase-order-sidebar-invoice"
                        icon={<FileTextIcon aria-hidden="true" />}
                        title="进项票"
                        onClick={
                            onSelectSection
                                ? () => onSelectSection("payable")
                                : undefined
                        }
                    >
                        <StatusBadge
                            tone={invoice.tone}
                            label={invoice.label}
                            appearance="text"
                        />
                    </SidebarTrackRow>
                </div>

                {gate.state !== "NOT_APPLICABLE" ? (
                    <div
                        className={cn(
                            "mt-4 rounded-lg border px-3.5 py-3",
                            gate.state === "SATISFIED"
                                ? "border-success-border/70 bg-success-soft/45"
                                : "border-warning-border/80 bg-warning-soft/55",
                        )}
                    >
                        <div className="flex flex-wrap items-center justify-between gap-2">
                            <p className="text-body-compact font-medium text-foreground">
                                先款条件
                            </p>
                            <StatusBadge
                                tone={
                                    gate.state === "SATISFIED"
                                        ? "success"
                                        : "warning"
                                }
                                label={
                                    gate.state === "SATISFIED"
                                        ? "已满足"
                                        : "未满足"
                                }
                                appearance="text"
                            />
                        </div>
                        <p className="mt-1.5 text-xs text-muted-foreground">
                            {gate.message}
                        </p>
                        <dl className="mt-2.5 grid grid-cols-3 gap-2 border-t border-border/50 pt-2.5 text-xs">
                            {[
                                { label: "需先款", value: gate.required },
                                { label: "已核销", value: gate.allocated },
                                { label: "还差", value: gate.gap },
                            ].map((item) => (
                                <div key={item.label} className="min-w-0">
                                    <dt className="text-muted-foreground">
                                        {item.label}
                                    </dt>
                                    <dd className="num mt-0.5 wrap-anywhere text-body-compact font-medium text-foreground">
                                        {amount(item.value)}
                                    </dd>
                                </div>
                            ))}
                        </dl>
                        <p className="mt-2 text-xs text-muted-foreground">
                            更新于{" "}
                            <time dateTime={gate.updatedAt} className="num">
                                {formatDateTime(gate.updatedAt, "default")}
                            </time>
                        </p>
                    </div>
                ) : null}
            </section>
        </div>
    )
}

function SidebarTrackRow({
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
    onClick?: () => void
}) {
    const content = (
        <>
            <span className="mt-0.5 shrink-0 text-muted-foreground [&>svg]:size-4">
                {icon}
            </span>
            <div className="min-w-0 flex-1 space-y-1 text-left">
                <h3 className="text-body-compact font-medium text-foreground">
                    {title}
                </h3>
                <div>{children}</div>
            </div>
            {onClick ? (
                <ChevronRightIcon
                    aria-hidden="true"
                    className="mt-0.5 size-4 shrink-0 text-muted-foreground transition-colors group-hover:text-foreground"
                />
            ) : null}
        </>
    )

    if (onClick) {
        return (
            <button
                id={id}
                type="button"
                aria-label={`查看${title}`}
                onClick={onClick}
                className="group -mx-2 flex w-[calc(100%+1rem)] cursor-pointer items-start gap-2.5 rounded-lg px-2 py-3 text-left transition-colors hover:bg-muted/45 focus-visible:outline-2 focus-visible:-outline-offset-2 focus-visible:outline-ring"
            >
                {content}
            </button>
        )
    }

    return (
        <div
            id={id}
            className="-mx-2 flex items-start gap-2.5 px-2 py-3 text-left"
        >
            {content}
        </div>
    )
}
