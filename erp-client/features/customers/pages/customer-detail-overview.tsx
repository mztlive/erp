"use client"

import * as React from "react"
import { CheckIcon, ChevronRightIcon, CopyIcon } from "lucide-react"

import {
    BusinessFailureState,
    DetailWorkspaceFact,
    DetailWorkspaceFacts,
    DetailWorkspaceMeta,
    DetailWorkspaceMetricItem,
    DetailWorkspaceMetrics,
    DetailWorkspaceSection,
    MoneyValue,
} from "@/components/business"
import { Button } from "@/components/ui/button"
import { StatusBadge } from "@/components/ui/status-badge"
import { toast } from "@/components/ui/toast"
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
    type CustomerActiveSectionId,
} from "@/features/customers/pages/customer-detail-helpers"
import type { CustomerCenterView } from "@/features/customers/types"
import { paymentTermLabel } from "@/lib/business-options"
import { formatDateTime } from "@/lib/datetime"
import { compareDecimal } from "@/lib/fixed-decimal"
import { hasPermission } from "@/lib/permissions"

function ClickableMetricTile({
    id,
    ariaLabel,
    onClick,
    children,
}: {
    id: string
    ariaLabel: string
    onClick: () => void
    children: React.ReactNode
}) {
    return (
        <button
            id={id}
            type="button"
            aria-label={ariaLabel}
            onClick={onClick}
            className="group -mx-2 -my-1 flex w-[calc(100%+1rem)] min-w-0 cursor-pointer items-start justify-between gap-2 rounded-lg px-2 py-1 text-left transition-colors hover:bg-muted/45 focus-visible:outline-2 focus-visible:outline-primary"
        >
            <div className="min-w-0 flex-1">{children}</div>
            <ChevronRightIcon
                aria-hidden="true"
                className="mt-0.5 size-3.5 shrink-0 text-muted-foreground/40 transition-colors group-hover:text-foreground"
            />
        </button>
    )
}

