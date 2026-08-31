"use client"

import * as React from "react"
import Link from "next/link"

import { Button } from "@/components/ui/button"
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
    return (
        <nav
            ref={navRef}
            aria-label={masterDataCopy.resourceNavAria}
            className="flex flex-wrap gap-2 border-b border-grid pb-3"
        >
            {MASTER_DATA_RESOURCES.map((item) => {
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
