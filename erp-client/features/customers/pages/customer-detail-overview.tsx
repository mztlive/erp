"use client"

import { PencilIcon } from "lucide-react"

import {
    BusinessFailureState,
    DocumentSection,
    DocumentSummary,
    MetricItem,
    MetricStrip,
    MoneyValue,
} from "@/components/business"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Button } from "@/components/ui/button"
import { paymentTermLabel } from "@/lib/business-options"
import { CustomerForm } from "@/features/customers/components/customer-form"
import type { CustomerCenterView } from "@/features/customers/types"
import {
    can,
    collaboratorCount,
    collaboratorShortNames,
    collaboratorSummary,
    ownerLabel,
} from "@/features/customers/pages/customer-detail-helpers"
import { CustomerDetailContactSections } from "@/features/customers/pages/customer-detail-contact-sections"

export function CustomerDetailMetrics({
    customer,
}: {
    customer: CustomerCenterView
}) {
    return (
        <MetricStrip
            columns={4}
            density="compact"
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
                detail="正式关联数据完整分页汇总"
                detailMode="tooltip"
            />
            <MetricItem
                density="compact"
                label="应收余额"
                value={<MoneyValue value={customer.metrics.receivableBalance} />}
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
    const collabCount = collaboratorCount(customer)
    return (
        <span className="inline-flex flex-wrap items-center gap-x-1.5 gap-y-0.5">
            <span>
                负责{" "}
                <span className="font-medium text-foreground">
                    {ownerLabel(customer)}
                </span>
            </span>
            <span className="text-border" aria-hidden="true">
                ·
            </span>
            <span title={collaboratorSummary(customer)}>
                协作{" "}
                <span className="font-medium text-foreground">
                    {collabCount > 0
                        ? `${collabCount} 人（${collaboratorShortNames(customer)}）`
                        : "无"}
                </span>
            </span>
        </span>
    )
}

export function CustomerDetailOverviewTab({
    customer,
    refetch,
    editing,
    savedNotice,
    onEditClick,
    onFormDirtyChange,
    onFormCancel,
    onFormSucceeded,
    onDismissSavedNotice,
}: {
    customer: CustomerCenterView
    refetch: () => void
    editing: boolean
    savedNotice: { revisionNo: number } | null
    onEditClick: () => void
    onFormDirtyChange: (isDirty: boolean) => void
    onFormCancel: () => void
    onFormSucceeded: (customerId: string, revisionNo?: number) => void
    onDismissSavedNotice: () => void
}) {
    const editBlocked = !can(customer, "EDIT_CUSTOMER")

    return (
        <div className="space-y-4 pt-4">
            {editing ? (
                <CustomerForm
                    mode="edit"
                    grouped
                    customer={customer}
                    onDirtyChange={onFormDirtyChange}
                    onCancel={onFormCancel}
                    onSucceeded={onFormSucceeded}
                />
            ) : (
                <>
                    {savedNotice ? (
                        <Alert variant="success">
                            <AlertTitle>客户资料已保存</AlertTitle>
                            <AlertDescription className="flex flex-wrap items-center justify-between gap-2">
                                <span>
                                    新版本 v{savedNotice.revisionNo}{" "}
                                    已生效，历史单据记录不变。
                                </span>
                                <Button
                                    type="button"
                                    size="sm"
                                    variant="ghost"
                                    onClick={onDismissSavedNotice}
                                >
                                    知道了
                                </Button>
                            </AlertDescription>
                        </Alert>
                    ) : null}
                    <DocumentSection
                        title="主体身份与客户角色"
                        description="当前基础资料版本，不覆盖历史单据记录"
                        action={
                            !editBlocked ? (
                                <Button
                                    type="button"
                                    size="sm"
                                    variant="outline"
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
                                description="主体分区加载失败。"
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
                            <DocumentSummary
                                columns="two"
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
                                            customer.currentRevision.shortName ??
                                            "—",
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
                                            : "—（仅录单提示）",
                                    },
                                    {
                                        id: "revision",
                                        label: "基础资料版本",
                                        value: `v${customer.currentRevision.revisionNo} · ${customer.currentRevision.effectiveFrom.slice(0, 10)} 更新`,
                                    },
                                    {
                                        id: "owner",
                                        label: "负责销售",
                                        value: ownerLabel(customer),
                                    },
                                ]}
                            />
                        )}
                    </DocumentSection>

                    <CustomerDetailContactSections
                        customer={customer}
                        refetch={refetch}
                    />
                </>
            )}
        </div>
    )
}
