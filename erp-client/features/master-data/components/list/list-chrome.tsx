"use client"

import * as React from "react"
import Link from "next/link"

import { Button } from "@/components/ui/button"
import { useAccountProfileQuery } from "@/features/auth/queries"
import { masterDataListPermission } from "@/features/master-data/lib/data"
import { hasPermission } from "@/lib/permissions"
import { masterDataCopy } from "@/features/master-data/lib/copy"
import { MASTER_DATA_RESOURCES } from "@/features/master-data/types"
import { toAutomationIdSegment } from "@/lib/automation-id"

function ResourceNav({
    resource,
    navRef,
    idPrefix,
}: {
    resource: string
    navRef?: React.RefObject<HTMLElement | null>
    idPrefix?: string
}) {
    const prefix = idPrefix ?? "master-data-list-chrome-nav"
    const account = useAccountProfileQuery()
    const permissions = account.isSuccess ? account.data.permissions : undefined
    const visibleResources = MASTER_DATA_RESOURCES.filter((item) =>
        hasPermission(permissions, masterDataListPermission[item.key]),
    )
    if (visibleResources.length === 0) return null
    return (
        <nav
            ref={navRef}
            aria-label={masterDataCopy.resourceNavAria}
            className="flex flex-wrap gap-2 border-b border-grid pb-3"
        >
            {visibleResources.map((item) => {
                const selected = item.key === resource
                return (
                    <Button
                        id={`${prefix}-${toAutomationIdSegment(item.key)}`}
                        key={item.key}
                        size="sm"
                        aria-current={selected ? "page" : undefined}
                        variant={selected ? "secondary" : "ghost"}
                        render={<Link href={`/master-data/${item.key}`} />}
                    >
                        {item.label}
                    </Button>
                )
            })}
        </nav>
    )
}

/** 禁用按钮的阻断原因提示：disabled 状态下浏览器不显示 title，用外层 span 承载。 */
function DisabledActionHint({
    message,
    children,
}: {
    message?: string
    children: React.ReactNode
}) {
    return message ? (
        <span title={message} className="inline-flex">
            {children}
        </span>
    ) : (
        <>{children}</>
    )
}

export { DisabledActionHint, ResourceNav }
