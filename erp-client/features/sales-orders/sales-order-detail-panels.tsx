"use client"

import * as React from "react"
import Link from "next/link"
import {
    CheckIcon,
    CircleDashedIcon,
    InfoIcon,
    PackageIcon,
    ShieldAlertIcon,
    WalletIcon,
} from "lucide-react"

import {
    DocumentHeader,
    DocumentSection,
    MoneyValue,
    surfaceInsetClassName,
} from "@/components/business"
import { welfareScenarioLabel } from "@/lib/business-options"
import {
    Alert,
    AlertAction,
    AlertDescription,
    AlertTitle,
} from "@/components/ui/alert"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import {
    Tooltip,
    TooltipContent,
    TooltipProvider,
    TooltipTrigger,
} from "@/components/ui/tooltip"
import { AcceptanceWorkspace } from "@/features/sales-orders/acceptance-workspace"
import { CardSalesApprovalPanel } from "@/features/sales-orders/card-sales-approval-panel"
import { RevisionHistoryCard } from "@/features/sales-orders/revision-history-card"
import { SalesOrderCollaborationCard } from "@/features/execution-projections/collaboration-card"
import type { SalesOrderDetailView } from "@/features/sales-orders/api"
import {
    NATURE_LABEL,
    ORIGIN_LABEL,
    stageDueDisplay,
} from "@/features/sales-orders/labels"
import {
    fulfillmentWorkspaceHref,
    isOpenProcurementRejection,
    lifecycleSteps,
    nextStepOwner,
    purchaseOrdersWorkspaceHref,
    receivableWorkspaceHref,
    type FocusTask,
    type LifecycleStep,
    type NavSectionId,
} from "@/features/sales-orders/sales-order-detail-model"
import { sumFixed } from "@/lib/fixed-decimal"
import { cn } from "@/lib/utils"

function remainingReceivable(gross: string, received: string) {
    try {
        return sumFixed([gross, `-${received}`], {
            maxScale: 2,
            outputScale: 2,
            allowNegative: true,
        })
    } catch {
        return gross
    }
}

export function SalesOrderIdentityHeader({
    order,
    primaryAction,
    secondaryActions,
}: {
    order: SalesOrderDetailView
    primaryAction?: React.ReactNode
    secondaryActions?: React.ReactNode
}) {
    return (
        <DocumentHeader
            density="compact"
            title={order.customerName}
            documentNumber={order.documentNumber}
            version={order.version}
            primaryStatus={order.primaryStatus}
            statuses={[
                {
                    id: "fulfillment",
                    label: "履约",
                    status: order.fulfillment,
                },
                {
                    id: "collection",
                    label: "回款",
                    status: order.collection,
                },
                {
                    id: "invoicing",
                    label: "开票",
                    status: order.invoicing,
                },
            ]}
            meta={
                <span className="inline-flex flex-wrap items-center gap-x-1.5 gap-y-0.5">
                    <Badge variant="secondary" className="font-normal">
                        {NATURE_LABEL[order.nature]}
                    </Badge>
                    <span aria-hidden="true">·</span>
                    <span>
                        负责人{" "}
                        <span className="font-medium text-foreground">
                            {order.ownerName}
                        </span>
                    </span>
                    <span aria-hidden="true">·</span>
                    <span>{ORIGIN_LABEL[order.originSystem]}</span>
                </span>
            }
            primaryAction={primaryAction}
            secondaryActions={secondaryActions}
        >
            <SalesOrderAmountSummary order={order} />
        </DocumentHeader>
    )
}

function SalesOrderAmountSummary({ order }: { order: SalesOrderDetailView }) {
    const receivableLeft = remainingReceivable(
        order.amountGross,
        order.receivedAmount,
    )

    return (
        <dl
            className="grid grid-cols-2 gap-x-4 gap-y-2 lg:grid-cols-4"
            aria-label="销售单金额摘要"
        >
            <div className="min-w-0">
                <dt className="text-xs text-muted-foreground">
                    成交金额（含税）
                </dt>
                <dd className="mt-0.5 text-sm font-semibold">
                    <MoneyValue value={order.amountGross} taxBasis="gross" />
                </dd>
            </div>
            <div className="min-w-0">
                <dt className="text-xs text-muted-foreground">已回款</dt>
                <dd className="mt-0.5 text-sm font-semibold">
                    <MoneyValue value={order.receivedAmount} taxBasis="gross" />
                    <span className="ml-1.5 text-xs font-normal text-muted-foreground">
                        {order.collection.label}
                    </span>
                </dd>
            </div>
            <div className="min-w-0">
                <dt className="text-xs text-muted-foreground">待回款</dt>
                <dd className="mt-0.5 text-sm font-semibold">
                    <MoneyValue value={receivableLeft} taxBasis="gross" />
                    {order.closeEligibility.receivableSettled ? (
                        <span className="ml-1.5 text-xs font-normal text-muted-foreground">
                            已收齐
                        </span>
                    ) : null}
                </dd>
            </div>
            <div className="min-w-0">
                <dt className="text-xs text-muted-foreground">已开票</dt>
                <dd className="mt-0.5 text-sm font-semibold">
                    <MoneyValue value={order.invoicedAmount} taxBasis="gross" />
                </dd>
            </div>
        </dl>
    )
}

