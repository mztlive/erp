"use client"

import * as React from "react"
import Link from "next/link"
import {
    CheckIcon,
    CircleDashedIcon,
    PackageIcon,
    WalletIcon,
} from "lucide-react"

import {
    DocumentSection,
    DocumentSummary,
    MoneyValue,
    surfaceInsetClassName,
} from "@/components/business"
import { welfareScenarioLabel } from "@/lib/business-options"
import { Button } from "@/components/ui/button"
import { StatusBadge } from "@/components/ui/status-badge"
import {
    Tooltip,
    TooltipContent,
    TooltipProvider,
    TooltipTrigger,
} from "@/components/ui/tooltip"
import { AcceptanceWorkspace } from "@/features/sales-orders/acceptance-workspace"
import { CardSalesApprovalPanel } from "@/features/sales-orders/card-sales-approval-panel"
import { CloseConditionsCard } from "@/features/sales-orders/close-conditions-card"
import { ProcurementRejectionCard } from "@/features/sales-orders/procurement-rejection-card"
import { RevisionHistoryCard } from "@/features/sales-orders/revision-history-card"
import { SalesOrderCollaborationCard } from "@/features/execution-projections/collaboration-card"
import type { SalesOrderDetailView } from "@/features/sales-orders/api"
import { stageDueDisplay } from "@/features/sales-orders/labels"
import {
    fulfillmentWorkspaceHref,
    lifecycleSteps,
    nextStepOwner,
    purchaseOrdersWorkspaceHref,
    receivableWorkspaceHref,
    type FocusTask,
    type LifecycleStep,
    type NavSectionId,
} from "@/features/sales-orders/sales-order-detail-model"
import { cn } from "@/lib/utils"

export function LifecycleRail({ order }: { order: SalesOrderDetailView }) {
    const rail = lifecycleSteps(order)

    if (rail.voided) {
        return (
            <p className="px-3 text-sm text-muted-foreground md:px-4">
                本单已作废，不再进入履约或结案。
            </p>
        )
    }

    return (
        <TooltipProvider>
            <ol
                className="flex flex-wrap items-center gap-x-1 gap-y-1.5 px-3 md:px-4"
                aria-label="销售单生命周期"
            >
                {rail.steps.map((step, index) => (
                    <li key={step.id} className="flex items-center gap-1">
                        {index > 0 ? (
                            <span
                                aria-hidden="true"
                                className="mx-0.5 h-px w-3 bg-border sm:w-5"
                            />
                        ) : null}
                        <RailNode step={step} />
                    </li>
                ))}
            </ol>
        </TooltipProvider>
    )
}

function RailNode({ step }: { step: LifecycleStep }) {
    const node = (
        <span
            className={cn(
                "inline-flex items-center gap-1 rounded-md px-1.5 py-0.5 text-xs",
                step.state === "current" &&
                    "bg-accent font-medium text-foreground",
                step.state === "done" && "text-muted-foreground",
                step.state === "todo" && "text-muted-foreground/70",
            )}
        >
            {step.state === "done" ? (
                <CheckIcon className="size-3 text-success" aria-hidden="true" />
            ) : (
                <CircleDashedIcon
                    className={cn(
                        "size-3",
                        step.state === "current"
                            ? "text-primary"
                            : "text-muted-foreground/60",
                    )}
                    aria-hidden="true"
                />
            )}
            {step.label}
        </span>
    )

    if (!step.hint) return node

    return (
        <Tooltip>
            <TooltipTrigger
                render={
                    <button
                        type="button"
                        aria-label={step.label}
                        className="rounded-sm focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
                    />
                }
            >
                {node}
            </TooltipTrigger>
            <TooltipContent className="max-w-xs text-xs">
                {step.hint}
            </TooltipContent>
        </Tooltip>
    )
}

export function NextStepCard({
    order,
    focusTask,
}: {
    order: SalesOrderDetailView
    focusTask: FocusTask | null
}) {
    const due = stageDueDisplay(order)

    return (
        <div className={cn(surfaceInsetClassName, "space-y-2 px-3 py-3")}>
            <div className="flex items-start justify-between gap-2">
                <h3 className="text-sm font-medium">
                    {focusTask?.title ?? "当前进度"}
                </h3>
                <StatusBadge
                    tone={order.primaryStatus.tone}
                    label={order.primaryStatus.label}
                />
            </div>
            <p className="text-xs text-muted-foreground">
                {focusTask?.description ??
                    "当前不需要你操作，可以在这里看到进度。"}
            </p>
            <dl className="grid grid-cols-2 gap-2 text-xs">
                <div>
                    <dt className="text-muted-foreground">责任人</dt>
                    <dd className="mt-0.5 text-foreground">
                        {nextStepOwner(order)}
                    </dd>
                </div>
                <div>
                    <dt className="text-muted-foreground">时限</dt>
                    <dd className="num mt-0.5 text-foreground">
                        {due ? due.label : "未设置"}
                    </dd>
                </div>
            </dl>
        </div>
    )
}

