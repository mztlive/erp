"use client"

import Link from "next/link"
import { ArrowUpRightIcon } from "lucide-react"

import {
    AsyncSectionState,
    BusinessEmptyState,
    BusinessFailureState,
    BusinessStatusBadge,
    DataFreshness,
    DetailWorkspaceFact,
    DetailWorkspaceFacts,
    DetailWorkspaceMetricItem,
    DetailWorkspaceMetrics,
    DetailWorkspaceSection,
    MoneyValue,
    detailWorkspaceStyles,
} from "@/components/business"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { StatusBadge } from "@/components/ui/status-badge"
import {
    Table,
    TableBody,
    TableCell,
    TableHead,
    TableHeader,
    TableRow,
} from "@/components/ui/table"
import type { CustomerCenterView } from "@/features/customers/types"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { formatDateTime } from "@/lib/datetime"
import { compareDecimal } from "@/lib/fixed-decimal"
import { openWorkspaceLabel } from "@/lib/ui-text"

export function CustomerDetailRelatedTab({
    customer,
    refetch,
}: {
    customer: CustomerCenterView
    refetch: () => void
}) {
    const contractsHref = `/sales/contracts?customerId=${encodeURIComponent(customer.customerId)}`
    const ordersHref = `/sales/orders?customerId=${encodeURIComponent(customer.customerId)}`

    if (customer.partitions.related === "error") {
        return (
            <DetailWorkspaceSection title="合同与销售" width="full">
                <BusinessFailureState
                    kind="system"
                    description="合同与销售单加载失败"
                    action={
                        <Button
                            id="customers-detail-related-retry"
                            type="button"
                            size="sm"
                            onClick={() => void refetch()}
                        >
                            重新加载
                        </Button>
                    }
                />
            </DetailWorkspaceSection>
        )
    }

    return (
        <>
            <DetailWorkspaceSection
                title="合同（最近）"
                badge={
                    customer.metrics.activeContractCount != null ? (
                        <Badge variant="neutral" className="num">
                            有效 {customer.metrics.activeContractCount} 份
                        </Badge>
                    ) : undefined
                }
                subtitle={
                    customer.metrics.expiringContractCount
                        ? `${customer.metrics.expiringContractCount} 份合同即将到期`
                        : undefined
                }
                actions={
                    <Button
                        id="customers-detail-related-view-contracts"
                        type="button"
                        size="sm"
                        variant="ghost"
                        className={detailWorkspaceStyles.subtleButton}
                        render={<Link href={contractsHref} />}
                    >
                        查看全部合同
                        <ArrowUpRightIcon
                            data-icon="inline-end"
                            aria-hidden="true"
                        />
                    </Button>
                }
                width="full"
            >
                {customer.contracts.length === 0 ? (
                    <BusinessEmptyState
                        kind="no-data"
                        title="暂无合同记录"
                        description="该客户尚未归档合同，可通过顶部「上传合同 PDF」登记。"
                        className="rounded-lg border-0 bg-transparent px-0 py-4 shadow-none ring-0"
                    />
                ) : (
                    <div className={detailWorkspaceStyles.dataTableWrapper}>
                        <Table>
                            <TableHeader>
                                <TableRow>
                                    <TableHead>合同编号</TableHead>
                                    <TableHead>合同状态</TableHead>
                                    <TableHead className="text-right">
                                        操作
                                    </TableHead>
                                </TableRow>
                            </TableHeader>
                            <TableBody>
                                {customer.contracts.map((item) => (
                                    <TableRow key={item.id}>
                                        <TableCell>
                                            <Link
                                                id={`customers-detail-related-${toAutomationIdSegment(item.id)}-number`}
                                                href={item.href}
                                                className="num font-medium text-primary underline-offset-4 hover:underline focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
                                            >
                                                {item.number}
                                            </Link>
                                        </TableCell>
                                        <TableCell>
                                            <BusinessStatusBadge
                                                context="list"
                                                {...item.status}
                                            />
                                        </TableCell>
                                        <TableCell className="text-right">
                                            <Button
                                                id={`customers-detail-related-${toAutomationIdSegment(item.id)}-open`}
                                                type="button"
                                                size="xs"
                                                variant="ghost"
                                                className={
                                                    detailWorkspaceStyles.inlineLinkButton
                                                }
                                                render={
                                                    <Link href={item.href} />
                                                }
                                            >
                                                打开合同
                                                <ArrowUpRightIcon
                                                    data-icon="inline-end"
                                                    aria-hidden="true"
                                                />
                                            </Button>
                                        </TableCell>
                                    </TableRow>
                                ))}
                            </TableBody>
                        </Table>
                    </div>
                )}
            </DetailWorkspaceSection>

            <DetailWorkspaceSection
                title="销售单（最近）"
                badge={
                    customer.metrics.inProgressSalesOrderCount != null ? (
                        <Badge variant="neutral" className="num">
                            进行中 {customer.metrics.inProgressSalesOrderCount}{" "}
                            单
                        </Badge>
                    ) : undefined
                }
                actions={
                    <Button
                        id="customers-detail-related-view-orders"
                        type="button"
                        size="sm"
                        variant="ghost"
                        className={detailWorkspaceStyles.subtleButton}
                        render={<Link href={ordersHref} />}
                    >
                        查看全部销售单
                        <ArrowUpRightIcon
                            data-icon="inline-end"
                            aria-hidden="true"
                        />
                    </Button>
                }
                width="full"
            >
                {customer.salesOrders.length === 0 ? (
                    <BusinessEmptyState
                        kind="no-data"
                        title="暂无销售单记录"
                        description="该客户尚未发生销售订单，可通过顶部「新建销售单」发起。"
                        className="rounded-lg border-0 bg-transparent px-0 py-4 shadow-none ring-0"
                    />
                ) : (
                    <div className={detailWorkspaceStyles.dataTableWrapper}>
                        <Table>
                            <TableHeader>
                                <TableRow>
                                    <TableHead>销售单号</TableHead>
                                    <TableHead>单据状态</TableHead>
                                    <TableHead>履约结案</TableHead>
                                    <TableHead>创建时间</TableHead>
                                    <TableHead className="text-right">
                                        操作
                                    </TableHead>
                                </TableRow>
                            </TableHeader>
                            <TableBody>
                                {customer.salesOrders.map((item) => (
                                    <TableRow key={item.id}>
                                        <TableCell>
                                            <Link
                                                id={`customers-detail-related-${toAutomationIdSegment(item.id)}-number`}
                                                href={item.href}
                                                className="num font-medium text-primary underline-offset-4 hover:underline focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
                                            >
                                                {item.number}
                                            </Link>
                                        </TableCell>
                                        <TableCell>
                                            <BusinessStatusBadge
                                                context="list"
                                                {...item.status}
                                            />
                                        </TableCell>
                                        <TableCell>
                                            {item.secondaryStatus ? (
                                                <BusinessStatusBadge
                                                    context="list"
                                                    appearance="quiet"
                                                    {...item.secondaryStatus}
                                                />
                                            ) : (
                                                <span className="text-xs text-muted-foreground">
                                                    —
                                                </span>
                                            )}
                                        </TableCell>
                                        <TableCell className="num text-muted-foreground">
                                            {item.createdAt
                                                ? formatDateTime(item.createdAt)
                                                : "—"}
                                        </TableCell>
                                        <TableCell className="text-right">
                                            <Button
                                                id={`customers-detail-related-${toAutomationIdSegment(item.id)}-open`}
                                                type="button"
                                                size="xs"
                                                variant="ghost"
                                                className={
                                                    detailWorkspaceStyles.inlineLinkButton
                                                }
                                                render={
                                                    <Link href={item.href} />
                                                }
                                            >
                                                打开销售单
                                                <ArrowUpRightIcon
                                                    data-icon="inline-end"
                                                    aria-hidden="true"
                                                />
                                            </Button>
                                        </TableCell>
                                    </TableRow>
                                ))}
                            </TableBody>
                        </Table>
                    </div>
                )}
            </DetailWorkspaceSection>
        </>
    )
}

