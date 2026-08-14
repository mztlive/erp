"use client"

import { DocumentAttachmentList } from "@/components/business"
import type { ContractCenterView } from "@/features/contracts/types"

/** 附件分区：合同 PDF 电子档列表。 */
export function ContractDetailAttachments({
    contract,
}: {
    contract: ContractCenterView
}) {
    return (
        <div className="space-y-3">
            <DocumentAttachmentList
                title="合同 PDF 电子档"
                openLabel="下载"
                attachments={contract.attachments.map((file) => ({
                    id: file.id,
                    name: file.name,
                    description: `${file.uploadedBy} · ${file.uploadedAt}${
                        file.revisionNo != null
                            ? ` · v${file.revisionNo}`
                            : ""
                    }`,
                    state:
                        file.securityState === "done"
                            ? ("done" as const)
                            : file.securityState === "quarantined"
                              ? ("error" as const)
                              : ("processing" as const),
                    errorMessage:
                        file.securityState === "quarantined"
                            ? "安全检查未通过，已记录隔离状态。"
                            : undefined,
                    onOpen: undefined,
                }))}
            />
        </div>
    )
}