function RelatedLane({
    label,
    count,
    status,
    href,
    actionLabel,
}: {
    label: string
    count: number
    status: string
    href: string
    actionLabel: string
}) {
    return (
        <li className="flex items-center justify-between gap-3 py-2">
            <div className="min-w-0">
                <div className="text-sm">
                    {label}
                    <span className="num ml-1.5 text-muted-foreground">
                        {count}
                    </span>
                </div>
                <div className="text-xs text-muted-foreground">{status}</div>
            </div>
            <Button
                type="button"
                size="xs"
                variant="outline"
                render={<Link href={href} />}
            >
                {actionLabel}
            </Button>
        </li>
    )
}

export function RelatedLanes({
    order,
    selfReturn,
    lanes,
}: {
    order: SalesOrderDetailView
    selfReturn: string
    lanes: Array<"purchase" | "fulfillment" | "receipt" | "invoice">
}) {
    const items: React.ReactNode[] = []
    if (lanes.includes("purchase")) {
        items.push(
            <RelatedLane
                key="purchase"
                label="采购单"
                count={order.related.purchaseOrders}
                status={order.fulfillment.label}
                href={purchaseOrdersWorkspaceHref(order, selfReturn)}
                actionLabel="打开采购"
            />,
        )
    }
    if (lanes.includes("fulfillment")) {
        items.push(
            <RelatedLane
                key="fulfillment"
                label="交付"
                count={order.related.fulfillments}
                status={order.fulfillment.label}
                href={fulfillmentWorkspaceHref(order, selfReturn)}
                actionLabel="打开交付"
            />,
        )
    }
    if (lanes.includes("receipt")) {
        items.push(
            <RelatedLane
                key="receipt"
                label="回款"
                count={order.related.receipts}
                status={order.collection.label}
                href={receivableWorkspaceHref(order, selfReturn)}
                actionLabel="打开往来"
            />,
        )
    }
    if (lanes.includes("invoice")) {
        items.push(
            <RelatedLane
                key="invoice"
                label="开票"
                count={order.related.invoices}
                status={order.invoicing.label}
                href={receivableWorkspaceHref(order, selfReturn)}
                actionLabel="打开往来"
            />,
        )
    }
    return <ul className="divide-y divide-border/30">{items}</ul>
}

export function LineItemsTable({ order }: { order: SalesOrderDetailView }) {
    const isCard = order.nature === "card_voucher"
    return (
        <div className="overflow-x-auto">
            <table className="w-full text-sm">
                <thead className="bg-muted/50 text-left">
                    <tr>
                        <th className="px-3 py-2 font-medium">项目</th>
                        <th className="px-3 py-2 font-medium">数量</th>
                        {isCard ? (
                            <th className="px-3 py-2 font-medium">
                                面额 / 形态
                            </th>
                        ) : (
                            <th className="px-3 py-2 font-medium">交付方式</th>
                        )}
                        <th className="px-3 py-2 font-medium text-right">
                            含税金额
                        </th>
                    </tr>
                </thead>
                <tbody>
                    {order.lineItems.map((line) => (
                        <tr key={line.id} className="border-t border-border/30">
                            <td className="px-3 py-2">
                                <div>{line.name}</div>
                                {line.sku ? (
                                    <div className="num text-xs text-muted-foreground">
                                        {line.sku}
                                    </div>
                                ) : null}
                            </td>
                            <td className="num px-3 py-2">
                                {line.quantity} {line.unit}
                            </td>
                            {isCard ? (
                                <td className="px-3 py-2 text-sm">
                                    {line.faceValue ? (
                                        <MoneyValue value={line.faceValue} />
                                    ) : (
                                        "—"
                                    )}
                                    {line.cardForm ? (
                                        <span className="mt-0.5 block text-xs text-muted-foreground">
                                            {line.cardForm}
                                        </span>
                                    ) : null}
                                </td>
                            ) : (
                                <td className="px-3 py-2 text-sm text-muted-foreground">
                                    <div>{line.fulfillmentMode ?? "—"}</div>
                                    {line.dueDate ? (
                                        <div className="num mt-0.5 text-xs">
                                            {line.dueDate}
                                        </div>
                                    ) : null}
                                </td>
                            )}
                            <td className="px-3 py-2 text-right">
                                <MoneyValue
                                    value={line.amountGross}
                                    taxBasis="gross"
                                />
                            </td>
                        </tr>
                    ))}
                </tbody>
            </table>
        </div>
    )
}