export function FocusTaskBanner({
    order,
    focusTask,
    action,
    canActOnRejection = false,
}: {
    order: SalesOrderDetailView
    focusTask: FocusTask
    action?: React.ReactNode
    canActOnRejection?: boolean
}) {
    const due = stageDueDisplay(order)
    const Icon = focusTask.tone === "warning" ? ShieldAlertIcon : InfoIcon
    const detail = [
        focusTask.description,
        focusTask.id === "procurement-rejection"
            ? rejectionBannerDetail(order, canActOnRejection)
            : null,
        `责任人 ${nextStepOwner(order)}`,
        due ? `时限 ${due.label}` : null,
    ]
        .filter(Boolean)
        .join(" · ")

    return (
        <Alert
            variant={focusTask.tone === "warning" ? "warning" : "info"}
            className="rounded-lg px-3 py-2"
        >
            <Icon aria-hidden="true" />
            <AlertTitle className="text-sm">
                现在要处理 · {focusTask.title}
            </AlertTitle>
            {action ? <AlertAction>{action}</AlertAction> : null}
            <AlertDescription className="text-xs [&_p]:mb-0">
                {detail}
            </AlertDescription>
        </Alert>
    )
}

function rejectionBannerDetail(order: SalesOrderDetailView, canAct: boolean) {
    const rejection = order.procurementRejection
    if (!rejection || !isOpenProcurementRejection(order)) return ""

    const changedCommercial =
        rejection.draftDifference.changedItemOrService ||
        rejection.draftDifference.changedSalesPrice
    const parts = [
        `第 ${rejection.rejectedSubmissionNo} 次报给采购`,
        `${rejection.rejectedByLabel} · ${rejection.rejectedAt}`,
        rejection.estimatedCost ? `采购成本 ${rejection.estimatedCost}` : null,
        rejection.estimatedMarginPercent
            ? `预计毛利 ${rejection.estimatedMarginPercent}%`
            : null,
        changedCommercial
            ? "商品或价格已有改动，用页头「改完再报」核对整单"
            : "还没改商品或价格，改完后才能再报",
        canAct ? null : "当前账号不能改这张单，也不能作废",
    ]
    return parts.filter(Boolean).join(" · ")
}

export function SectionLead({ children }: { children: React.ReactNode }) {
    return <p className="mb-2 text-xs text-muted-foreground">{children}</p>
}

