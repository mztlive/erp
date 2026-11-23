"use client"

import { PdfDocumentCanvas } from "@/components/business/pdf-document-canvas"

/** 只渲染当前页；不执行 PDF 脚本，字体与解码资源从同源加载。 */
export function ContractPdfPages({
    blob,
    idPrefix = "contract-import-pdf",
}: {
    blob: Blob
    idPrefix?: string
}) {
    return (
        <PdfDocumentCanvas
            blob={blob}
            idPrefix={idPrefix}
            containPage
            containerAriaLabel="合同原文页，可滚动查看"
            canvasAriaLabel={(page) =>
                `合同原文第 ${page} 页，请通过“查看完整原文”阅读可选择的文本`
            }
            loadingLabel="正在渲染原文…"
            errorMessage="此文件暂时无法分页预览，请点击“查看完整原文”。"
        />
    )
}
