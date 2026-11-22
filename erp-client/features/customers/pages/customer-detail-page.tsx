"use client"

import * as React from "react"
import Link from "next/link"
import { FilePlus2Icon, ShoppingCartIcon } from "lucide-react"

import {
    BusinessEmptyState,
    BusinessFailureState,
    DetailWorkspaceBody,
    DetailWorkspaceChrome,
    DetailWorkspaceDirtyBadge,
    DetailWorkspacePage,
    DetailWorkspaceSection,
    DetailWorkspaceSkeleton,
    DetailWorkspaceTabs,
    DiscardConfirmDialog,
    GuardedBusinessAction,
    detailWorkspaceStyles,
} from "@/components/business"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Button } from "@/components/ui/button"
import { CustomerAssignmentDialog } from "@/features/customers/components/customer-assignment-dialog"
import { useCustomerDetailState } from "@/features/customers/hooks/use-customer-detail-state"
import {
    CustomerDetailRelatedTab,
    CustomerDetailSettlementTab,
} from "@/features/customers/pages/customer-detail-business-tabs"
import {
    CustomerDetailAuditTab,
    CustomerDetailQualityTab,
} from "@/features/customers/pages/customer-detail-governance-tabs"
import {
    SECTION_NAV,
    blocker,
    can,
} from "@/features/customers/pages/customer-detail-helpers"
import {
    CustomerDetailIdentityMeta,
    CustomerDetailMetrics,
    CustomerDetailOverviewTab,
} from "@/features/customers/pages/customer-detail-overview"
import type { CustomerSectionId } from "@/features/customers/types"

const backToCustomers = {
    id: "customers-detail-back",
    label: "客户列表",
    href: "/sales/customers",
} as const