export function CustomerDetailSettlementTab({
    customer,
    refetch,
}: {
    customer: CustomerCenterView
    refetch: () => void
}) {
    const receivableHref = `/finance/customer-accounts?customerId=${encodeURIComponent(customer.customerId)}`
    const qualityHref = `/analytics/customer-quality?customerId=${encodeURIComponent(customer.customerId)}`
    const summary = customer.receivableSummary
    const hasOverdue =
        summary != null && compareDecimal(summary.overdueAmount, "0", 2) > 0
    const receivableCleared =
        summary != null &&
        compareDecimal(summary.receivableBalance, "0", 2) === 0
    const invoiceCompleted =
        summary?.openInvoiceableTotal != null &&
        compareDecimal(summary.openInvoiceableTotal, "0", 2) === 0

    return (
        <>
            <DetailWorkspaceSection
                title="票款摘要"
                width="full"
                subtitle={
                    summary?.projectedAt ? (
                        <>
                            更新于{" "}
                            <span className="num">
                                {formatDateTime(summary.projectedAt)}
                            </span>
                        </>
                    ) : undefined
                }
                actions={
                    <Button
                        id="customers-detail-settlement-open-accounts"
                        type="button"
                        size="sm"
                        variant="ghost"
                        className={detailWorkspaceStyles.subtleButton}
                        render={<Link href={receivableHref} />}
                    >
                        {openWorkspaceLabel("W11")}
                        <ArrowUpRightIcon
                            data-icon="inline-end"
                            aria-hidden="true"
                        />
                    </Button>
                }
            >
                {customer.partitions.settlement === "error" ? (
                    <BusinessFailureState
                        kind="system"
                        description="票款汇总加载失败"
                        action={
                            <Button
                                id="customers-detail-settlement-retry"
                                type="button"
                                size="sm"
                                onClick={() => void refetch()}
                            >
                                重新加载
                            </Button>
                        }
                    />
                ) : summary ? (
                    <div className="space-y-3">
                        <DetailWorkspaceMetrics
                            columns={3}
                            variant="inline"
                            ariaLabel="票款核心金额"
                        >
                            <DetailWorkspaceMetricItem
                                label="应收余额"
                                value={
                                    <MoneyValue
                                        value={summary.receivableBalance}
                                    />
                                }
                                detail={receivableCleared ? "已结清" : "待回款"}
                            />
                            <DetailWorkspaceMetricItem
                                label={
                                    <>
                                        <span>逾期金额</span>
                                        {hasOverdue ? (
                                            <StatusBadge
                                                tone="destructive"
                                                label="存在逾期"
                                                appearance="text"
                                            />
                                        ) : null}
                                    </>
                                }
                                value={
                                    <MoneyValue value={summary.overdueAmount} />
                                }
                                detail={
                                    hasOverdue ? (
                                        <span className="num text-destructive-soft-foreground">
                                            {summary.earliestOverdueDate
                                                ? `最早逾期 ${summary.earliestOverdueDate}`
                                                : "请及时跟进回款"}
                                        </span>
                                    ) : (
                                        "当前无逾期"
                                    )
                                }
                            />
                            <DetailWorkspaceMetricItem
                                label="可开票余额"
                                value={
                                    <MoneyValue
                                        value={summary.openInvoiceableTotal}
                                    />
                                }
                                detail={invoiceCompleted ? "已开齐" : "待开票"}
                            />
                        </DetailWorkspaceMetrics>

                        {summary.reliabilityNote ? (
                            <p className="text-xs text-muted-foreground">
                                {summary.reliabilityNote}
                            </p>
                        ) : null}
                    </div>
                ) : (
                    <BusinessEmptyState
                        kind="no-data"
                        title="暂无票款摘要"
                        description="系统暂无该客户的应收或开票数据。"
                        className="rounded-lg border-0 bg-transparent px-0 py-4 shadow-none ring-0"
                    />
                )}
            </DetailWorkspaceSection>

            <DetailWorkspaceSection
                title="经营摘要"
                width="full"
                actions={
                    <Button
                        id="customers-detail-quality-open"
                        type="button"
                        size="sm"
                        variant="ghost"
                        className={detailWorkspaceStyles.subtleButton}
                        render={<Link href={qualityHref} />}
                    >
                        打开经营质量
                        <ArrowUpRightIcon
                            data-icon="inline-end"
                            aria-hidden="true"
                        />
                    </Button>
                }
            >
                <AsyncSectionState
                    status={
                        customer.partitions.quality === "error"
                            ? "error"
                            : "success"
                    }
                    error="经营分析数据暂时无法加载，请稍后重试"
                    errorKind="projection"
                    retryAction={
                        <Button
                            id="customers-detail-quality-retry"
                            type="button"
                            size="sm"
                            onClick={() => void refetch()}
                        >
                            重新加载
                        </Button>
                    }
                >
                    {customer.partitions.quality === "ok" &&
                    customer.qualitySummary ? (
                        <div className="space-y-4">
                            <DetailWorkspaceFacts
                                columns={4}
                                ariaLabel="经营分层标签"
                            >
                                <DetailWorkspaceFact
                                    label="规模标签"
                                    value={
                                        <Badge variant="neutral">
                                            {customer.qualitySummary.scaleLabel}
                                        </Badge>
                                    }
                                />
                                <DetailWorkspaceFact
                                    label="利润贡献"
                                    value={
                                        <Badge variant="neutral">
                                            {
                                                customer.qualitySummary
                                                    .profitContributionLabel
                                            }
                                        </Badge>
                                    }
                                />
                                <DetailWorkspaceFact
                                    label="回款风险"
                                    value={
                                        <Badge variant="neutral">
                                            {
                                                customer.qualitySummary
                                                    .collectionRiskLabel
                                            }
                                        </Badge>
                                    }
                                />
                                <DetailWorkspaceFact
                                    label="最近业务"
                                    numeric
                                    value={
                                        customer.qualitySummary.lastBusinessAt
                                            ? formatDateTime(
                                                  customer.qualitySummary
                                                      .lastBusinessAt,
                                              )
                                            : undefined
                                    }
                                />
                            </DetailWorkspaceFacts>
                            <DataFreshness
                                updatedAt={formatDateTime(
                                    customer.qualitySummary.projectionAt,
                                )}
                                dateTime={customer.qualitySummary.projectionAt}
                                state={
                                    customer.qualitySummary.isStale
                                        ? "stale"
                                        : "fresh"
                                }
                                label="经营质量汇总于"
                            />
                        </div>
                    ) : customer.partitions.quality === "ok" ? (
                        <BusinessEmptyState
                            kind="no-data"
                            title="暂无经营摘要"
                            description="该客户在当前分析周期内尚未生成经营分层标签。"
                            className="rounded-lg border-0 bg-transparent px-0 py-4 shadow-none ring-0"
                        />
                    ) : null}
                </AsyncSectionState>
            </DetailWorkspaceSection>
        </>
    )
}
