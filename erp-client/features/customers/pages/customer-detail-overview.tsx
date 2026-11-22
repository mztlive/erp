"use client"

import {
    BusinessFailureState,
    DetailWorkspaceMetricItem,
    DetailWorkspaceMetrics,
    DetailWorkspaceSection,
    DocumentSummary,
    MoneyValue,
} from "@/components/business"
import { Button } from "@/components/ui/button"
import { StatusBadge } from "@/components/ui/status-badge"
import { useAccountProfileQuery } from "@/features/auth/queries"
import { CustomerForm } from "@/features/customers/components/customer-form"
import {
    CustomerDetailContactSections,
    CustomerReadonlyAddressesSection,
    CustomerReadonlyBankAccountsSection,
    CustomerReadonlyContactsSection,
} from "@/features/customers/pages/customer-detail-contact-sections"
import {
    can,
    ownerLabel,
} from "@/features/customers/pages/customer-detail-helpers"
import { embeddedSummaryClassName } from "@/features/customers/pages/customer-detail-records"
import type { CustomerCenterView } from "@/features/customers/types"
import { paymentTermLabel } from "@/lib/business-options"
import { compareDecimal } from "@/lib/fixed-decimal"
import { hasPermission } from "@/lib/permissions"

export function CustomerDetailMetrics({
    customer,
}: {
    customer: CustomerCenterView
}) {
    const overdue = customer.metrics.overdueAmount
    const hasOverdue =
        overdue != null &&
        overdue.trim() !== "" &&
        compareDecimal(overdue, "0", 2) > 0

    return (
        <DetailWorkspaceMetrics columns={4} variant="bar" ariaLabel="关系指标">
            <DetailWorkspaceMetricItem
                label="有效合同"
                value={
                    customer.metrics.activeContractCount != null
                        ? String(customer.metrics.activeContractCount)
                        : "—"
                }
                detail={
                    customer.metrics.expiringContractCount ? (
                        <span className="num text-warning-soft-foreground">
                            {customer.metrics.expiringContractCount} 份将到期
                        </span>
                    ) : undefined
                }
            />
            <DetailWorkspaceMetricItem
                label="进行中销售单"
                value={
                    customer.metrics.inProgressSalesOrderCount != null
                        ? String(customer.metrics.inProgressSalesOrderCount)
                        : "—"
                }
                detail="履约中的销售单总数"
            />
            <DetailWorkspaceMetricItem
                emphasis="primary"
                label="应收余额"
                value={
                    <MoneyValue value={customer.metrics.receivableBalance} />
                }
                detail="客户往来汇总"
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
                    <MoneyValue
                        value={overdue}
                        className={hasOverdue ? "text-destructive" : undefined}
                    />
                }
                detail={hasOverdue ? "请及时跟进回款" : undefined}
            />
        </DetailWorkspaceMetrics>
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
    formResetEpoch,
    onFormDirtyChange,
    shortcutBlocked,
    onFormReset,
    onFormSucceeded,
}: {
    customer: CustomerCenterView
    refetch: () => void
    formResetEpoch: number
    onFormDirtyChange: (isDirty: boolean) => void
    shortcutBlocked: boolean
    onFormReset: () => void
    onFormSucceeded: (customerId: string, revisionNo?: number) => void
}) {
    const accountProfile = useAccountProfileQuery()
    const canEdit =
        can(customer, "EDIT_CUSTOMER") &&
        hasPermission(accountProfile.data?.permissions, "customer:update")

    if (canEdit) {
        return (
            <CustomerForm
                key={`${customer.customerId}-${formResetEpoch}`}
                mode="edit"
                grouped
                shortcutBlocked={shortcutBlocked}
                customer={customer}
                onDirtyChange={onFormDirtyChange}
                onCancel={onFormReset}
                onSucceeded={onFormSucceeded}
                readonlyContactsFallback={
                    <CustomerReadonlyContactsSection
                        customer={customer}
                        refetch={refetch}
                    />
                }
                readonlyAddressesFallback={
                    <CustomerReadonlyAddressesSection
                        customer={customer}
                        refetch={refetch}
                    />
                }
                readonlyBanksFallback={
                    <CustomerReadonlyBankAccountsSection customer={customer} />
                }
            />
        )
    }

    return (
        <>
            <DetailWorkspaceSection title="客户基本资料" width="form">
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
                                numeric: true,
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
