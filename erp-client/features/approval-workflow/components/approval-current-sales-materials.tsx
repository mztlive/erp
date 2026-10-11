"use client"

import { BusinessFailureState } from "@/components/business"
import { useApprovalMaterials } from "../hooks/use-approval-materials"
import { ApprovalSalesOrderPaper } from "./approval-sales-order-paper"
import { ApprovalMaterialFiles } from "./approval-material-files"

/** 当前销售来源独立预览，绝不替换申请提交时保存的资料。 */
export function ApprovalCurrentSalesMaterials({
    instanceId,
    revisionId,
    enabled,
}: {
    instanceId: string
    revisionId?: string
    enabled: boolean
}) {
    const query = useApprovalMaterials(instanceId, enabled)
    const basis = query.data?.current_sales_basis
    return (
        <article className="space-y-6 rounded-lg bg-card p-6 shadow-lg sm:p-10">
            <header className="space-y-2 border-b pb-5 pr-8">
                <h2 className="text-xl font-semibold">销售单预览</h2>
                <p className="text-sm text-muted-foreground">
                    当前生效销售版本及关联合同、凭证
                </p>
            </header>
            {query.isPending || query.isFetching ? (
                <p role="status">正在读取销售资料…</p>
            ) : query.isError ? (
                <BusinessFailureState
                    id="approval-current-sales-retry"
                    error={query.error}
                    onRetry={() => void query.refetch()}
                />
            ) : !basis || basis.source.revision_id !== revisionId ? (
                <p role="alert">
                    销售来源已变化或暂不可读取，请关闭预览并刷新工作台。
                </p>
            ) : (
                <>
                    <ApprovalSalesOrderPaper
                        submission={basis.sales_order.revision}
                        documentNo={basis.sales_order.document_no}
                        submitterName={null}
                    />
                    <section className="space-y-3 border-t pt-5">
                        <h3 className="font-medium">
                            销售版本 V{basis.source.revision_no} 的合同与凭证
                        </h3>
                        <ApprovalMaterialFiles
                            instanceId={instanceId}
                            sourceRevisionId={basis.source.revision_id}
                            attachments={basis.attachments}
                            idPrefix="approval-current-sales-material"
                            emptyMessage={
                                basis.unavailable_reason ??
                                "此销售版本未关联可读取的合同或凭证，请联系销售核对归档资料。"
                            }
                        />
                    </section>
                </>
            )}
        </article>
    )
}
