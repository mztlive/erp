"use client"

import Link from "next/link"

import {
    BusinessFailureState,
    PageHeader,
    PageScaffold,
} from "@/components/business"
import { Button } from "@/components/ui/button"
import { SalesOrderCreateForm } from "@/features/sales-orders/components/sales-order-create-form"
import { useSalesOrderDraftResumeQuery } from "@/features/sales-orders/hooks/queries"
import type { SalesOrderNature } from "@/features/sales-orders/types"

/**
 * 继续编辑草稿时，先取回草稿内容再挂载表单——避免表单先以空 defaultValues
 * 挂载、草稿到货后再 reset 的时序复杂度（TanStack Form 的 defaultValues 只在
 * 挂载时生效一次）。
 */
export function SalesOrderCreatePage({
    initialCustomerId = "",
    initialContractId = "",
    initialContractRevisionId = "",
    initialNature = "physical_service",
    initialSalesOrderId = "",
}: {
    initialCustomerId?: string
    initialContractId?: string
    initialContractRevisionId?: string
    initialNature?: SalesOrderNature
    /** 从草稿详情页"继续编辑"进入时携带；为空则是全新建单。 */
    initialSalesOrderId?: string
}) {
    const resumeQuery = useSalesOrderDraftResumeQuery(initialSalesOrderId)

    if (initialSalesOrderId) {
        if (resumeQuery.isPending) {
            return (
                <PageScaffold>
                    <PageHeader
                        title="继续编辑草稿"
                        description="正在加载已保存的内容…"
                    />
                    <div
                        className="space-y-3"
                        aria-busy="true"
                        aria-label="加载中"
                    >
                        <div className="h-16 animate-pulse rounded-lg bg-muted" />
                        <div className="h-40 animate-pulse rounded-lg bg-muted" />
                    </div>
                </PageScaffold>
            )
        }
        if (resumeQuery.isError) {
            return (
                <PageScaffold>
                    <PageHeader title="草稿加载失败" />
                    <BusinessFailureState
                        error={resumeQuery.error}
                        onRetry={() => void resumeQuery.refetch()}
                        retryLabel="重新加载"
                        details={
                            <Button
                                type="button"
                                variant="outline"
                                render={
                                    <Link
                                        href={`/sales/orders/${initialSalesOrderId}`}
                                    />
                                }
                            >
                                返回销售单详情
                            </Button>
                        }
                    />
                </PageScaffold>
            )
        }
        if (!resumeQuery.data) {
            return (
                <PageScaffold>
                    <PageHeader
                        title="草稿加载失败"
                        description="这张草稿可能已被提交、作废，或暂时无法访问。"
                        actions={
                            <Button
                                type="button"
                                variant="outline"
                                render={
                                    <Link
                                        href={`/sales/orders/${initialSalesOrderId}`}
                                    />
                                }
                            >
                                返回销售单详情
                            </Button>
                        }
                    />
                </PageScaffold>
            )
        }
        return (
            <SalesOrderCreateForm
                purpose="draft"
                initialCustomerId={initialCustomerId}
                initialContractId={initialContractId}
                initialContractRevisionId={initialContractRevisionId}
                initialNature={initialNature}
                initialDraft={resumeQuery.data}
            />
        )
    }

    return (
        <SalesOrderCreateForm
            initialCustomerId={initialCustomerId}
            initialContractId={initialContractId}
            initialContractRevisionId={initialContractRevisionId}
            initialNature={initialNature}
        />
    )
}
