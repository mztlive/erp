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
import { Button } from "@/components/ui/button"
import { StatusBadge } from "@/components/ui/status-badge"
import { displayInstanceStatus } from "@/features/approval-workflow/display"
import type { PurchaseOrderDetailSectionId } from "@/features/purchase-orders/pages/purchase-order-detail-helpers"
import {
    SUPPLY_PRICE_BASIS_LABEL,
    type PurchaseOrderCenterView,
} from "@/features/purchase-orders/types"
import { formatDateTime } from "@/lib/datetime"
import { cn } from "@/lib/utils"

/**
 * 右侧摘要栏（无外框，由 DetailWorkspaceBody 的 1px 竖线与正文分栏）：
 * 汇总办理进度（含应付未结 / 履约 / 审批 / 进项票快捷跳转）、先款条件与责任人信息。
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
    const approvalStatus = order.approval?.instance
        ? displayInstanceStatus(order.approval.instance.status)
        : order.identity.reviewLabel

    const amount = (value?: string) =>
        costMasked ? "•••" : value == null ? "—" : <MoneyValue value={value} />

    return (
        <div className="min-w-0 space-y-5">
            <section
                aria-labelledby="purchase-order-progress-heading"
                className="min-w-0"
            >
                <div className="flex flex-wrap items-center justify-between gap-2">
                    <h2
                        id="purchase-order-progress-heading"
                        className="text-section-title font-semibold tracking-tight text-foreground"
                    >
                        办理进度
                    </h2>
                    <StatusBadge
                        tone={order.identity.statusTone}
                        label={order.identity.statusLabel}
                    />
                </div>
                {order.approval?.instance?.currentAssigneeName ? (
                    <p className="mt-1.5 text-xs text-muted-foreground">
                        当前审批人{" "}
                        <span className="font-medium text-foreground">
                            {order.approval.instance.currentAssigneeName}
                        </span>
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
                        <p className="text-body-compact text-foreground">
                            {approvalStatus}
                        </p>
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
                        <p className="text-body-compact text-foreground">
                            {order.progress.fulfillment}
                        </p>
                        {order.header.expectedDate ? (
                            <p className="num mt-0.5 text-xs text-muted-foreground">
                                预计交期 {order.header.expectedDate}
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
                        <p className="text-body-compact text-foreground">
                            {order.progress.payment}
                        </p>
                        <div className="mt-1 flex flex-wrap items-baseline justify-between gap-x-2 text-xs text-muted-foreground">
                            <span>应付未结</span>
                            <span className="num text-body-compact font-semibold text-foreground">
                                {amount(
                                    order.payableSummary?.payableOpenAmount,
                                )}
                            </span>
                        </div>
                        {!order.payableSummary ? (
                            <p className="mt-0.5 text-xs text-muted-foreground">
                                尚未形成应付（需审批通过）。
                            </p>
                        ) : null}
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
                        <p className="text-body-compact text-foreground">
                            {order.progress.invoice}
                        </p>
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

            <section
                aria-labelledby="purchase-order-owner-heading"
                className="border-t border-border/60 pt-5"
            >
                <h2
                    id="purchase-order-owner-heading"
                    className="text-section-title font-semibold tracking-tight text-foreground"
                >
                    责任人信息
                </h2>
                <dl className="mt-3 grid grid-cols-2 gap-x-4 gap-y-3 text-body-compact">
                    {[
                        { label: "负责人", value: order.header.ownerName },
                        {
                            label: "提交人",
                            value: order.header.submittedBy ?? "—",
                        },
                        ...(order.delivery
                            ? [
                                  {
                                      label: "价格依据",
                                      value: SUPPLY_PRICE_BASIS_LABEL[
                                          order.delivery.priceBasis
                                      ],
                                  },
                              ]
                            : []),
                    ].map((item) => (
                        <div key={item.label} className="min-w-0 space-y-0.5">
                            <dt className="text-xs text-muted-foreground">
                                {item.label}
                            </dt>
                            <dd className="break-words font-medium text-foreground">
                                {item.value}
                            </dd>
                        </div>
                    ))}
                </dl>
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
    return (
        <div className="group -mx-2 flex items-start gap-2.5 rounded-lg px-2 py-3 transition-colors hover:bg-muted/40">
            <span className="mt-0.5 shrink-0 text-muted-foreground [&>svg]:size-4">
                {icon}
            </span>
            <div className="min-w-0 flex-1 space-y-0.5">
                <h3 className="text-xs font-medium text-muted-foreground">
                    {title}
                </h3>
                <div>{children}</div>
            </div>
            {onClick ? (
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
            ) : null}
        </div>
    )
}
