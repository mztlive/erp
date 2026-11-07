"use client"

import type * as React from "react"

import { CardTitle } from "@/components/ui/card"

/** 审批卡标题。对象中心 tab 内用小号标题，与概览正文对齐。 */
export function ApprovalSectionHeading({
    compact,
    children,
}: {
    compact?: boolean
    children: React.ReactNode
}) {
    if (compact) {
        return <h2 className="text-sm font-medium">{children}</h2>
    }
    return <CardTitle>{children}</CardTitle>
}
