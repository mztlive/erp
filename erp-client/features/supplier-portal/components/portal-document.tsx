"use client"

import { useMutation } from "@tanstack/react-query"
import { Button } from "@/components/ui/button"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { portalFile } from "../api"
import { PortalError } from "./surface"

/** 原稿资料始终通过当前申请授权读取，PDF不当成图片渲染。 */
export function PortalDocument({
    applicationId,
    assetId,
    label,
    idPrefix,
}: {
    applicationId: string
    assetId: string
    label: string
    idPrefix: string
}) {
    const download = useMutation({
        mutationFn: async () => {
            const blob = await portalFile(
                { request_id: applicationId },
                assetId,
            )
            const url = URL.createObjectURL(blob)
            const link = document.createElement("a")
            link.href = url
            link.download =
                blob.type === "application/pdf" ? `${label}.pdf` : label
            link.click()
            setTimeout(() => URL.revokeObjectURL(url), 1000)
        },
        retry: false,
    })
    return (
        <div className="space-y-2">
            <Button
                id={`${idPrefix}-${toAutomationIdSegment(applicationId)}-${toAutomationIdSegment(assetId)}`}
                type="button"
                variant="outline"
                size="sm"
                disabled={download.isPending}
                onClick={() => download.mutate()}
            >
                {download.isPending ? "正在读取…" : `下载${label}`}
            </Button>
            <PortalError error={download.error} />
        </div>
    )
}