export function CustomerDetailMetrics({
    customer,
    onSelectSection,
}: {
    customer: CustomerCenterView
    onSelectSection: (section: CustomerActiveSectionId) => void
}) {
    const overdue = customer.metrics.overdueAmount
    const hasOverdue =
        overdue != null &&
        overdue.trim() !== "" &&
        compareDecimal(overdue, "0", 2) > 0
    const openInvoiceable = customer.receivableSummary?.openInvoiceableTotal
    const hasOpenInvoiceable =
        openInvoiceable != null &&
        openInvoiceable.trim() !== "" &&
        compareDecimal(openInvoiceable, "0", 2) > 0

    return (
        <DetailWorkspaceMetrics columns={4} variant="bar" ariaLabel="关系指标">
            <div>
                <ClickableMetricTile
                    id="customers-detail-metric-contracts"
                    ariaLabel="查看客户有效合同"
                    onClick={() => onSelectSection("related")}
                >
                    <DetailWorkspaceMetricItem
                        label="有效合同"
                        value={
                            customer.metrics.activeContractCount != null
                                ? `${customer.metrics.activeContractCount} 份`
                                : "—"
                        }
                        detail={
                            customer.partitions.related === "error" ? (
                                "关联数据加载失败"
                            ) : customer.metrics.expiringContractCount ? (
                                <span className="num text-warning-soft-foreground">
                                    {customer.metrics.expiringContractCount}{" "}
                                    份将到期
                                </span>
                            ) : customer.contracts.length > 0 ? (
                                `最近记录 ${customer.contracts.length} 份`
                            ) : (
                                "暂无有效合同"
                            )
                        }
                    />
                </ClickableMetricTile>
            </div>
            <div>
                <ClickableMetricTile
                    id="customers-detail-metric-orders"
                    ariaLabel="查看客户进行中销售单"
                    onClick={() => onSelectSection("related")}
                >
                    <DetailWorkspaceMetricItem
                        label="进行中销售单"
                        value={
                            customer.metrics.inProgressSalesOrderCount != null
                                ? `${customer.metrics.inProgressSalesOrderCount} 单`
                                : "—"
                        }
                        detail={
                            customer.partitions.related === "error"
                                ? "关联数据加载失败"
                                : customer.salesOrders.length > 0
                                  ? `最近记录 ${customer.salesOrders.length} 单`
                                  : "暂无进行中单据"
                        }
                    />
                </ClickableMetricTile>
            </div>
            <div>
                <ClickableMetricTile
                    id="customers-detail-metric-receivable"
                    ariaLabel="查看客户票款与应收详情"
                    onClick={() => onSelectSection("settlement")}
                >
                    <DetailWorkspaceMetricItem
                        label="应收余额"
                        value={
                            <MoneyValue
                                value={customer.metrics.receivableBalance}
                                showHint={false}
                            />
                        }
                        detail={
                            customer.partitions.settlement === "error" ? (
                                "票款数据加载失败"
                            ) : hasOpenInvoiceable ? (
                                <span className="inline-flex items-baseline gap-1">
                                    <span>可开票</span>
                                    <MoneyValue
                                        value={openInvoiceable}
                                        showHint={false}
                                        className="font-normal text-muted-foreground"
                                    />
                                </span>
                            ) : (
                                (customer.receivableSummary
                                    ?.collectionProgressLabel ?? "暂无往来数据")
                            )
                        }
                    />
                </ClickableMetricTile>
            </div>
            <div>
                <ClickableMetricTile
                    id="customers-detail-metric-overdue"
                    ariaLabel="查看客户逾期与回款详情"
                    onClick={() => onSelectSection("settlement")}
                >
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
                        value={<MoneyValue value={overdue} showHint={false} />}
                        detail={
                            customer.partitions.settlement === "error" ? (
                                "票款数据加载失败"
                            ) : hasOverdue ? (
                                <span className="text-destructive-soft-foreground">
                                    {customer.receivableSummary
                                        ?.earliestOverdueDate
                                        ? `最早逾期 ${customer.receivableSummary.earliestOverdueDate}`
                                        : "请及时跟进回款"}
                                </span>
                            ) : (
                                "当前无逾期"
                            )
                        }
                    />
                </ClickableMetricTile>
            </div>
        </DetailWorkspaceMetrics>
    )
}

export function CustomerDetailIdentityMeta({
    customer,
}: {
    customer: CustomerCenterView
}) {
    const shortName = customer.currentRevision.shortName?.trim()
    const distinctShortName =
        shortName && shortName !== customer.currentRevision.legalName.trim()
            ? shortName
            : undefined
    const paymentTerm = customer.currentRevision.defaultPaymentTerm
        ? paymentTermLabel(customer.currentRevision.defaultPaymentTerm)
        : undefined

    return (
        <DetailWorkspaceMeta
            items={[
                distinctShortName ? `简称：${distinctShortName}` : null,
                paymentTerm ? `付款：${paymentTerm}` : null,
                <span key="owner">
                    负责{" "}
                    <span className="font-medium text-foreground">
                        {ownerLabel(customer)}
                    </span>
                </span>,
            ]}
        />
    )
}

function CopyCreditCodeButton({ value }: { value: string }) {
    const [copied, setCopied] = React.useState(false)

    const handleCopy = React.useCallback(() => {
        if (typeof navigator === "undefined" || !navigator.clipboard) return
        void navigator.clipboard.writeText(value).then(() => {
            setCopied(true)
            toast.add({
                title: "已复制统一社会信用代码",
                type: "success",
                timeout: 2000,
            })
            window.setTimeout(() => setCopied(false), 2000)
        })
    }, [value])

    return (
        <Button
            id="customers-detail-copy-credit-code"
            type="button"
            variant="ghost"
            size="xs"
            className="h-6 px-1.5 text-muted-foreground hover:text-foreground"
            aria-label="复制统一社会信用代码"
            onClick={handleCopy}
        >
            {copied ? (
                <CheckIcon
                    className="size-3.5 text-success"
                    aria-hidden="true"
                />
            ) : (
                <CopyIcon className="size-3.5" aria-hidden="true" />
            )}
            {copied ? "已复制" : "复制"}
        </Button>
    )
}

