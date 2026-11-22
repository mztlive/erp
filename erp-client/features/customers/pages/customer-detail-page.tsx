"use client"

import * as React from "react"
import Link from "next/link"
import {
    FilePlus2Icon,
    PencilIcon,
    ShoppingCartIcon,
    TriangleAlertIcon,
} from "lucide-react"

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
import { useAccountProfileQuery } from "@/features/auth/queries"
import { CustomerAssignmentDialog } from "@/features/customers/components/customer-assignment-dialog"
import { useCustomerDetailState } from "@/features/customers/hooks/use-customer-detail-state"
import {
    CustomerDetailRelatedTab,
    CustomerDetailSettlementTab,
} from "@/features/customers/pages/customer-detail-business-tabs"
import { CustomerDetailAuditTab } from "@/features/customers/pages/customer-detail-governance-tabs"
import {
    SECTION_NAV,
    blocker,
    can,
    customerMonogram,
    type CustomerActiveSectionId,
} from "@/features/customers/pages/customer-detail-helpers"
import {
    CustomerDetailIdentityMeta,
    CustomerDetailMetrics,
    CustomerDetailOverviewTab,
} from "@/features/customers/pages/customer-detail-overview"
import { CustomerDetailSidebar } from "@/features/customers/pages/customer-detail-sidebar"
import { hasPermission } from "@/lib/permissions"
import { cn } from "@/lib/utils"

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
    const { query, customer, activeSection, editing, handleSectionChange } =
        state
    const accountProfile = useAccountProfileQuery()
    const [assignmentOpen, setAssignmentOpen] = React.useState(false)

    if (query.isPending && !customer) {
        return (
            <DetailWorkspaceSkeleton
                title="客户详情"
                description="正在加载客户…"
                summary
                sidebar
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

    const canEdit =
        can(customer, "EDIT_CUSTOMER") &&
        hasPermission(accountProfile.data?.permissions, "customer:update")
    const contractBlocker = blocker(customer, "UPLOAD_CONTRACT_PDF")
    const salesBlocker = blocker(customer, "CREATE_SALES_ORDER")
    const showContractAction =
        can(customer, "UPLOAD_CONTRACT_PDF") || Boolean(contractBlocker)
    const showSalesAction =
        can(customer, "CREATE_SALES_ORDER") || Boolean(salesBlocker)
    const contractBlocked = !can(customer, "UPLOAD_CONTRACT_PDF")
    const salesBlocked = !can(customer, "CREATE_SALES_ORDER")

    const disabledBlockerNotes = Array.from(
        new Set([contractBlocker, salesBlocker].filter(Boolean)),
    )

    const isOverviewReadMode = activeSection === "overview" && !editing
    const relatedTotal =
        customer.metrics.activeContractCount != null &&
        customer.metrics.inProgressSalesOrderCount != null
            ? customer.metrics.activeContractCount +
              customer.metrics.inProgressSalesOrderCount
            : undefined

    const tabItems = SECTION_NAV.map((item) => {
        if (item.id === "related" && relatedTotal != null && relatedTotal > 0) {
            return {
                ...item,
                badge: (
                    <span className="num rounded-full bg-muted px-1.5 py-0 text-tiny font-medium text-muted-foreground">
                        {relatedTotal}
                    </span>
                ),
            }
        }
        if (item.id === "audit" && customer.revisionTimeline.length > 0) {
            return {
                ...item,
                badge: (
                    <span className="num rounded-full bg-muted px-1.5 py-0 text-tiny font-medium text-muted-foreground">
                        {customer.revisionTimeline.length}
                    </span>
                ),
            }
        }
        return item
    })

    const pendingDescription = (() => {
        if (!state.pendingNavigation) return ""
        if (state.pendingNavigation.kind === "cancel-edit") {
            return "当前修改尚未保存，退出编辑后将还原为最近一次保存的资料。"
        }
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
        if (state.pendingNavigation.kind === "cancel-edit") return "放弃并退出"
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
                media={
                    <div
                        aria-hidden="true"
                        className={cn(
                            "flex size-11 shrink-0 items-center justify-center rounded-lg border text-section-title font-semibold",
                            isDisabled
                                ? "border-neutral-border bg-neutral-soft text-neutral-soft-foreground"
                                : "border-info-border/70 bg-info-soft text-info-soft-foreground",
                        )}
                    >
                        {customerMonogram(customer)}
                    </div>
                }
                numberLabel="客户编号"
                title={customer.currentRevision.legalName}
                documentNumber={customer.customerNo}
                version={`v${customer.currentRevision.revisionNo}`}
                primaryStatus={customer.statusLabel}
                meta={<CustomerDetailIdentityMeta customer={customer} />}
                titleExtra={
                    editing && state.formDirty ? (
                        <DetailWorkspaceDirtyBadge />
                    ) : null
                }
                secondaryActions={
                    <>
                        {canEdit ? (
                            editing ? (
                                <Button
                                    id="customers-detail-edit-cancel"
                                    type="button"
                                    size="sm"
                                    variant="ghost"
                                    className={
                                        detailWorkspaceStyles.quietButton
                                    }
                                    onClick={state.requestCancelEditing}
                                >
                                    退出编辑
                                </Button>
                            ) : (
                                <Button
                                    id="customers-detail-overview-edit"
                                    type="button"
                                    size="sm"
                                    variant="ghost"
                                    className={
                                        detailWorkspaceStyles.quietButton
                                    }
                                    onClick={state.startEditing}
                                >
                                    <PencilIcon
                                        data-icon="inline-start"
                                        aria-hidden="true"
                                    />
                                    编辑资料
                                </Button>
                            )
                        ) : null}
                        {showContractAction ? (
                            <GuardedBusinessAction
                                id="customers-detail-upload-contract"
                                size="sm"
                                variant="ghost"
                                className={detailWorkspaceStyles.quietButton}
                                disabled={contractBlocked}
                                reason={contractBlocker}
                                render={
                                    contractBlocked ? undefined : (
                                        <Link
                                            href={uploadContractHref}
                                            onClick={(event) => {
                                                if (
                                                    !editing ||
                                                    !state.formDirty
                                                )
                                                    return
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
                        ) : null}
                    </>
                }
                primaryAction={
                    showSalesAction ? (
                        <GuardedBusinessAction
                            id="customers-detail-create-sales-order"
                            size="sm"
                            variant={editing ? "ghost" : "default"}
                            className={
                                editing
                                    ? detailWorkspaceStyles.quietButton
                                    : undefined
                            }
                            disabled={salesBlocked}
                            reason={salesBlocker}
                            render={
                                salesBlocked ? undefined : (
                                    <Link
                                        href={createSalesOrderHref}
                                        onClick={(event) => {
                                            if (!editing || !state.formDirty)
                                                return
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
                    ) : undefined
                }
                tabs={
                    <DetailWorkspaceTabs
                        value={activeSection}
                        onValueChange={(next) =>
                            handleSectionChange(next as CustomerActiveSectionId)
                        }
                        items={tabItems}
                        ariaLabel="客户详情分区"
                        tabIdPrefix="customers-detail-tab"
                    />
                }
            />

            <DetailWorkspaceBody
                summary={
                    isOverviewReadMode ? (
                        <CustomerDetailMetrics
                            customer={customer}
                            onSelectSection={handleSectionChange}
                        />
                    ) : undefined
                }
                sidebar={
                    isOverviewReadMode ? (
                        <CustomerDetailSidebar
                            customer={customer}
                            refetch={() => void query.refetch()}
                            onSelectSection={handleSectionChange}
                            onManageAssignments={() => setAssignmentOpen(true)}
                        />
                    ) : undefined
                }
                sidebarLabel={
                    isOverviewReadMode ? "客户归属与业务速览" : undefined
                }
                alerts={
                    <>
                        {editing && state.formDirty && query.isError ? (
                            <Alert variant="warning">
                                <TriangleAlertIcon aria-hidden="true" />
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
                                <TriangleAlertIcon aria-hidden="true" />
                                <AlertTitle>客户已停用</AlertTitle>
                                <AlertDescription>
                                    该客户已停用，不可新签合同或新建销售单；历史业务单据及往来账款仍可正常查看。
                                    {disabledBlockerNotes.length > 0
                                        ? `（${disabledBlockerNotes.join("；")}）`
                                        : ""}
                                </AlertDescription>
                            </Alert>
                        ) : null}
                    </>
                }
                saveDockOpen={
                    editing && activeSection === "overview" && state.formDirty
                }
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
                            editing={editing}
                            onEditClick={state.startEditing}
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
