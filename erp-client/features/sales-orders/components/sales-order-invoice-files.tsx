"use client"
import { BusinessFailureState } from "@/components/business"
import { DetailRecordSection } from "@/components/business/detail-presentation"
import { FinancialFileList } from "@/features/file-assets/components/financial-file-list"
import { useSalesInvoiceFiles } from "@/features/file-assets/hooks/business-finance-files"

/** 本单已登记发票附件按销售详情资格下载，与完整财务明细资格分别读取。 */
export function SalesOrderInvoiceFiles({
    salesOrderId,
}: {
    salesOrderId: string
}) {
    const { query, download } = useSalesInvoiceFiles(salesOrderId)
    return (
        <DetailRecordSection title="发票文件" count={query.data?.length}>
            {query.isPending ? (
                <p role="status" className="text-sm text-muted-foreground">
                    正在加载发票文件…
                </p>
            ) : query.isError ? (
                <BusinessFailureState
                    title="发票文件加载失败"
                    error={query.error}
                    onRetry={() => void query.refetch()}
                />
            ) : query.data.length ? (
                <FinancialFileList
                    files={query.data}
                    idPrefix="sales-order-invoice-file"
                    onDownload={(file) => download.mutate(file)}
                    pendingAssetId={
                        download.isPending
                            ? download.variables?.file_asset_id
                            : undefined
                    }
                />
            ) : (
                <p className="text-sm text-muted-foreground">
                    财务尚未上传本单发票文件
                </p>
            )}
        </DetailRecordSection>
    )
}