export function LifecycleRail({ order }: { order: SalesOrderDetailView }) {
    const rail = lifecycleSteps(order)

    if (rail.voided) {
        return (
            <p className="text-xs text-muted-foreground">
                本单已作废，不再进入履约或结案。
            </p>
        )
    }

    return (
        <TooltipProvider>
            <ol
                className="flex w-full items-center"
                aria-label="销售单生命周期"
            >
                {rail.steps.map((step, index) => (
                    <li
                        key={step.id}
                        className={cn(
                            "flex min-w-0 items-center",
                            index < rail.steps.length - 1 && "flex-1",
                        )}
                    >
                        <RailNode step={step} />
                        {index < rail.steps.length - 1 ? (
                            <span
                                aria-hidden="true"
                                className={cn(
                                    "mx-1 h-px min-w-4 flex-1",
                                    step.state === "done"
                                        ? "bg-success/50"
                                        : "bg-border",
                                )}
                            />
                        ) : null}
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
                "inline-flex shrink-0 items-center gap-1 rounded-full px-2 py-1 text-xs",
                step.state === "current" &&
                    "bg-accent font-medium text-foreground ring-1 ring-primary/15",
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

const RELATED_LANE_COPY = {
    purchase: {
        label: "采购单",
        hint: "供应商是否已接单、能否交付",
        actionLabel: "打开采购",
    },
    fulfillment: {
        label: "交付",
        hint: "发货、直发或服务执行",
        actionLabel: "打开交付",
    },
    receipt: {
        label: "回款",
        hint: "登记回款并核销到本单",
        actionLabel: "打开往来",
    },
    invoice: {
        label: "开票",
        hint: "开票单独看，不挡结案",
        actionLabel: "打开往来",
    },
} as const

function RelatedLane({
    lane,
    count,
    status,
    href,
}: {
    lane: keyof typeof RELATED_LANE_COPY
    count: number
    status: string
    href: string
}) {
    const copy = RELATED_LANE_COPY[lane]
    return (
        <li className="flex items-center justify-between gap-3 py-2.5">
            <div className="min-w-0">
                <div className="text-sm font-medium">
                    {copy.label}
                    <span className="num ml-1.5 font-normal text-muted-foreground">
                        {count} 笔
                    </span>
                </div>
                <div className="text-xs text-muted-foreground">
                    {copy.hint} · {status}
                </div>
            </div>
            <Button
                type="button"
                size="sm"
                variant="secondary"
                render={<Link href={href} />}
            >
                {copy.actionLabel}
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
                lane="purchase"
                count={order.related.purchaseOrders}
                status={order.fulfillment.label}
                href={purchaseOrdersWorkspaceHref(order, selfReturn)}
            />,
        )
    }
    if (lanes.includes("fulfillment")) {
        items.push(
            <RelatedLane
                key="fulfillment"
                lane="fulfillment"
                count={order.related.fulfillments}
                status={order.fulfillment.label}
                href={fulfillmentWorkspaceHref(order, selfReturn)}
            />,
        )
    }
    if (lanes.includes("receipt")) {
        items.push(
            <RelatedLane
                key="receipt"
                lane="receipt"
                count={order.related.receipts}
                status={order.collection.label}
                href={receivableWorkspaceHref(order, selfReturn)}
            />,
        )
    }
    if (lanes.includes("invoice")) {
        items.push(
            <RelatedLane
                key="invoice"
                lane="invoice"
                count={order.related.invoices}
                status={order.invoicing.label}
                href={receivableWorkspaceHref(order, selfReturn)}
            />,
        )
    }
    return <ul className="divide-y divide-border/30">{items}</ul>
}

function OverviewField({
    label,
    value,
    numeric,
}: {
    label: string
    value: React.ReactNode
    numeric?: boolean
}) {
    return (
        <div className="min-w-0">
            <dt className="text-xs text-muted-foreground">{label}</dt>
            <dd className={cn("mt-0.5 truncate text-sm", numeric && "num")}>
                {value}
            </dd>
        </div>
    )
}

export function LineItemsTable({ order }: { order: SalesOrderDetailView }) {
    const isCard = order.nature === "card_voucher"
    return (
        <div className="overflow-x-auto">
            <table className="w-full text-sm">
                <thead className="bg-muted/50 text-left">
                    <tr>
                        <th className="px-3 py-1.5 font-medium">项目</th>
                        <th className="px-3 py-1.5 font-medium">数量</th>
                        {isCard ? (
                            <th className="px-3 py-1.5 font-medium">
                                面额 / 形态
                            </th>
                        ) : (
                            <th className="px-3 py-1.5 font-medium">
                                交付方式
                            </th>
                        )}
                        <th className="px-3 py-1.5 font-medium text-right">
                            含税金额
                        </th>
                    </tr>
                </thead>
                <tbody>
                    {order.lineItems.map((line) => (
                        <tr key={line.id} className="border-t border-border/30">
                            <td className="px-3 py-1.5">
                                <div>{line.name}</div>
                                {line.sku ? (
                                    <div className="num text-xs text-muted-foreground">
                                        {line.sku}
                                    </div>
                                ) : null}
                            </td>
                            <td className="num px-3 py-1.5">
                                {line.quantity} {line.unit}
                            </td>
                            {isCard ? (
                                <td className="px-3 py-1.5 text-sm">
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
                                <td className="px-3 py-1.5 text-sm text-muted-foreground">
                                    <div>{line.fulfillmentMode ?? "—"}</div>
                                    {line.dueDate ? (
                                        <div className="num mt-0.5 text-xs">
                                            {line.dueDate}
                                        </div>
                                    ) : null}
                                </td>
                            )}
                            <td className="px-3 py-1.5 text-right">
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
    showApproval,
    onApprovalResult,
}: {
    order: SalesOrderDetailView
    showApproval: boolean
    onApprovalResult?: (result: {
        status: "succeeded" | "blocked" | "rejected"
        title: string
        description: string
        reference: string
        nextResponsible?: string
    }) => void
}) {
    const isCard = order.nature === "card_voucher"

    return (
        <div className="space-y-4">
            {showApproval && order.activeCardSalesApproval ? (
                <CardSalesApprovalPanel
                    order={order}
                    approval={order.activeCardSalesApproval}
                    onResult={onApprovalResult}
                />
            ) : null}

            <dl className="grid grid-cols-2 gap-x-4 gap-y-2 xl:grid-cols-3">
                <OverviewField
                    label="关联合同"
                    value={order.contractRevisionLabel || "—"}
                />
                <OverviewField
                    label="福利场景"
                    value={welfareScenarioLabel(order.welfareScene)}
                />
                <OverviewField
                    label="付款条件"
                    value={order.paymentTerms || "—"}
                />
                <OverviewField
                    label={isCard ? "履约期限（到期交付）" : "履约期限"}
                    value={order.fulfillmentDeadline || "—"}
                    numeric
                />
                <OverviewField
                    label="客户联系人"
                    value={order.customerContact ?? "—"}
                />
                <OverviewField
                    label="当前版本"
                    value={`v${order.version}`}
                    numeric
                />
            </dl>

            <div>
                <div className="mb-2 flex items-baseline justify-between gap-2">
                    <h2 className="text-sm font-medium">
                        {isCard ? "卡券明细" : "销售明细"}
                    </h2>
                    <p className="text-xs text-muted-foreground">
                        {order.lineItems.length} 行
                    </p>
                </div>
                <LineItemsTable order={order} />
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
    acceptanceExpanded,
    canAccept,
    onExpandAcceptance,
    onCollapseAcceptance,
}: {
    order: SalesOrderDetailView
    selfReturn: string
    acceptanceExpanded: boolean
    canAccept: boolean
    onExpandAcceptance: () => void
    onCollapseAcceptance: () => void
}) {
    const isCard = order.nature === "card_voucher"

    return (
        <div className="space-y-3">
            <SectionLead>
                {isCard
                    ? "卡券到期即算交付完成。消费多少不影响本单是否交付完毕。"
                    : "采购接单和发货在对应工作面完成；客户确认后，在本页登记验收。"}
            </SectionLead>
            <DocumentSection
                title="采购与交付"
                className="py-3 first:pt-0 last:pb-0"
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
        <div className="space-y-3">
            <SectionLead>
                回款收齐后系统自动结案。开票进度单独看，不挡结案。
            </SectionLead>
            <DocumentSection
                title="回款与开票"
                className="py-3 first:pt-0 last:pb-0"
                action={
                    <Button
                        type="button"
                        size="sm"
                        variant="outline"
                        render={
                            <Link
                                href={receivableWorkspaceHref(
                                    order,
                                    selfReturn,
                                )}
                            />
                        }
                    >
                        <WalletIcon
                            data-icon="inline-start"
                            aria-hidden="true"
                        />
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
        </div>
    )
}

export function VersionsPanel({ order }: { order: SalesOrderDetailView }) {
    return (
        <div className="space-y-4">
            <SectionLead>
                改单会另开一笔，不会改掉客户正在执行的版本。生效前仍按当前版本履约和回款。
            </SectionLead>
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
        <div className="space-y-4">
            <SectionLead>
                这里只看商城接收和执行投影，不提供第二套改单入口。
            </SectionLead>
            <SalesOrderCollaborationCard
                salesOrderId={order.id}
                salesOrderNo={order.documentNumber}
            />
        </div>
    )
}

export function navItemsFor(order: SalesOrderDetailView): Array<{
    id: NavSectionId
    label: string
    hint: string
    show: boolean
}> {
    return [
        {
            id: "overview",
            label: "概览",
            hint: "约定、明细和下一步",
            show: true,
        },
        {
            id: "fulfillment",
            label: "履约",
            hint: "采购、发货和验收",
            show: true,
        },
        {
            id: "receivable",
            label: "票款",
            hint: "回款和开票",
            show: true,
        },
        {
            id: "collaboration",
            label: "协同",
            hint: "商城同步与执行投影",
            show: order.nature === "card_voucher",
        },
        {
            id: "versions",
            label: "版本",
            hint: "改单与历史版本",
            show: true,
        },
    ]
}