export function CustomerDetailPage({
    customerId,
    section,
}: {
    customerId: string
    section?: string
}) {
    const state = useCustomerDetailState(customerId, section)
    const { query, customer, activeSection, handleSectionChange } = state
    const [assignmentOpen, setAssignmentOpen] = React.useState(false)

    if (query.isPending && !customer) {
        return (
            <DetailWorkspaceSkeleton
                title="客户详情"
                description="正在加载客户…"
                summary
            />
        )
    }

    if (query.isError && !customer) {
        return (
            <DetailWorkspacePage>
                <DetailWorkspaceChrome
                    title="客户详情"
                    back={backToCustomers}
                />
                <DetailWorkspaceBody>
                    <DetailWorkspaceSection width="form">
                        <BusinessFailureState
                            title="客户资料加载失败"
                            error={query.error}
                            action={
                                <Button
                                    id="customers-detail-retry"
                                    type="button"
                                    onClick={() => void query.refetch()}
                                >
                                    重试
                                </Button>
                            }
                        />
                    </DetailWorkspaceSection>
                </DetailWorkspaceBody>
            </DetailWorkspacePage>
        )
    }

    if (!customer) {
        return (
            <DetailWorkspacePage>
                <DetailWorkspaceChrome
                    title="客户不存在或无权访问"
                    back={backToCustomers}
                />
                <DetailWorkspaceBody>
                    <DetailWorkspaceSection width="form">
                        <BusinessEmptyState
                            kind="no-data"
                            title="未找到该客户"
                            description="可能编号有误，或当前账号无权查看该客户。"
                            action={
                                <Button
                                    id="customers-detail-not-found-back"
                                    size="sm"
                                    render={<Link href="/sales/customers" />}
                                >
                                    返回客户列表
                                </Button>
                            }
                        />
                    </DetailWorkspaceSection>
                </DetailWorkspaceBody>
            </DetailWorkspacePage>
        )
    }

    const isDisabled = customer.status === "disabled"
    const uploadContractHref = `/sales/contracts?customerId=${encodeURIComponent(customer.customerId)}&upload=1`
    const createSalesOrderHref = `/sales/orders?mode=create&customerId=${encodeURIComponent(customer.customerId)}`

    const contractBlocked = !can(customer, "UPLOAD_CONTRACT_PDF")
    const salesBlocked = !can(customer, "CREATE_SALES_ORDER")

    const pendingDescription = (() => {
        if (!state.pendingNavigation) return ""
        if (state.pendingNavigation.kind === "back") {
            return "编辑内容尚未保存，返回客户列表后将丢失。"
        }
        if (state.pendingNavigation.kind === "href") {
            return `编辑内容尚未保存，离开前往「${state.pendingNavigation.label}」后将丢失。`
        }
        return "编辑内容尚未保存，切换标签页后将丢失。可先保存修改再切换。"
    })()

    const pendingConfirmLabel = (() => {
        if (!state.pendingNavigation) return "放弃修改"
        if (state.pendingNavigation.kind === "back") return "放弃并返回"
        if (state.pendingNavigation.kind === "href") return "放弃并前往"
        return "放弃并切换"
    })()

    return (
        <DetailWorkspacePage>
            <DetailWorkspaceChrome
                back={{
                    id: "customers-detail-back",
                    label: "客户列表",
                    onClick: state.handleBack,
                }}
                numberLabel="客户编号"
                title={customer.currentRevision.legalName}
                documentNumber={customer.customerNo}
                version={`v${customer.currentRevision.revisionNo}`}
                primaryStatus={customer.statusLabel}
                meta={<CustomerDetailIdentityMeta customer={customer} />}
                titleExtra={
                    state.formDirty ? <DetailWorkspaceDirtyBadge /> : null
                }
                secondaryActions={
                    <GuardedBusinessAction
                        id="customers-detail-create-sales-order"
                        size="sm"
                        variant="ghost"
                        className={detailWorkspaceStyles.quietButton}
                        disabled={salesBlocked}
                        reason={blocker(customer, "CREATE_SALES_ORDER")}
                        render={
                            salesBlocked ? undefined : (
                                <Link
                                    href={createSalesOrderHref}
                                    onClick={(event) => {
                                        if (!state.formDirty) return
                                        event.preventDefault()
                                        state.handleNavigateHref(
                                            createSalesOrderHref,
                                            "新建销售单",
                                        )
                                    }}
                                />
                            )
                        }
                    >
                        <ShoppingCartIcon
                            data-icon="inline-start"
                            aria-hidden="true"
                        />
                        新建销售单
                    </GuardedBusinessAction>
                }
                primaryAction={
                    <GuardedBusinessAction
                        id="customers-detail-upload-contract"
                        size="sm"
                        variant={state.formDirty ? "ghost" : "default"}
                        className={
                            state.formDirty
                                ? detailWorkspaceStyles.quietButton
                                : undefined
                        }
                        disabled={contractBlocked}
                        reason={blocker(customer, "UPLOAD_CONTRACT_PDF")}
                        render={
                            contractBlocked ? undefined : (
                                <Link
                                    href={uploadContractHref}
                                    onClick={(event) => {
                                        if (!state.formDirty) return
                                        event.preventDefault()
                                        state.handleNavigateHref(
                                            uploadContractHref,
                                            "上传合同 PDF",
                                        )
                                    }}
                                />
                            )
                        }
                    >
                        <FilePlus2Icon
                            data-icon="inline-start"
                            aria-hidden="true"
                        />
                        上传合同 PDF
                    </GuardedBusinessAction>
                }
                tabs={
                    <DetailWorkspaceTabs
                        value={activeSection}
                        onValueChange={(next) =>
                            handleSectionChange(next as CustomerSectionId)
                        }
                        items={SECTION_NAV}
                        ariaLabel="客户详情分区"
                        tabIdPrefix="customers-detail-tab"
                    />
                }
            />

            <DetailWorkspaceBody
                summary={<CustomerDetailMetrics customer={customer} />}
                alerts={
                    <>
                        {state.formDirty && query.isError ? (
                            <Alert variant="warning">
                                <AlertTitle>最新客户资料暂时不可用</AlertTitle>
                                <AlertDescription className="flex flex-wrap items-center gap-2">
                                    <span>
                                        当前填写内容已保留。客户资料可能已被其他人更新，保存前建议刷新核对最新资料。
                                    </span>
                                    <Button
                                        id="customers-detail-edit-retry"
                                        type="button"
                                        variant="outline"
                                        size="sm"
                                        onClick={() => void query.refetch()}
                                    >
                                        重试读取
                                    </Button>
                                </AlertDescription>
                            </Alert>
                        ) : null}

                        {isDisabled ? (
                            <Alert variant="warning">
                                <AlertTitle>客户已停用</AlertTitle>
                                <AlertDescription>
                                    该客户已停用，不可新签合同或新建销售单；历史业务单据及往来账款仍可正常查看。
                                    {blocker(customer, "UPLOAD_CONTRACT_PDF") ||
                                    blocker(customer, "CREATE_SALES_ORDER")
                                        ? `（${[
                                              blocker(
                                                  customer,
                                                  "UPLOAD_CONTRACT_PDF",
                                              ),
                                              blocker(
                                                  customer,
                                                  "CREATE_SALES_ORDER",
                                              ),
                                          ]
                                              .filter(Boolean)
                                              .join("；")}）`
                                        : ""}
                                </AlertDescription>
                            </Alert>
                        ) : null}
                    </>
                }
                saveDockOpen={activeSection === "overview" && state.formDirty}
            >
                <div
                    role="tabpanel"
                    aria-labelledby={`customers-detail-tab-${activeSection}`}
                    className="min-w-0"
                >
                    {activeSection === "overview" ? (
                        <CustomerDetailOverviewTab
                            customer={customer}
                            refetch={() => void query.refetch()}
                            formResetEpoch={state.formResetEpoch}
                            onFormDirtyChange={state.setFormDirty}
                            shortcutBlocked={
                                state.pendingNavigation != null ||
                                assignmentOpen
                            }
                            onFormReset={state.handleFormReset}
                            onFormSucceeded={(_customerId, revisionNo) =>
                                state.completeEditing(revisionNo)
                            }
                        />
                    ) : activeSection === "related" ? (
                        <CustomerDetailRelatedTab
                            customer={customer}
                            refetch={() => void query.refetch()}
                        />
                    ) : activeSection === "settlement" ? (
                        <CustomerDetailSettlementTab
                            customer={customer}
                            refetch={() => void query.refetch()}
                        />
                    ) : activeSection === "quality" ? (
                        <CustomerDetailQualityTab
                            customer={customer}
                            refetch={() => void query.refetch()}
                        />
                    ) : (
                        <CustomerDetailAuditTab
                            customer={customer}
                            refetch={() => void query.refetch()}
                            onManageAssignments={() => setAssignmentOpen(true)}
                        />
                    )}
                </div>
            </DetailWorkspaceBody>

            <CustomerAssignmentDialog
                customerId={customer.customerId}
                assignments={customer.assignments}
                open={assignmentOpen}
                onOpenChange={setAssignmentOpen}
            />

            <DiscardConfirmDialog
                id="customers-detail-pending-section-discard"
                open={state.pendingNavigation != null}
                onOpenChange={(open) => {
                    if (!open) state.dismissPendingNavigation()
                }}
                title="放弃未保存的修改？"
                description={pendingDescription}
                confirmLabel={pendingConfirmLabel}
                cancelLabel="继续编辑"
                onConfirm={state.discardPendingAndNavigate}
            />
        </DetailWorkspacePage>
    )
}
