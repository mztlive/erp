"use client"
import { BusinessFailureState } from "@/components/business"
import { FinancialFileList } from "@/features/file-assets/components/financial-file-list"
import { usePurchasePaymentReceipts } from "@/features/file-assets/hooks/business-finance-files"
import { cn } from "@/lib/utils"

/** 正式采购履约任务的付款回单，下载时由服务端再次核对任务与采购来源。 */
export function PurchasePaymentReceipts({
    workItemId,
    embedded = false,
    title = "财务付款回单",
    layout = "section",
}: {
    workItemId: string
    embedded?: boolean
    title?: string
    layout?: "section" | "row"
}) {
    const { query, download } = usePurchasePaymentReceipts(workItemId)
    const messageClass = cn(
        "text-sm text-muted-foreground",
        layout === "section" && "mt-2",
    )
    const content = query.isPending ? (
        <p role="status" className={messageClass}>
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
        <p className={messageClass}>当前采购单暂无已上传的付款回单</p>
    )
    return (
        <section
            aria-label={title}
            className={cn(
                !embedded && "rounded-xl border border-border/70 p-4",
                layout === "row" &&
                    "grid grid-cols-[5rem_minmax(0,1fr)] items-baseline gap-3",
            )}
        >
            <div className="flex flex-wrap items-baseline gap-2">
                <h3
                    className={
                        layout === "row"
                            ? "text-xs font-normal text-muted-foreground"
                            : "text-sm font-semibold"
                    }
                >
                    {title}
                </h3>
                {query.isSuccess && query.data.length > 0 ? (
                    <span className="text-xs text-muted-foreground">
                        {query.data.length} 份回单
                    </span>
                ) : null}
            </div>
            <div className="min-w-0">{content}</div>
        </section>
    )
}
