"use client"
import { BusinessFailureState } from "@/components/business"
import { FinancialFileList } from "@/features/file-assets/components/financial-file-list"
import { usePurchasePaymentReceipts } from "@/features/file-assets/hooks/business-finance-files"

/** 正式采购履约任务的付款回单，下载时由服务端再次核对任务与采购来源。 */
export function PurchasePaymentReceipts({
    workItemId,
    embedded = false,
}: {
    workItemId: string
    embedded?: boolean
}) {
    const { query, download } = usePurchasePaymentReceipts(workItemId)
    const content = query.isPending ? (
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
    )
    return (
        <section
            aria-label="财务付款回单"
            className={
                embedded ? undefined : "rounded-xl border border-border/70 p-4"
            }
        >
            <div className="flex items-baseline gap-2">
                <h3 className="text-sm font-semibold">财务付款回单</h3>
                {query.isSuccess && query.data.length > 0 ? (
                    <span className="text-xs text-muted-foreground">
                        {query.data.length} 份回单
                    </span>
                ) : null}
            </div>
            {content}
        </section>
    )
}