export function OverviewPanel({
    order,
    focusTask,
    selfReturn,
    showApproval,
}: {
    order: SalesOrderDetailView
    focusTask: FocusTask | null
    selfReturn: string
    showApproval: boolean
}) {
    const isCard = order.nature === "card_voucher"

    return (
        <div className="grid gap-6 xl:grid-cols-[minmax(0,1fr)_19rem]">
            <aside className="order-first space-y-4 xl:order-last xl:sticky xl:top-14 xl:self-start">
                <NextStepCard order={order} focusTask={focusTask} />
                <CloseConditionsCard order={order} />
                <div>
                    <h3 className="mb-1 text-sm font-medium">相关业务</h3>
                    <RelatedLanes
                        order={order}
                        selfReturn={selfReturn}
                        lanes={
                            isCard
                                ? ["receipt", "invoice"]
                                : ["purchase", "fulfillment", "receipt"]
                        }
                    />
                </div>
            </aside>

            <div className="min-w-0">
                {showApproval && order.activeCardSalesApproval ? (
                    <div className="mb-6">
                        <CardSalesApprovalPanel
                            order={order}
                            approval={order.activeCardSalesApproval}
                        />
                    </div>
                ) : null}

                <DocumentSection title="订单信息">
                    <DocumentSummary
                        columns="three"
                        className="rounded-none border-0 bg-transparent p-0 shadow-none"
                        items={[
                            {
                                id: "contract",
                                label: "关联合同",
                                value: order.contractRevisionLabel || "—",
                            },
                            {
                                id: "scene",
                                label: "福利场景",
                                value: welfareScenarioLabel(order.welfareScene),
                            },
                            {
                                id: "payment",
                                label: "付款条件",
                                value: order.paymentTerms || "—",
                            },
                            {
                                id: "deadline",
                                label: isCard
                                    ? "履约期限（到期交付）"
                                    : "履约期限",
                                value: order.fulfillmentDeadline || "—",
                                numeric: true,
                            },
                            {
                                id: "contact",
                                label: "客户联系人",
                                value: order.customerContact ?? "—",
                            },
                            {
                                id: "version",
                                label: "当前版本",
                                value: `v${order.version}`,
                                numeric: true,
                            },
                        ]}
                    />
                </DocumentSection>

                <DocumentSection
                    title={isCard ? "卡券明细" : "销售明细"}
                    description={`${order.lineItems.length} 行`}
                >
                    <LineItemsTable order={order} />
                </DocumentSection>
            </div>
        </div>
    )
}

function AcceptanceSummary({
    order,
    canAccept,
    expanded,
    onExpand,
    onCollapse,
}: {
    order: SalesOrderDetailView
    canAccept: boolean
    expanded: boolean
    onExpand: () => void
    onCollapse: () => void
}) {
    const latest = order.acceptance

    if (expanded) {
        return (
            <div className="space-y-3">
                <div className="flex items-center justify-between gap-2">
                    <h3 className="text-sm font-medium">客户验收</h3>
                    <Button
                        type="button"
                        size="sm"
                        variant="ghost"
                        onClick={onCollapse}
                    >
                        收起验收
                    </Button>
                </div>
                <AcceptanceWorkspace salesOrderId={order.id} />
            </div>
        )
    }

    return (
        <div className={cn(surfaceInsetClassName, "px-3 py-3")}>
            <div className="flex flex-wrap items-start justify-between gap-3">
                <div className="min-w-0 space-y-1">
                    <h3 className="text-sm font-medium">客户验收</h3>
                    <p className="text-xs text-muted-foreground">
                        {latest
                            ? `最近 ${latest.reference} · ${latest.postedAt}${latest.note ? ` · ${latest.note}` : ""}`
                            : "还没有验收记录。客户确认完成后，本单才算交付完毕。"}
                    </p>
                    <p className="text-xs text-muted-foreground">
                        交付进度：{order.fulfillment.label}
                    </p>
                </div>
                <Button
                    type="button"
                    size="sm"
                    disabled={!canAccept}
                    title={
                        canAccept
                            ? undefined
                            : "当前不能验收，请先完成交付或确认权限。"
                    }
                    onClick={onExpand}
                >
                    登记验收
                </Button>
            </div>
        </div>
    )
}

