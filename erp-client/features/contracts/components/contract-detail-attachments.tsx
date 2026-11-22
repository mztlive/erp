"use client"

import * as React from "react"

import {
    DetailWorkspaceSection,
    DocumentAttachmentList,
} from "@/components/business"
import { ContractFilePreviewDialog } from "@/features/contracts/components/contract-file-preview-dialog"
import type { ContractCenterView } from "@/features/contracts/types"

/** 附件分区：合同 PDF 电子档列表，支持在线预览与下载。 */
export function ContractDetailAttachments({
    contract,
}: {
    contract: ContractCenterView
}) {
    const [previewFile, setPreviewFile] = React.useState<{
        id: string
        name: string
    } | null>(null)

    return (
        <DetailWorkspaceSection width="form">
            <DocumentAttachmentList
                idPrefix="contracts-detail-attachments"
                title="合同 PDF 电子档"
                openLabel="预览"
                attachments={contract.attachments.map((file) => ({
                    id: file.id,
                    name: file.name,
                    description: `${file.uploadedBy} · ${file.uploadedAt}${
                        file.revisionNo != null ? ` · v${file.revisionNo}` : ""
                    }`,
                    state:
                        file.securityState === "done"
                            ? ("done" as const)
                            : file.securityState === "quarantined"
                              ? ("error" as const)
                              : ("processing" as const),
                    errorMessage:
                        file.securityState === "quarantined"
                            ? "文件安全检测未通过，暂不支持下载，请重新上传合规的 PDF 文件。"
                            : undefined,
                    onOpen:
                        file.securityState === "done"
                            ? () =>
                                  setPreviewFile({
                                      id: file.id,
                                      name: file.name,
                                  })
                            : undefined,
                }))}
            />
            {previewFile ? (
                <ContractFilePreviewDialog
                    contractId={contract.contractId}
                    fileId={previewFile.id}
                    fileName={previewFile.name}
                    open
                    onOpenChange={(open) => {
                        if (!open) setPreviewFile(null)
                    }}
                />
            ) : null}
        </DetailWorkspaceSection>
    )
}
