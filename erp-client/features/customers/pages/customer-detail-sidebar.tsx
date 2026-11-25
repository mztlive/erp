"use client"

import Link from "next/link"
import {
    ArrowUpRightIcon,
    FileTextIcon,
    ShoppingBagIcon,
    UsersIcon,
} from "lucide-react"

import {
    BusinessStatusBadge,
    MoneyValue,
    detailWorkspaceStyles,
} from "@/components/business"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { StatusBadge } from "@/components/ui/status-badge"
import {
    can,
    currentCollaborators,
    currentOwners,
    type CustomerActiveSectionId,
} from "@/features/customers/pages/customer-detail-helpers"
import { periodLabel } from "@/features/customers/pages/customer-detail-records"
import type { CustomerCenterView } from "@/features/customers/types"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { formatDateTime } from "@/lib/datetime"
import { compareDecimal } from "@/lib/fixed-decimal"
import { openWorkspaceLabel } from "@/lib/ui-text"

/**
 * 客户概览右侧摘要栏（无套娃卡片，由 DetailWorkspaceBody 的 1px 竖线与正文分栏）：
 * 汇总负责销售、近期合同与销售单、票款与经营画像，并提供一键下钻。
 */
export function CustomerDetailSidebar({
    customer,
    refetch,
    onSelectSection,
    onManageAssignments,
}: {
    customer: CustomerCenterView
    refetch: () => void
    onSelectSection: (section: CustomerActiveSectionId) => void
    onManageAssignments: () => void
}) {
    const owners = currentOwners(customer)
    const collaborators = currentCollaborators(customer)
    const canManageAssignments = can(customer, "MANAGE_ASSIGNMENTS")
    const recentContracts = customer.contracts.slice(0, 3)
    const recentOrders = customer.salesOrders.slice(0, 3)
    const hasRecentBusiness =
        recentContracts.length > 0 || recentOrders.length > 0

    const overdue = customer.metrics.overdueAmount
    const hasOverdue =
        overdue != null &&
        overdue.trim() !== "" &&
        compareDecimal(overdue, "0", 2) > 0
    const receivableCleared =
        customer.receivableSummary != null &&
        compareDecimal(customer.receivableSummary.receivableBalance, "0", 2) ===
            0

    const receivableHref = `/finance/customer-accounts?customerId=${encodeURIComponent(customer.customerId)}`

    return (
        <div className="min-w-0 space-y-6">
            <section aria-label="负责销售" className="space-y-3">
                <div className="flex flex-wrap items-center justify-between gap-2">
                    <h2 className="text-section-title font-semibold tracking-tight text-foreground">
                        负责销售
                    </h2>
                    {canManageAssignments ? (
                        <Button
                            id="customers-detail-sidebar-manage-assignments"
                            type="button"
                            size="sm"
                            variant="ghost"
                            className={detailWorkspaceStyles.inlineLinkButton}
                            onClick={onManageAssignments}
                        >
                            管理
                        </Button>
                    ) : null}
                </div>

                {owners.length === 0 && collaborators.length === 0 ? (
                    <p className="text-xs text-muted-foreground">
                        暂未指派负责销售
                    </p>
                ) : (
                    <ul className="divide-y divide-border/60">
                        {owners.map((assignment) => (
                            <li
                                key={assignment.id}
                                className="flex items-center justify-between gap-2 py-2 first:pt-0 last:pb-0"
                            >
                                <div className="flex min-w-0 items-center gap-2">
                                    <span
                                        aria-hidden="true"
                                        className="flex size-6 shrink-0 items-center justify-center rounded-full bg-info-soft text-tiny font-semibold text-info-soft-foreground"
                                    >
                                        {assignment.userName.slice(0, 1)}
                                    </span>
                                    <span className="truncate font-medium text-foreground">
                                        {assignment.userName}
                                    </span>
                                </div>
                                <span className="num shrink-0 text-xs text-muted-foreground">
                                    {periodLabel(
                                        assignment.effectiveFrom,
                                        assignment.effectiveTo,
                                    )}
                                </span>
                            </li>
                        ))}
                        {collaborators.map((assignment) => (
                            <li
                                key={assignment.id}
                                className="flex items-center justify-between gap-2 py-2 first:pt-0 last:pb-0"
                            >
                                <div className="flex min-w-0 items-center gap-2">
                                    <span
                                        aria-hidden="true"
                                        className="flex size-6 shrink-0 items-center justify-center rounded-full bg-muted text-tiny font-medium text-muted-foreground"
                                    >
                                        {assignment.userName.slice(0, 1)}
                                    </span>
                                    <span className="truncate text-foreground">
                                        {assignment.userName}
                                    </span>
                                    <Badge variant="neutral">协作</Badge>
                                </div>
                                <span className="num shrink-0 text-xs text-muted-foreground">
                                    {periodLabel(
                                        assignment.effectiveFrom,
                                        assignment.effectiveTo,
                                    )}
                                </span>
                            </li>
                        ))}
                    </ul>
                )}

                <Button
                    id="customers-detail-sidebar-open-audit"
                    type="button"
                    size="xs"
                    variant="ghost"
                    className="-ml-2 text-xs text-muted-foreground hover:text-foreground"
                    onClick={() => onSelectSection("audit")}
                >
                    <UsersIcon data-icon="inline-start" aria-hidden="true" />
                    查看归属与修改记录
                </Button>
            </section>

            <section
                aria-label="近期业务"
                className="space-y-3 border-t border-border/60 pt-5"
            >
                <div className="flex flex-wrap items-center justify-between gap-2">
                    <h2 className="text-section-title font-semibold tracking-tight text-foreground">
                        近期业务
                    </h2>
                    <Button
                        id="customers-detail-sidebar-open-related"
                        type="button"
                        size="sm"
                        variant="ghost"
                        className={detailWorkspaceStyles.inlineLinkButton}
                        onClick={() => onSelectSection("related")}
                    >
                        查看全部
                    </Button>
                </div>

                {customer.partitions.related === "error" ? (
                    <div className="space-y-2 text-xs text-muted-foreground">
                        <p>合同与销售单摘要加载失败。</p>
                        <Button
                            id="customers-detail-sidebar-related-retry"
                            type="button"
                            size="xs"
                            variant="outline"
                            onClick={() => void refetch()}
                        >
                            重新加载
                        </Button>
                    </div>
                ) : !hasRecentBusiness ? (
                    <p className="text-xs text-muted-foreground">
                        暂无关联单据，可通过顶部入口新建销售单或上传合同。
                    </p>
                ) : (
                    <div className="space-y-3.5">
                        {recentContracts.length > 0 ? (
                            <div className="space-y-1.5">
                                <p className="text-xs font-medium text-muted-foreground">
                                    最近合同
                                </p>
                                <ul className="divide-y divide-border/60">
                                    {recentContracts.map((item) => (
                                        <li
                                            key={item.id}
                                            className="flex items-center justify-between gap-2 py-2 first:pt-0 last:pb-0"
                                        >
                                            <Link
                                                id={`customers-detail-sidebar-contract-${toAutomationIdSegment(item.id)}`}
                                                href={item.href}
                                                className="num inline-flex min-w-0 items-center gap-1.5 font-medium text-primary underline-offset-4 hover:underline"
                                            >
                                                <FileTextIcon
                                                    className="size-3.5 shrink-0 text-muted-foreground"
                                                    aria-hidden="true"
                                                />
                                                <span className="truncate">
                                                    {item.number}
                                                </span>
                                            </Link>
                                            <BusinessStatusBadge
                                                context="list"
                                                {...item.status}
                                            />
                                        </li>
                                    ))}
                                </ul>
                            </div>
                        ) : null}

                        {recentOrders.length > 0 ? (
                            <div className="space-y-1.5">
                                <p className="text-xs font-medium text-muted-foreground">
                                    最近销售单
                                </p>
                                <ul className="divide-y divide-border/60">
                                    {recentOrders.map((item) => (
                                        <li
                                            key={item.id}
                                            className="space-y-1 py-2 first:pt-0 last:pb-0"
                                        >
                                            <div className="flex items-center justify-between gap-2">
                                                <Link
                                                    id={`customers-detail-sidebar-order-${toAutomationIdSegment(item.id)}`}
                                                    href={item.href}
                                                    className="num inline-flex min-w-0 items-center gap-1.5 font-medium text-primary underline-offset-4 hover:underline"
                                                >
                                                    <ShoppingBagIcon
                                                        className="size-3.5 shrink-0 text-muted-foreground"
                                                        aria-hidden="true"
                                                    />
                                                    <span className="truncate">
                                                        {item.number}
                                                    </span>
                                                </Link>
                                                <BusinessStatusBadge
                                                    context="list"
                                                    {...item.status}
                                                />
                                            </div>
                                            {item.secondaryStatus ||
                                            item.createdAt ? (
                                                <div className="flex items-center justify-between gap-2 pl-5 text-xs text-muted-foreground">
                                                    <span>
                                                        {item.secondaryStatus
                                                            ?.label ?? ""}
                                                    </span>
                                                    {item.createdAt ? (
                                                        <span className="num">
                                                            {formatDateTime(
                                                                item.createdAt,
                                                                "monthDay",
                                                            )}
                                                        </span>
                                                    ) : null}
                                                </div>
                                            ) : null}
                                        </li>
                                    ))}
                                </ul>
                            </div>
                        ) : null}
                    </div>
                )}
            </section>

            <section
                aria-label="票款与经营速览"
                className="space-y-3 border-t border-border/60 pt-5"
            >
                <div className="flex flex-wrap items-center justify-between gap-2">
                    <h2 className="text-section-title font-semibold tracking-tight text-foreground">
                        票款与经营
                    </h2>
                    <Button
                        id="customers-detail-sidebar-open-settlement"
                        type="button"
                        size="sm"
                        variant="ghost"
                        className={detailWorkspaceStyles.inlineLinkButton}
                        onClick={() => onSelectSection("settlement")}
                    >
                        查看详情
                    </Button>
                </div>

                <dl className="space-y-2.5 text-body-compact">
                    <div className="flex items-baseline justify-between gap-3">
                        <dt className="shrink-0 text-xs text-muted-foreground">
                            回款状态
                        </dt>
                        <dd className="text-right">
                            {customer.partitions.settlement === "error" ? (
                                <span className="text-xs text-muted-foreground">
                                    加载失败
                                </span>
                            ) : customer.receivableSummary ? (
                                <StatusBadge
                                    appearance="text"
                                    tone={
                                        hasOverdue
                                            ? "destructive"
                                            : receivableCleared
                                              ? "success"
                                              : "warning"
                                    }
                                    label={
                                        hasOverdue
                                            ? "存在逾期"
                                            : (customer.receivableSummary
                                                  .collectionProgressLabel ??
                                              "—")
                                    }
                                />
                            ) : (
                                <span className="text-xs text-muted-foreground">
                                    暂无数据
                                </span>
                            )}
                        </dd>
                    </div>
                    {customer.receivableSummary?.openInvoiceableTotal !=
                    null ? (
                        <div className="flex items-baseline justify-between gap-3">
                            <dt className="shrink-0 text-xs text-muted-foreground">
                                可开票余额
                            </dt>
                            <dd className="text-right">
                                <MoneyValue
                                    value={
                                        customer.receivableSummary
                                            .openInvoiceableTotal
                                    }
                                    showHint={false}
                                />
                            </dd>
                        </div>
                    ) : null}
                    {customer.receivableSummary?.earliestOverdueDate ? (
                        <div className="flex items-baseline justify-between gap-3">
                            <dt className="shrink-0 text-xs text-muted-foreground">
                                最早逾期日
                            </dt>
                            <dd className="num text-right text-destructive-soft-foreground">
                                {customer.receivableSummary.earliestOverdueDate}
                            </dd>
                        </div>
                    ) : null}
                    {customer.qualitySummary ? (
                        <div className="space-y-1.5 pt-1">
                            <dt className="text-xs text-muted-foreground">
                                经营标签
                            </dt>
                            <dd className="flex flex-wrap gap-1.5">
                                <Badge variant="neutral">
                                    {customer.qualitySummary.scaleLabel}
                                </Badge>
                                <Badge variant="neutral">
                                    {
                                        customer.qualitySummary
                                            .profitContributionLabel
                                    }
                                </Badge>
                                <Badge variant="neutral">
                                    {
                                        customer.qualitySummary
                                            .collectionRiskLabel
                                    }
                                </Badge>
                            </dd>
                        </div>
                    ) : null}
                </dl>

                <div className="pt-1">
                    <Button
                        id="customers-detail-sidebar-open-accounts"
                        type="button"
                        size="xs"
                        variant="ghost"
                        className="-ml-2 text-xs text-primary hover:bg-sidebar-accent/70 hover:text-primary"
                        render={<Link href={receivableHref} />}
                    >
                        {openWorkspaceLabel("W11")}
                        <ArrowUpRightIcon
                            data-icon="inline-end"
                            aria-hidden="true"
                        />
                    </Button>
                </div>
            </section>
        </div>
    )
}