export function FulfillmentPanel({
    order,
    selfReturn,
    showRejection,
    acceptanceExpanded,
    canAccept,
    onExpandAcceptance,
    onCollapseAcceptance,
}: {
    order: SalesOrderDetailView
    selfReturn: string
    showRejection: boolean
    acceptanceExpanded: boolean
    canAccept: boolean
    onExpandAcceptance: () => void
    onCollapseAcceptance: () => void
}) {
    const isCard = order.nature === "card_voucher"

    return (
        <div className="space-y-6">
            {showRejection && order.procurementRejection ? (
                <ProcurementRejectionCard
                    order={order}
                    rejection={order.procurementRejection}
                />
            ) : null}

            <DocumentSection
                title="采购与交付"
                action={
                    isCard ? undefined : (
                        <Button
                            type="button"
                            size="sm"
                            variant="outline"
                            render={
                                <Link
                                    href={fulfillmentWorkspaceHref(
                                        order,
                                        selfReturn,
                                    )}
                                />
                            }
                        >
                            <PackageIcon
                                data-icon="inline-start"
                                aria-hidden="true"
                            />
                            去发货/交付
                        </Button>
                    )
                }
            >
                <RelatedLanes
                    order={order}
                    selfReturn={selfReturn}
                    lanes={
                        isCard ? ["fulfillment"] : ["purchase", "fulfillment"]
                    }
                />
            </DocumentSection>

            {isCard ? (
                <div className={cn(surfaceInsetClassName, "px-3 py-3")}>
                    <h3 className="text-sm font-medium">卡券交付</h3>
                    <p className="mt-1 text-xs text-muted-foreground">
                        到期即算交付完成。期限{" "}
                        {order.fulfillmentDeadline || "—"} · 当前{" "}
                        {order.fulfillment.label}
                        。消费多少不影响本单是否交付完成。
                    </p>
                </div>
            ) : (
                <AcceptanceSummary
                    order={order}
                    canAccept={canAccept}
                    expanded={acceptanceExpanded}
                    onExpand={onExpandAcceptance}
                    onCollapse={onCollapseAcceptance}
                />
            )}
        </div>
    )
}

export function ReceivablePanel({
    order,
    selfReturn,
}: {
    order: SalesOrderDetailView
    selfReturn: string
}) {
    return (
        <DocumentSection
            title="回款与开票"
            action={
                <Button
                    type="button"
                    size="sm"
                    variant="outline"
                    render={
                        <Link
                            href={receivableWorkspaceHref(order, selfReturn)}
                        />
                    }
                >
                    <WalletIcon data-icon="inline-start" aria-hidden="true" />
                    记一笔回款
                </Button>
            }
        >
            <RelatedLanes
                order={order}
                selfReturn={selfReturn}
                lanes={["receipt", "invoice"]}
            />
        </DocumentSection>
    )
}

export function VersionsPanel({ order }: { order: SalesOrderDetailView }) {
    return (
        <div className="space-y-4">
            <RevisionHistoryCard
                revisions={order.revisions}
                currentVersion={order.version}
                contractRevisionLabel={order.contractRevisionLabel}
            />
            {order.activeChangeOrder ? (
                <p className="text-sm text-muted-foreground">
                    改单进行中：{order.activeChangeOrder.statusLabel}（基于 v
                    {order.activeChangeOrder.baseRevisionNo}）。
                    {order.activeChangeOrder.impactPath === "operations"
                        ? "还需运营确认影响，再由财务复核后生效。"
                        : "还需采购确认交付影响，再由财务复核后生效。"}
                </p>
            ) : null}
        </div>
    )
}

export function CollaborationPanel({ order }: { order: SalesOrderDetailView }) {
    if (order.nature !== "card_voucher") {
        return (
            <p className="text-sm text-muted-foreground">
                只有卡券销售单会与商城对接。
            </p>
        )
    }
    return (
        <SalesOrderCollaborationCard
            salesOrderId={order.id}
            salesOrderNo={order.documentNumber}
        />
    )
}

export function navItemsFor(order: SalesOrderDetailView): Array<{
    id: NavSectionId
    label: string
    show: boolean
}> {
    return [
        { id: "overview", label: "概览", show: true },
        { id: "fulfillment", label: "履约", show: true },
        { id: "receivable", label: "票款", show: true },
        {
            id: "collaboration",
            label: "协同",
            show: order.nature === "card_voucher",
        },
        { id: "versions", label: "版本", show: true },
    ]
}
