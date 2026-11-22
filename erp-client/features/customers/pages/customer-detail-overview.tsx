"use client"

import { PencilIcon } from "lucide-react"

import {
    BusinessFailureState,
    DetailWorkspaceSection,
    detailWorkspaceStyles,
    DocumentSummary,
    MetricItem,
    MetricStrip,
    MoneyValue,
} from "@/components/business"
import { Button } from "@/components/ui/button"
import { paymentTermLabel } from "@/lib/business-options"
import { CustomerForm } from "@/features/customers/components/customer-form"
import type { CustomerCenterView } from "@/features/customers/types"
import {
    can,
    ownerLabel,
} from "@/features/customers/pages/customer-detail-helpers"
import { CustomerDetailContactSections } from "@/features/customers/pages/customer-detail-contact-sections"
import { embeddedSummaryClassName } from "@/features/customers/pages/customer-detail-records"

export function CustomerDetailMetrics({
    customer,
}: {
    customer: CustomerCenterView
}) {
    return (
        <MetricStrip
            columns={4}
            density="compact"
            variant="plain"
            className="grid-cols-2 py-3.5 lg:[&>*+*]:border-l lg:[&>*+*]:border-border/60 lg:[&>*+*]:pl-6"
            aria-label="关系指标"
            aria-live="polite"
        >
            <MetricItem
                density="compact"
                label="有效合同"
                value={String(customer.metrics.activeContractCount ?? "—")}
                detail={
                    customer.metrics.expiringContractCount
                        ? `${customer.metrics.expiringContractCount} 将到期`
                        : undefined
                }
                detailMode="inline"
            />
            <MetricItem
                density="compact"
                label="进行中销售单"
                value={String(
                    customer.metrics.inProgressSalesOrderCount ?? "—",
                )}
                detail="该客户当前履约中的销售单总数"
                detailMode="tooltip"
            />
            <MetricItem
                density="compact"
                label="应收余额"
                value={
                    <MoneyValue value={customer.metrics.receivableBalance} />
                }
                detail="客户往来汇总"
                detailMode="tooltip"
            />
            <MetricItem
                density="compact"
                label="逾期金额"
                value={<MoneyValue value={customer.metrics.overdueAmount} />}
                detailMode="none"
            />
        </MetricStrip>
    )
}

export function CustomerDetailIdentityMeta({
    customer,
}: {
    customer: CustomerCenterView
}) {
    return (
        <span>
            负责{" "}
            <span className="font-medium text-foreground">
                {ownerLabel(customer)}
            </span>
        </span>
    )
}

export function CustomerDetailOverviewTab({
    customer,
    refetch,
    editing,
    onEditClick,
    onFormDirtyChange,
    shortcutBlocked,
    onFormCancel,
    onFormSucceeded,
}: {
    customer: CustomerCenterView
    refetch: () => void
    editing: boolean
    onEditClick: () => void
    onFormDirtyChange: (isDirty: boolean) => void
    shortcutBlocked: boolean
    onFormCancel: () => void
    onFormSucceeded: (customerId: string, revisionNo?: number) => void
}) {
    const editBlocked = !can(customer, "EDIT_CUSTOMER")

    if (editing) {
        return (
            <CustomerForm
                key={customer.customerId}
                mode="edit"
                grouped
                shortcutBlocked={shortcutBlocked}
                customer={customer}
                onDirtyChange={onFormDirtyChange}
                onCancel={onFormCancel}
                onSucceeded={onFormSucceeded}
            />
        )
    }

    return (
        <>
            <DetailWorkspaceSection
                title="客户基本资料"
                width="form"
                actions={
                    !editBlocked ? (
                        <Button
                            id="customers-detail-overview-edit"
                            type="button"
                            size="sm"
                            variant="ghost"
                            className={detailWorkspaceStyles.subtleButton}
                            onClick={onEditClick}
                        >
                            <PencilIcon
                                data-icon="inline-start"
                                aria-hidden="true"
                            />
                            编辑资料
                        </Button>
                    ) : null
                }
            >
                {customer.partitions.identity === "error" ? (
                    <BusinessFailureState
                        kind="system"
                        description="基本信息加载失败"
                        action={
                            <Button
                                id="customers-detail-overview-retry"
                                type="button"
                                size="sm"
                                onClick={() => void refetch()}
                            >
                                重新加载
                            </Button>
                        }
                    />
                ) : (
                    <DocumentSummary
                        className={embeddedSummaryClassName}
                        columns="four"
                        items={[
                            {
                                id: "legalName",
                                label: "法定名称",
                                value: customer.currentRevision.legalName,
                            },
                            {
                                id: "shortName",
                                label: "客户简称",
                                value:
                                    customer.currentRevision.shortName ?? "—",
                            },
                            {
                                id: "credit",
                                label: "统一社会信用代码",
                                value:
                                    customer.currentRevision
                                        .unifiedCreditCode ?? "—",
                            },
                            {
                                id: "payment",
                                label: "默认付款条件",
                                value: customer.currentRevision
                                    .defaultPaymentTerm
                                    ? paymentTermLabel(
                                          customer.currentRevision
                                              .defaultPaymentTerm,
                                      )
                                    : "未设置（开单时按需选择）",
                            },
                        ]}
                    />
                )}
            </DetailWorkspaceSection>

            <CustomerDetailContactSections
                customer={customer}
                refetch={refetch}
            />
        </>
    )
}
