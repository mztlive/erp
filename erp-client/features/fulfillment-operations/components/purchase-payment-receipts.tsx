"use client"
import { BusinessFailureState } from "@/components/business"
import { FinancialFileList } from "@/features/file-assets/components/financial-file-list"
import { usePurchasePaymentReceipts } from "@/features/file-assets/hooks/business-finance-files"

/** 正式采购履约任务的付款回单，下载时由服务端再次核对任务与采购来源。 */
export function PurchasePaymentReceipts({
    workItemId,
}: {
    workItemId: string
}) {
    const { query, download } = usePurchasePaymentReceipts(workItemId)
    return (
        <section className="rounded-xl border border-border/70 p-4">
            <h3 className="text-sm font-medium">财务付款回单</h3>
            {query.isPending ? (
                <p role="status" className="mt-2 text-sm text-muted-foreground">
                    正在加载付款回单…
                </p>
            ) : query.isError ? (
                <BusinessFailureState
                    title="付款回单加载失败"
                    error={query.error}
                    onRetry={() => void query.refetch()}
                />
            ) : query.data.length ? (
                <FinancialFileList
                    files={query.data}
                    idPrefix={`purchase-payment-${workItemId}`}
                    onDownload={(file) => download.mutate(file)}
                    pendingAssetId={
                        download.isPending
                            ? download.variables?.file_asset_id
                            : undefined
                    }
                />
            ) : (
                <p className="mt-2 text-sm text-muted-foreground">
                    当前采购单暂无已上传的付款回单
                </p>
            )}
        </section>
    )
}
