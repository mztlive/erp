"use client"

import Link from "next/link"

import {
    BusinessEmptyState,
    BusinessFailureState,
    BusinessStatusBadge,
    DocumentSection,
    DocumentSummary,
    MoneyValue,
    surfaceInsetClassName,
} from "@/components/business"
import { Button } from "@/components/ui/button"
import {
    Card,
    CardContent,
    CardHeader,
    CardTitle,
} from "@/components/ui/card"
import { cn } from "@/lib/utils"
import { openWorkspaceLabel } from "@/lib/ui-text"
import type { CustomerCenterView } from "@/features/customers/types"

function RelatedList({
    title,
    empty,
    items,
}: {
    title: string
    empty: string
    items: CustomerCenterView["contracts"]
}) {
    return (
        <Card size="sm" className="shadow-none ring-1 ring-foreground/[0.04]">
            <CardHeader className="border-b border-grid">
                <CardTitle className="text-sm">{title}</CardTitle>
            </CardHeader>
            <CardContent className="space-y-2">
                {items.length === 0 ? (
                    <p className="text-sm text-muted-foreground">{empty}</p>
                ) : (
                    items.map((item) => (
                        <div
                            key={item.id}
                            className={cn(
                                surfaceInsetClassName,
                                "flex flex-wrap items-center justify-between gap-2 px-3 py-2 text-sm",
                            )}
                        >
                            <div className="min-w-0">
                                <div className="flex flex-wrap items-center gap-2">
                                    <Link
                                        href={item.href}
                                        className="num font-medium underline-offset-4 hover:underline focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
                                    >
                                        {item.number}
                                    </Link>
                                    <BusinessStatusBadge
                                        context="list"
                                        {...item.status}
                                    />
                                </div>
                                <p className="text-muted-foreground">
                                    {item.title}
                                </p>
                                {item.detail ? (
                                    <p className="text-xs text-muted-foreground">
                                        {item.detail}
                                    </p>
                                ) : null}
                            </div>
                            <Button
                                type="button"
                                size="sm"
                                variant="ghost"
                                render={<Link href={item.href} />}
                            >
                                打开
                            </Button>
                        </div>
                    ))
                )}
            </CardContent>
        </Card>
    )
}

export function CustomerDetailRelatedTab({
    customer,
    refetch,
}: {
    customer: CustomerCenterView
    refetch: () => void
}) {
    return (
        <div className="space-y-4 pt-4">
            <DocumentSection
                title="合同与销售"
                description="以下列出最近合同与进行中销售单。"
                action={
                    <div className="flex flex-wrap gap-2">
                        <Button
                            type="button"
                            size="sm"
                            variant="ghost"
                            render={
                                <Link
                                    href={`/sales/contracts?customerId=${encodeURIComponent(customer.customerId)}`}
                                />
                            }
                        >
                            查看全部合同
                        </Button>
                        <Button
                            type="button"
                            size="sm"
                            variant="ghost"
                            render={
                                <Link
                                    href={`/sales/orders?customerId=${encodeURIComponent(customer.customerId)}`}
                                />
                            }
                        >
                            查看全部销售单
                        </Button>
                    </div>
                }
            >
                {customer.partitions.related === "error" ? (
                    <BusinessFailureState
                        kind="system"
                        description="关联业务分区失败；主体与其它分区仍保留。"
                        action={
                            <Button
                                type="button"
                                size="sm"
                                onClick={() => void refetch()}
                            >
                                重试
                            </Button>
                        }
                    />
                ) : (
                    <div className="grid gap-4 lg:grid-cols-2">
                        <RelatedList
                            title="合同（最近）"
                            empty="暂无合同摘要"
                            items={customer.contracts}
                        />
                        <RelatedList
                            title="销售单（最近）"
                            empty="暂无销售单摘要"
                            items={customer.salesOrders}
                        />
                    </div>
                )}
            </DocumentSection>
        </div>
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

    return (
        <div className="space-y-4 pt-4">
            <DocumentSection
                title="票款摘要"
                description="只读应收汇总；不在此核销或开票。往来详情进入客户往来。"
                action={
                    <Button
                        type="button"
                        size="sm"
                        variant="ghost"
                        render={<Link href={receivableHref} />}
                    >
                        {openWorkspaceLabel("W11")}
                    </Button>
                }
            >
                {customer.partitions.settlement === "error" ? (
                    <BusinessFailureState
                        kind="system"
                        description="票款分区失败；主体身份仍保留。"
                        action={
                            <Button
                                type="button"
                                size="sm"
                                onClick={() => void refetch()}
                            >
                                重试
                            </Button>
                        }
                    />
                ) : customer.receivableSummary ? (
                    <div className="space-y-3">
                        <DocumentSummary
                            columns="two"
                            items={[
                                {
                                    id: "ar",
                                    label: "应收余额",
                                    value: (
                                        <MoneyValue
                                            value={
                                                customer.receivableSummary
                                                    .receivableBalance
                                            }
                                        />
                                    ),
                                    numeric: true,
                                },
                                {
                                    id: "od",
                                    label: "逾期金额",
                                    value: (
                                        <MoneyValue
                                            value={
                                                customer.receivableSummary
                                                    .overdueAmount
                                            }
                                        />
                                    ),
                                    numeric: true,
                                },
                                {
                                    id: "earliest",
                                    label: "最早逾期日",
                                    value:
                                        customer.receivableSummary
                                            .earliestOverdueDate ?? "—",
                                },
                                {
                                    id: "coll",
                                    label: "回款进度",
                                    value:
                                        customer.receivableSummary
                                            .collectionProgressLabel ?? "—",
                                },
                                {
                                    id: "inv",
                                    label: "开票进度",
                                    value:
                                        customer.receivableSummary
                                            .invoicingProgressLabel ?? "—",
                                },
                            ]}
                        />
                        <p className="text-xs text-muted-foreground">
                            应收余额与逾期金额与顶部指标一致，并非增量数据。
                        </p>
                        {customer.receivableSummary.reliabilityNote ? (
                            <p className="text-xs text-muted-foreground">
                                {customer.receivableSummary.reliabilityNote}
                            </p>
                        ) : null}
                    </div>
                ) : (
                    <BusinessEmptyState
                        kind="no-data"
                        title="暂无票款摘要"
                        description="系统暂无应收数据。"
                        className="rounded-lg border-0 bg-transparent p-6 shadow-none ring-0"
                    />
                )}
            </DocumentSection>
        </div>
    )
}
