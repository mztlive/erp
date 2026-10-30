"use client"

import { LoadingButton } from "@/components/ui/loading-button"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { useDepositResolutionEvidence } from "../hooks/queries"

/** 使用受控读取入口交付核实凭证，错误保留在原对账记录旁。 */
export function DepositResolutionEvidence({
    reconciliationId,
    assetId,
}: {
    reconciliationId: string
    assetId: string
}) {
    const download = useDepositResolutionEvidence(assetId)
    return (
        <span className="inline-flex flex-col items-start gap-1">
            <LoadingButton
                id={`supplier-deposits-resolution-evidence-${toAutomationIdSegment(reconciliationId)}`}
                type="button"
                variant="link"
                size="sm"
                className="h-auto p-0"
                loading={download.isPending}
                disabled={download.isPending}
                onClick={() => download.mutate()}
            >
                下载核实凭证
            </LoadingButton>
            {download.isError ? (
                <span role="alert" className="text-xs text-destructive">
                    凭证读取失败，请确认文件查看权限后重试。
                </span>
            ) : null}
        </span>
    )
}
