"use client"

import * as React from "react"
import Link from "next/link"
import { FilePlus2Icon, ShoppingCartIcon } from "lucide-react"

import {
    BusinessFailureState,
    DiscardConfirmDialog,
    DocumentHeader,
    GuardedBusinessAction,
    PageHeader,
    PageScaffold,
    surfacePanelClassName,
} from "@/components/business"
import { cn } from "@/lib/utils"
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
    const {
        query,
        customer,
        activeSection,
        editing,
        savedNotice,
        handleSectionChange,
    } = state
    const [assignmentDialog, setAssignmentDialog] = React.useState<{
        target?: React.ComponentProps<typeof CustomerAssignmentDialog>["target"]
    } | null>(null)

    if (query.isPending) {
        return (
            <PageScaffold>
                <PageHeader title="客户详情" description="正在加载客户…" />
                <div className="space-y-3" aria-busy="true" aria-label="加载中">
                    <div className="h-16 animate-pulse rounded-lg bg-muted" />
                    <div className="h-20 animate-pulse rounded-lg bg-muted" />
                    <div className="h-40 animate-pulse rounded-lg bg-muted" />
                </div>
            </PageScaffold>
        )
    }

    if (query.isError) {
        return (
            <PageScaffold>
                <PageHeader title="客户详情" />
                <BusinessFailureState
                    error={query.error}
                    action={
                        <Button
                            type="button"
                            onClick={() => void query.refetch()}
                        >
                            重试
                        </Button>
                    }
                />
            </PageScaffold>
        )
    }

    if (!customer) {
        return (
            <PageScaffold>
                <PageHeader
                    title="客户不存在或无权访问"
                    description="未找到该客户。可能编号有误，或当前角色无权访问该客户。"
                    actions={
                        <Button render={<Link href="/sales/customers" />}>
                            返回客户选择
                        </Button>
                    }
                />
            </PageScaffold>
        )
    }

    const isDisabled = customer.status === "disabled"
    const uploadContractHref = `/sales/contracts?customerId=${encodeURIComponent(customer.customerId)}`
    const createSalesOrderHref = `/sales/orders?mode=create&customerId=${encodeURIComponent(customer.customerId)}`

    const contractBlocked = !can(customer, "UPLOAD_CONTRACT_PDF")
    const salesBlocked = !can(customer, "CREATE_SALES_ORDER")

    return (
        <PageScaffold>
            {/* First screen: identity + owner + metrics + primary actions */}
            <DocumentHeader
                density="compact"
                title={customer.currentRevision.legalName}
                documentNumber={customer.customerNo}
                version={`v${customer.currentRevision.revisionNo}`}
                primaryStatus={customer.statusLabel}
                meta={<CustomerDetailIdentityMeta customer={customer} />}
                primaryAction={
                    <div className="flex flex-wrap items-center gap-2">
                        <GuardedBusinessAction
                            size="sm"
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
                        <GuardedBusinessAction
                            size="sm"
                            variant="secondary"
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
                    </div>
                }
            />

            {isDisabled ? (
                <Alert variant="warning">
                    <AlertTitle>客户已停用</AlertTitle>
                    <AlertDescription>
                        稳定身份、历史修订与已引用单据保留。上传合同和新建销售单已禁用
                        {blocker(customer, "UPLOAD_CONTRACT_PDF") ||
                        blocker(customer, "CREATE_SALES_ORDER")
                            ? `（${[
                                  blocker(customer, "UPLOAD_CONTRACT_PDF"),
                                  blocker(customer, "CREATE_SALES_ORDER"),
                              ]
                                  .filter(Boolean)
                                  .join("；")}）`
                            : ""}
                        ；可继续查看历史与票款摘要。
                    </AlertDescription>
                </Alert>
            ) : null}

            <CustomerDetailMetrics customer={customer} />

            <div
                className={cn(surfacePanelClassName, "min-w-0 overflow-hidden")}
            >
                <Tabs
                    value={activeSection}
                    onValueChange={(next) => {
                        handleSectionChange(
                            (next as CustomerSectionId) ?? "overview",
                        )
                    }}
                >
                    <TabsList
                        variant="line"
                        className="sticky top-0 z-10 w-full justify-start overflow-x-auto rounded-none border-b border-grid bg-card/95 px-3 backdrop-blur supports-backdrop-filter:bg-card/80"
                    >
                        {SECTION_NAV.map((item) => (
                            <TabsTrigger
                                key={item.id}
                                value={item.id}
                                className="flex-none"
                            >
                                {item.label}
                            </TabsTrigger>
                        ))}
                    </TabsList>

                    <TabsContent
                        value="overview"
                        className="px-3 pb-3 md:px-4 md:pb-4"
                    >
                        <CustomerDetailOverviewTab
                            customer={customer}
                            refetch={() => void query.refetch()}
                            editing={editing}
                            savedNotice={savedNotice}
                            onEditClick={state.startEditing}
                            onFormDirtyChange={state.setFormDirty}
                            onFormCancel={state.cancelEditing}
                            onFormSucceeded={(_customerId, revisionNo) =>
                                state.completeEditing(revisionNo)
                            }
                            onDismissSavedNotice={state.dismissSavedNotice}
                        />
                    </TabsContent>

                    <TabsContent
                        value="related"
                        className="px-3 pb-3 md:px-4 md:pb-4"
                    >
                        <CustomerDetailRelatedTab
                            customer={customer}
                            refetch={() => void query.refetch()}
                        />
                    </TabsContent>

                    <TabsContent
                        value="settlement"
                        className="px-3 pb-3 md:px-4 md:pb-4"
                    >
                        <CustomerDetailSettlementTab
                            customer={customer}
                            refetch={() => void query.refetch()}
                        />
                    </TabsContent>

                    <TabsContent
                        value="quality"
                        className="px-3 pb-3 md:px-4 md:pb-4"
                    >
                        <CustomerDetailQualityTab
                            customer={customer}
                            refetch={() => void query.refetch()}
                        />
                    </TabsContent>

                    <TabsContent
                        value="audit"
                        className="px-3 pb-3 md:px-4 md:pb-4"
                    >
                        <CustomerDetailAuditTab
                            customer={customer}
                            refetch={() => void query.refetch()}
                            onManageAssignments={() => setAssignmentDialog({})}
                            onEndCollaboration={(target) =>
                                setAssignmentDialog({ target })
                            }
                        />
                    </TabsContent>
                </Tabs>
            </div>

            <CustomerAssignmentDialog
                customerId={customer.customerId}
                open={assignmentDialog != null}
                target={assignmentDialog?.target}
                onOpenChange={(open) => {
                    if (!open) setAssignmentDialog(null)
                }}
            />

            <DiscardConfirmDialog
                open={state.pendingSection != null}
                onOpenChange={(open) => {
                    if (!open) state.dismissPendingSection()
                }}
                title="放弃未保存的修改？"
                description="编辑内容尚未保存，切换分区后将丢失。可先保存修订再切换。"
                confirmLabel="放弃并切换"
                cancelLabel="继续编辑"
                onConfirm={state.discardPendingAndSwitch}
            />
        </PageScaffold>
    )
}