export function CustomerDetailOverviewTab({
    customer,
    refetch,
    editing,
    onEditClick,
    formResetEpoch,
    onFormDirtyChange,
    shortcutBlocked,
    onFormReset,
    onFormSucceeded,
}: {
    customer: CustomerCenterView
    refetch: () => void
    editing: boolean
    onEditClick: () => void
    formResetEpoch: number
    onFormDirtyChange: (isDirty: boolean) => void
    shortcutBlocked: boolean
    onFormReset: () => void
    onFormSucceeded: (customerId: string, revisionNo?: number) => void
}) {
    const accountProfile = useAccountProfileQuery()
    const permissions = accountProfile.data?.permissions
    const canEdit =
        can(customer, "EDIT_CUSTOMER") &&
        hasPermission(permissions, "customer:update")
    const canReadContacts = accountProfile.isSuccess
        ? hasPermission(permissions, "party_contact:detail")
        : customer.contacts.length > 0
    const canReadAddresses = accountProfile.isSuccess
        ? hasPermission(permissions, "party_address:detail")
        : customer.addresses.length > 0
    const canReadBanks = accountProfile.isSuccess
        ? hasPermission(permissions, "party_bank_account:detail")
        : customer.bankAccounts.length > 0

    if (editing && canEdit) {
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
                    canReadContacts ? (
                        <CustomerReadonlyContactsSection
                            customer={customer}
                            refetch={refetch}
                        />
                    ) : null
                }
                readonlyAddressesFallback={
                    canReadAddresses ? (
                        <CustomerReadonlyAddressesSection
                            customer={customer}
                            refetch={refetch}
                        />
                    ) : null
                }
                readonlyBanksFallback={
                    canReadBanks ? (
                        <CustomerReadonlyBankAccountsSection
                            customer={customer}
                        />
                    ) : null
                }
            />
        )
    }

    const creditCode = customer.currentRevision.unifiedCreditCode?.trim()

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
                    <DetailWorkspaceFacts columns={2} ariaLabel="客户基本资料">
                        <DetailWorkspaceFact
                            label="法定名称"
                            value={customer.currentRevision.legalName}
                        />
                        <DetailWorkspaceFact
                            label="客户简称"
                            value={customer.currentRevision.shortName}
                        />
                        <DetailWorkspaceFact
                            label="统一社会信用代码"
                            numeric
                            value={
                                creditCode ? (
                                    <span className="inline-flex flex-wrap items-center gap-1.5">
                                        <span className="num">
                                            {creditCode}
                                        </span>
                                        <CopyCreditCodeButton
                                            value={creditCode}
                                        />
                                    </span>
                                ) : undefined
                            }
                        />
                        <DetailWorkspaceFact
                            label="默认付款条件"
                            value={
                                customer.currentRevision.defaultPaymentTerm
                                    ? paymentTermLabel(
                                          customer.currentRevision
                                              .defaultPaymentTerm,
                                      )
                                    : "未设置（开单时按需选择）"
                            }
                        />
                        <DetailWorkspaceFact
                            label="客户状态"
                            value={
                                <StatusBadge
                                    tone={customer.statusLabel.tone}
                                    label={customer.statusLabel.label}
                                />
                            }
                        />
                        <DetailWorkspaceFact
                            label="资料更新"
                            numeric
                            value={`v${customer.currentRevision.revisionNo} · ${formatDateTime(customer.freshness.formalFactsAt)}`}
                        />
                    </DetailWorkspaceFacts>
                )}
            </DetailWorkspaceSection>

            <CustomerDetailContactSections
                customer={customer}
                refetch={refetch}
                onEditClick={canEdit ? onEditClick : undefined}
            />
        </>
    )
}
