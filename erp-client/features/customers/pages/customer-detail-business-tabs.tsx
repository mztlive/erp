"use client"

import Link from "next/link"

import {
    BusinessEmptyState,
    BusinessFailureState,
    BusinessStatusBadge,
    DocumentSection,
    DocumentSummary,
} from "@/components/business"
import { Button } from "@/components/ui/button"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { openWorkspaceLabel } from "@/lib/ui-text"
import type { CustomerCenterView } from "@/features/customers/types"
import {
    DetailRecordColumn,
    DetailRecordColumns,
    DetailRecordRow,
    detailSectionClassName,
    embeddedSummaryClassName,
} from "@/features/customers/pages/customer-detail-records"

function RelatedColumn({
    label,
    empty,
    items,
}: {
    label: string
    empty: string
    items: CustomerCenterView["contracts"]
}) {
    return (
        <DetailRecordColumn label={label}>
            {items.length === 0 ? (
                <p className="py-2 text-sm text-muted-foreground">{empty}</p>
            ) : (
                items.map((item) => (
                    <DetailRecordRow key={item.id}>
                        <Link
                            id={`customers-detail-related-${toAutomationIdSegment(item.id)}-number`}
                            href={item.href}
                            className="num font-medium underline-offset-4 hover:underline focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
                        >
                            {item.number}
                        </Link>
                        <BusinessStatusBadge context="list" {...item.status} />
                        <span className="min-w-0 text-muted-foreground">
                            {item.title}
                        </span>
                        {item.detail ? (
                            <span className="ml-auto text-muted-foreground">
                                {item.detail}
                            </span>
                        ) : null}
                    </DetailRecordRow>
                ))
            )}
        </DetailRecordColumn>
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
        <DocumentSection
            className={detailSectionClassName}
            title="合同与销售"
            action={
                <div className="flex flex-wrap gap-2">
                    <Button
                        id="customers-detail-related-view-contracts"
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
                        id="customers-detail-related-view-orders"
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
                            id="customers-detail-related-retry"
                            type="button"
                            size="sm"
                            onClick={() => void refetch()}
                        >
                            重试
                        </Button>
                    }
                />
            ) : (
                <DetailRecordColumns>
                    <RelatedColumn
                        label="合同（最近）"
                        empty="暂无合同摘要"
                        items={customer.contracts}
                    />
                    <RelatedColumn
                        label="销售单（最近）"
                        empty="暂无销售单摘要"
                        items={customer.salesOrders}
                    />
                </DetailRecordColumns>
            )}
        </DocumentSection>
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
        <DocumentSection
            className={detailSectionClassName}
            title="票款摘要"
            description="只读，不在此核销或开票。"
            action={
                <Button
                    id="customers-detail-settlement-open-accounts"
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
                            id="customers-detail-settlement-retry"
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
                        className={embeddedSummaryClassName}
                        columns="three"
                        items={[
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
    )
}
