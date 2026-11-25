"use client"

import { BusinessFailureState } from "@/components/business"
import { Spinner } from "@/components/ui/spinner"
import { cn } from "@/lib/utils"
import { useApprovalMaterials } from "../hooks/use-approval-materials"
import { ApprovalSalesOrderPaper } from "./approval-sales-order-paper"
import { ApprovalMaterialFiles } from "./approval-material-files"

/** 当前销售来源独立预览，绝不替换申请提交时保存的资料。 */
export function ApprovalCurrentSalesMaterials({
    instanceId,
    revisionId,
    enabled,
    bare = false,
}: {
    instanceId: string
    revisionId?: string
    enabled: boolean
    bare?: boolean
}) {
    const query = useApprovalMaterials(instanceId, enabled)
    const basis = query.data?.current_sales_basis
    return (
        <article
            className={cn(bare ? "space-y-4" : "space-y-6 bg-card p-6 sm:p-8")}
        >
            {!bare ? (
                <header className="space-y-2 border-b border-border/60 pb-5">
                    <h2 className="text-section-title font-semibold tracking-tight">
                        销售单预览
                    </h2>
                    <p className="text-xs text-muted-foreground">
                        当前生效销售版本及关联合同、凭证
                    </p>
                </header>
            ) : null}
            {query.isPending || (!query.data && query.isFetching) ? (
                <div className="flex min-h-48 items-center justify-center py-8">
                    <p
                        role="status"
                        className="flex items-center gap-2 text-xs text-muted-foreground"
                    >
                        <Spinner aria-label="正在读取销售资料" />
                        正在读取销售资料…
                    </p>
                </div>
            ) : query.isError ? (
                <BusinessFailureState
                    id="approval-current-sales-retry"
                    error={query.error}
                    onRetry={() => void query.refetch()}
                />
            ) : !basis || basis.source.revision_id !== revisionId ? (
                <p
                    role="alert"
                    className="py-6 text-center text-body-compact text-muted-foreground"
                >
                    销售来源已变化或暂不可读取，请关闭预览并刷新工作台。
                </p>
            ) : (
                <>
                    <ApprovalSalesOrderPaper
                        compact={bare}
                        submission={basis.sales_order.revision}
                        documentNo={basis.sales_order.document_no}
                        submitterName={null}
                    />
                    <section className="space-y-2.5 border-t border-border/60 pt-4">
                        <h3 className="text-body-compact font-semibold text-foreground">
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
