"use client"

import type { ReactNode } from "react"

import { DetailWorkspaceSection } from "@/components/business"

/** 采购详情内的无框纯白业务分区：由顶部 1px 细线与留白建立秩序，保留 region 语义名。 */
export function PurchaseOrderDetailSection({
    title,
    count,
    description,
    action,
    width = "full",
    children,
}: {
    title: string
    count?: number
    description?: ReactNode
    action?: ReactNode
    compact?: boolean
    width?: "form" | "full"
    children: ReactNode
}) {
    return (
        <DetailWorkspaceSection
            width={width}
            title={title}
            ariaLabel={title}
            badge={
                count != null ? (
                    <span className="num rounded-md bg-muted px-1.5 py-0.5 text-xs text-muted-foreground">
                        {count}
                    </span>
                ) : null
            }
            subtitle={description}
            actions={action}
        >
            {children}
        </DetailWorkspaceSection>
    )
}
