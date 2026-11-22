"use client"

import * as React from "react"
import Link from "next/link"
import { FilePlus2Icon, ShoppingCartIcon } from "lucide-react"

import {
    BusinessFailureState,
    DiscardConfirmDialog,
    DetailWorkspaceBody,
    DetailWorkspaceChrome,
    DetailWorkspaceDirtyBadge,
    DetailWorkspacePage,
    DetailWorkspaceSkeleton,
    detailWorkspaceStyles,
    GuardedBusinessAction,
} from "@/components/business"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Button } from "@/components/ui/button"
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs"
import { CustomerAssignmentDialog } from "@/features/customers/components/customer-assignment-dialog"
import type { CustomerSectionId } from "@/features/customers/types"
import { useCustomerDetailState } from "@/features/customers/hooks/use-customer-detail-state"
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
import {
    CustomerDetailRelatedTab,
    CustomerDetailSettlementTab,
} from "@/features/customers/pages/customer-detail-business-tabs"
import {
    CustomerDetailAuditTab,
    CustomerDetailQualityTab,
} from "@/features/customers/pages/customer-detail-governance-tabs"

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
    const [assignmentOpen, setAssignmentOpen] = React.useState(false)

    if (query.isPending && !customer) {
        return (
            <div
                className="flex min-w-0 flex-1 flex-col"
                aria-busy="true"
                aria-label="加载中"
            >
                <DetailWorkspaceSkeleton
                    title="客户详情"
                    description="正在加载客户…"
                />
            </div>
        )
    }

    if (query.isError && !customer) {
        return (
            <DetailWorkspacePage>
                <DetailWorkspaceChrome title="客户详情" />
                <DetailWorkspaceBody>
                    <div className="pt-4">
                        <BusinessFailureState
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
                    </div>
                </DetailWorkspaceBody>
            </DetailWorkspacePage>
        )
    }

    if (!customer) {
        return (
            <DetailWorkspacePage>
                <DetailWorkspaceChrome
                    title="客户不存在或无权访问"
                    meta="未找到该客户。可能编号有误，或当前账号无权查看该客户。"
                    secondaryActions={
                        <Button
                            id="customers-detail-not-found-back"
                            render={<Link href="/sales/customers" />}
                        >
                            返回客户选择
                        </Button>
                    }
                />
            </DetailWorkspacePage>
        )
    }

    const isDisabled = customer.status === "disabled"
    const uploadContractHref = `/sales/contracts?customerId=${encodeURIComponent(customer.customerId)}&upload=1`
    const createSalesOrderHref = `/sales/orders?mode=create&customerId=${encodeURIComponent(customer.customerId)}`

    const contractBlocked = !can(customer, "UPLOAD_CONTRACT_PDF")
    const salesBlocked = !can(customer, "CREATE_SALES_ORDER")

    return (
        <DetailWorkspacePage>
            <Tabs
                value={activeSection}
                onValueChange={(next) =>
                    handleSectionChange(
                        (next as CustomerSectionId) ?? "overview",
                    )
                }
                className="min-w-0 flex-1 gap-0"
            >
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
                        editing && state.formDirty ? (
                            <DetailWorkspaceDirtyBadge />
                        ) : null
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
                                    <Link href={createSalesOrderHref} />
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
                            variant={editing ? "ghost" : "default"}
                            className={
                                editing
                                    ? detailWorkspaceStyles.quietButton
                                    : undefined
                            }
                            disabled={contractBlocked}
                            reason={blocker(customer, "UPLOAD_CONTRACT_PDF")}
                            render={
                                contractBlocked ? undefined : (
                                    <Link href={uploadContractHref} />
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
                        <TabsList
                            variant="line"
                            aria-label="客户详情分区"
                            className={detailWorkspaceStyles.tabsList}
                        >
                            {SECTION_NAV.map((item) => (
                                <TabsTrigger
                                    id={`customers-detail-tab-${toAutomationIdSegment(item.id)}`}
                                    key={item.id}
                                    value={item.id}
                                    className={detailWorkspaceStyles.tab}
                                >
                                    {item.label}
                                </TabsTrigger>
                            ))}
                        </TabsList>
                    }
                />

                <DetailWorkspaceBody
                    summary={<CustomerDetailMetrics customer={customer} />}
                    alerts={
                        <>
                            {editing && (query.isError || !query.data) ? (
                                <Alert variant="warning">
                                    <AlertTitle>
                                        最新客户资料暂时不可用
                                    </AlertTitle>
                                    <AlertDescription>
                                        当前填写内容已保留。客户资料可能已被其他人更新，保存前建议刷新核对最新资料。
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
                                        {blocker(
                                            customer,
                                            "UPLOAD_CONTRACT_PDF",
                                        ) ||
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
                    saveDockOpen={
                        activeSection === "overview" &&
                        editing &&
                        state.formDirty
                    }
                >
                    <TabsContent value="overview" className="min-w-0">
                        <CustomerDetailOverviewTab
                            customer={customer}
                            refetch={() => void query.refetch()}
                            editing={editing}
                            onEditClick={state.startEditing}
                            onFormDirtyChange={state.setFormDirty}
                            shortcutBlocked={
                                state.pendingSection != null || assignmentOpen
                            }
                            onFormCancel={state.cancelEditing}
                            onFormSucceeded={(_customerId, revisionNo) =>
                                state.completeEditing(revisionNo)
                            }
                        />
                    </TabsContent>

                    <TabsContent value="related" className="min-w-0">
                        <CustomerDetailRelatedTab
                            customer={customer}
                            refetch={() => void query.refetch()}
                        />
                    </TabsContent>

                    <TabsContent value="settlement" className="min-w-0">
                        <CustomerDetailSettlementTab
                            customer={customer}
                            refetch={() => void query.refetch()}
                        />
                    </TabsContent>

                    <TabsContent value="quality" className="min-w-0">
                        <CustomerDetailQualityTab
                            customer={customer}
                            refetch={() => void query.refetch()}
                        />
                    </TabsContent>

                    <TabsContent value="audit" className="min-w-0">
                        <CustomerDetailAuditTab
                            customer={customer}
                            refetch={() => void query.refetch()}
                            onManageAssignments={() => setAssignmentOpen(true)}
                        />
                    </TabsContent>
                </DetailWorkspaceBody>
            </Tabs>

            <CustomerAssignmentDialog
                customerId={customer.customerId}
                assignments={customer.assignments}
                open={assignmentOpen}
                onOpenChange={setAssignmentOpen}
            />

            <DiscardConfirmDialog
                id="customers-detail-pending-section-discard"
                open={state.pendingSection != null}
                onOpenChange={(open) => {
                    if (!open) state.dismissPendingSection()
                }}
                title="放弃未保存的修改？"
                description={
                    state.pendingSection === "back"
                        ? "编辑内容尚未保存，返回列表后将丢失。"
                        : "编辑内容尚未保存，切换标签页后将丢失。可先保存修改再切换。"
                }
                confirmLabel={
                    state.pendingSection === "back"
                        ? "放弃并返回"
                        : "放弃并切换"
                }
                cancelLabel="继续编辑"
                onConfirm={state.discardPendingAndSwitch}
            />
        </DetailWorkspacePage>
    )
}
