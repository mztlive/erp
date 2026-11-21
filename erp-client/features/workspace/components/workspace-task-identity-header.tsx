"use client"

import type { ReactNode } from "react"

import { StatusBadge } from "@/components/ui/status-badge"

import type { WorkspaceWorkItem } from "../types"
import { isBlockedWorkItem } from "../lib/work-item"
import { WorkspaceDocumentBadge } from "./workspace-document-badge"
import { WorkspaceTaskHeaderActions } from "./workspace-task-context"

/**
 * 工作台右侧作业面标题栏：单据身份、标题、说明，以及问号/全屏等轻动作。
 */
export function WorkspaceTaskIdentityHeader({
    item,
    title,
    subtitle,
    badges,
    compact = false,
    children,
}: {
    item: WorkspaceWorkItem
    title?: ReactNode
    subtitle?: ReactNode
    badges?: ReactNode
    compact?: boolean
    children?: ReactNode
}) {
    const blocked = isBlockedWorkItem(item)
    const overdue = item.dueBucket === "overdue"
    const resolvedSubtitle =
        subtitle !== undefined
            ? subtitle
            : [item.stableNumber].filter(Boolean).join(" · ")

    return (
        <>
            <div
                className={
                    compact
                        ? "flex min-w-0 flex-wrap items-center gap-x-3 gap-y-2"
                        : "flex min-w-0 flex-col gap-2"
                }
            >
                {compact && (
                    <h2 className="break-words text-section-title font-semibold tracking-tight">
                        {title ?? item.objectTitle}
                    </h2>
                )}
                <div className="flex flex-wrap items-center gap-2">
                    {!compact && <WorkspaceDocumentBadge item={item} />}
                    {badges ?? (
                        <StatusBadge
                            label={item.statusLabel}
                            tone={item.statusTone}
                        />
                    )}
                    {blocked ? (
                        <StatusBadge label="受阻" tone="warning" />
                    ) : overdue ? (
                        <StatusBadge label="已超期" tone="destructive" />
                    ) : null}
                </div>
                {!compact && (
                    <h2 className="break-words text-section-title font-semibold tracking-tight">
                        {title ?? item.objectTitle}
                    </h2>
                )}
                {resolvedSubtitle ? (
                    <div className="text-body-compact text-muted-foreground">
                        {resolvedSubtitle}
                    </div>
                ) : null}
            </div>
            <WorkspaceTaskHeaderActions item={item}>
                {children}
            </WorkspaceTaskHeaderActions>
        </>
    )
}
