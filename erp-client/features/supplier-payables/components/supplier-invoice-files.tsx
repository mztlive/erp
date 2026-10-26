"use client"

import { BusinessFailureState } from "@/components/business"
import {
    PreviewSection,
    PreviewNote,
} from "@/components/business/financial-preview"
import { FinancialFileList } from "@/features/file-assets/components/financial-file-list"
import { useInvoiceFiles } from "@/features/file-assets/hooks/business-finance-files"

export function SupplierInvoiceFiles({ invoiceId }: { invoiceId: string }) {
    const { query, download } = useInvoiceFiles(invoiceId)
    return (
        <PreviewSection title="发票文件">
            {query.isPending ? (
                <p role="status" className="text-muted-foreground">
                    正在读取发票文件…
                </p>
            ) : null}
            {query.isError ? (
                <BusinessFailureState
                    title="发票文件读取失败"
                    error={query.error}
                    onRetry={() => void query.refetch()}
                />
            ) : null}
            {query.isSuccess ? (
                query.data.length ? (
                    <FinancialFileList
                        files={query.data}
                        idPrefix="supplier-invoice-file"
                        showDocumentNumber={false}
                        onDownload={(file) => download.mutate(file)}
                        pendingAssetId={
                            download.isPending
                                ? download.variables?.file_asset_id
                                : undefined
                        }
                    />
                ) : (
                    <PreviewNote>未上传发票文件</PreviewNote>
                )
            ) : null}
        </PreviewSection>
    )
}
