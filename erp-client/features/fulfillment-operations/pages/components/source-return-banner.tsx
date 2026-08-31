"use client"

import Link from "next/link"

import { surfaceInsetClassName } from "@/components/business"
import { Button } from "@/components/ui/button"
import type { FulfillmentOperation } from "@/features/fulfillment-operations/types"
import { workspaceLabel } from "@/lib/ui-text"
import type { WorkspaceId } from "@/lib/workspace-registry"

export type SourceReturnBannerProps = {
    fromWorkspace: string | undefined
    sourceReturnHref: string | undefined
    operation: FulfillmentOperation | undefined
}

/** 跨页深链的来源提示条：说清从哪里进来、返回会回到哪里。 */
export function SourceReturnBanner({
    fromWorkspace,
    sourceReturnHref,
    operation,
}: SourceReturnBannerProps) {
    if (!sourceReturnHref) return null
    return (
        <div
            className={`${surfaceInsetClassName} flex flex-wrap items-center justify-between gap-2 px-3 py-2.5 text-sm`}
        >
            <span className="text-muted-foreground">
                从
                {fromWorkspace
                    ? workspaceLabel(fromWorkspace as WorkspaceId)
                    : "关联页面"}
                进来的
                {operation
                    ? ` · 已经定位到 ${operation.source.salesOrderNo}${
                          operation.source.purchaseNo
                              ? ` / ${operation.source.purchaseNo}`
                              : ""
                      }`
                    : ""}
                。返回时会回到原来的位置。
            </span>
            <Button
                id="fulfillment-operations-source-return-back"
                type="button"
                size="sm"
                variant="ghost"
                render={<Link href={sourceReturnHref} />}
            >
                返回来源
            </Button>
        </div>
    )
}
