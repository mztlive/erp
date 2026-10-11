"use client"

import { useSearchParams } from "next/navigation"
import { RequireAuth } from "@/components/providers/auth-session-provider"
import { ApprovalMaterialPreview } from "../components/approval-material-preview"

export function ApprovalMaterialPage() {
    const params = useSearchParams()
    const instanceId = params.get("instance")?.trim()
    const assetId = params.get("file")?.trim()
    const sourceRevisionId =
        params.get("source_revision_id")?.trim() || undefined
    const fileName = params.get("name")?.trim() || "审批附件"

    return (
        <RequireAuth>
            {instanceId && assetId ? (
                <ApprovalMaterialPreview
                    key={`${instanceId}:${assetId}:${sourceRevisionId ?? ""}`}
                    instanceId={instanceId}
                    assetId={assetId}
                    fileName={fileName}
                    sourceRevisionId={sourceRevisionId}
                />
            ) : (
                <main className="p-6 text-sm" role="alert">
                    附件链接不完整，请返回审批提交资料重新打开。
                </main>
            )}
        </RequireAuth>
    )
}
