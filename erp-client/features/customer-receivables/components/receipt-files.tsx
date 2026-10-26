"use client"

import { BusinessFailureState } from "@/components/business"
import { FinancialFileList } from "@/features/file-assets/components/financial-file-list"
import { useCustomerReceiptFiles } from "@/features/file-assets/hooks/business-finance-files"

/** 原登记人编辑时按提交资格读取，详情按整单读取资格访问。 */
export function ReceiptFiles({
    receiptId,
    draft = false,
}: {
    receiptId: string
    draft?: boolean
}) {
    const { query, download } = useCustomerReceiptFiles(receiptId, draft)
    if (query.isPending)
        return (
            <p role="status" className="text-sm text-muted-foreground">
                正在读取回款凭证…
            </p>
        )
    if (query.isError)
        return (
            <BusinessFailureState
                title="回款凭证读取失败"
                error={query.error}
                onRetry={() => void query.refetch()}
            />
        )
    if (!query.data.length)
        return <p className="text-sm text-muted-foreground">未上传回款凭证</p>
    return (
        <FinancialFileList
            files={query.data}
            idPrefix={draft ? "receipt-draft-file" : "receipt-detail-file"}
            showDocumentNumber={false}
            onDownload={(file) => download.mutate(file)}
            pendingAssetId={
                download.isPending
                    ? download.variables?.file_asset_id
                    : undefined
            }
        />
    )
}
