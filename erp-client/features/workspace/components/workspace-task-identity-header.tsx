"use client"

import type { ReactNode } from "react"
import Link from "next/link"
import { ArrowLeftIcon } from "lucide-react"

import {
    useWorkspaceInspectorMode,
    useWorkspaceTaskNavigation,
    useWorkspaceTaskPage,
} from "@/components/business/workspace-task-pane"
import { useWorkspaceTab } from "@/components/layout/workspace-tab-context"
import { Button, buttonVariants } from "@/components/ui/button"
import { StatusBadge, type StatusTone } from "@/components/ui/status-badge"
import { getWorkItemStatusMeta } from "@/lib/task-status-dictionary"
import { cn } from "@/lib/utils"

import type { WorkspaceWorkItem } from "../types"
import { workspaceDetailTitle } from "../hooks/use-workspace-detail-title"
import { stripDocumentNumberPrefix } from "../lib/stable-number"
import { isBlockedWorkItem } from "../lib/work-item"
import { WorkspaceDocumentBadge } from "./workspace-document-badge"
import { WorkspaceTaskHeaderActions } from "./workspace-task-context"
import { WorkspaceTaskResponsibilityActions } from "./workspace-task-responsibility-actions"

export type WorkspaceCompactTaskHeaderProps = Readonly<{
    title: string
    primaryStatus?: Readonly<{ label: string; tone?: StatusTone }>
    badges?: ReactNode
    meta?: ReactNode
    documentNumber?: string
    actions?: ReactNode
    className?: string
}>

/**
 * 紧凑单行 56px 吸顶任务页头：
 * 将返回队列、对象标题、单据与状态徽章、单号元信息、队列步进（第 X/Y 项）与右侧工具图标融合为单行，
 * 替代传统双行 104px 页头，最大化释放正文首屏纵向高度。
 */
export function WorkspaceCompactTaskHeader({
    title,
    primaryStatus,
    badges,
    meta,
    documentNumber,
    actions,
    className,
}: WorkspaceCompactTaskHeaderProps) {
    const inspectorMode = useWorkspaceInspectorMode()
    const inWorkspaceTab = useWorkspaceTab()
    const navigation = useWorkspaceTaskNavigation()
    const effectiveBack = navigation?.back
    const back =
        inWorkspaceTab && effectiveBack?.visibility !== "always"
            ? undefined
            : effectiveBack

    if (inspectorMode) {
        if (!actions) return null
        return (
            <div
                data-slot="inspector-utility-actions"
                className={cn(
                    "flex min-w-0 flex-wrap items-center justify-end gap-1",
                    className,
                )}
            >
                {actions}
            </div>
        )
    }

    return (
        <header
            data-slot="detail-workspace-chrome"
            className={cn(
                "flex min-h-14 flex-wrap items-center justify-between gap-x-3 gap-y-1.5 border-b border-border/60 bg-card py-2",
                className,
            )}
        >
            <div className="flex min-w-0 flex-1 flex-wrap items-center gap-x-2 gap-y-1">
                {back ? (
                    <>
                        {back.href != null ? (
                            <Link
                                id={back.id}
                                href={back.href}
                                className={cn(
                                    buttonVariants({
                                        variant: "ghost",
                                        size: "xs",
                                    }),
                                    "-ml-1.5 shrink-0 text-muted-foreground hover:text-foreground",
                                )}
                            >
                                <ArrowLeftIcon aria-hidden="true" />
                                {back.label}
                            </Link>
                        ) : (
                            <Button
                                id={back.id}
                                type="button"
                                variant="ghost"
                                size="xs"
                                className="-ml-1.5 shrink-0 text-muted-foreground hover:text-foreground"
                                onClick={back.onClick}
                            >
                                <ArrowLeftIcon aria-hidden="true" />
                                {back.label}
                            </Button>
                        )}
                        <span
                            aria-hidden="true"
                            className="h-4 w-px shrink-0 bg-border/70"
                        />
                    </>
                ) : null}

                <h1
                    className="min-w-0 max-w-full truncate text-section-title font-semibold tracking-tight text-foreground"
                    title={title}
                >
                    {title}
                </h1>

                {primaryStatus ? (
                    <StatusBadge
                        label={primaryStatus.label}
                        tone={primaryStatus.tone}
                    />
                ) : null}
                {badges}

                {meta ? (
                    <div
                        className="flex min-w-0 flex-wrap items-center gap-x-2 gap-y-0.5 text-xs text-muted-foreground"
                        data-workspace-document-number={documentNumber}
                    >
                        {meta}
                    </div>
                ) : documentNumber ? (
                    <span
                        className="num text-xs text-muted-foreground select-all"
                        data-workspace-document-number={documentNumber}
                    >
                        {documentNumber}
                    </span>
                ) : null}
            </div>

            {navigation?.meta || actions ? (
                <div
                    data-slot="detail-page-header-actions"
                    className="ml-auto flex max-w-full shrink-0 flex-wrap items-center justify-end gap-1.5"
                >
                    {navigation?.meta ? (
                        <div className="flex shrink-0 items-center text-xs text-muted-foreground">
                            {navigation.meta}
                        </div>
                    ) : null}
                    {navigation?.meta && actions ? (
                        <span
                            aria-hidden="true"
                            className="mx-0.5 h-4 w-px shrink-0 bg-border/70"
                        />
                    ) : null}
                    {actions}
                </div>
            ) : null}
        </header>
    )
}

/**
 * 工作台右侧作业面标题栏：单据身份、标题、说明，以及问号/刷新/折叠右轨等轻动作。
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
    title?: string
    subtitle?: ReactNode
    badges?: ReactNode
    compact?: boolean
    children?: ReactNode
}) {
    const inspectorMode = useWorkspaceInspectorMode()
    const fullPage = useWorkspaceTaskPage()
    const blocked = isBlockedWorkItem(item)
    const overdue = item.dueBucket === "overdue"
    const resolvedSubtitle =
        subtitle !== undefined
            ? subtitle
            : [item.stableNumber].filter(Boolean).join(" · ")

    const number = stripDocumentNumberPrefix(item.stableNumber)
    const taskTabNumber =
        workspaceDetailTitle(item) === item.workItemTypeLabel.trim()
            ? undefined
            : number
    const taskStatus = { label: item.statusLabel, tone: item.statusTone }
    const blockedMeta = getWorkItemStatusMeta(
        "OPEN",
        item.processingState === "READY"
            ? "APPROVAL_BLOCKED"
            : item.processingState,
    )
    const urgency = blocked ? (
        <StatusBadge label={blockedMeta.label} tone={blockedMeta.tone} />
    ) : overdue ? (
        <StatusBadge label="已超期" tone="destructive" />
    ) : null
    const actions = (
        <WorkspaceTaskHeaderActions item={item}>
            <WorkspaceTaskResponsibilityActions item={item} />
            {children}
        </WorkspaceTaskHeaderActions>
    )

    if (inspectorMode) {
        return (
            <div
                data-slot="inspector-utility-actions"
                className="flex min-w-0 flex-wrap items-center justify-end gap-1"
            >
                {actions}
            </div>
        )
    }

    if (fullPage) {
        return (
            <WorkspaceCompactTaskHeader
                title={title ?? item.objectTitle}
                primaryStatus={badges == null ? taskStatus : undefined}
                badges={
                    <>
                        {!compact && <WorkspaceDocumentBadge item={item} />}
                        {badges}
                        {urgency}
                    </>
                }
                documentNumber={taskTabNumber}
                meta={
                    resolvedSubtitle ? (
                        <span className="num min-w-0 truncate">
                            {resolvedSubtitle}
                        </span>
                    ) : undefined
                }
                actions={actions}
            />
        )
    }

    return (
        <div className="flex min-w-0 items-start justify-between gap-3">
            <div
                className={
                    compact
                        ? "flex min-w-0 flex-wrap items-center gap-x-3 gap-y-1.5"
                        : "flex min-w-0 flex-col gap-1.5"
                }
            >
                {compact && (
                    <h2 className="break-words text-section-title font-semibold tracking-tight">
                        {title ?? item.objectTitle}
                    </h2>
                )}
                <div className="flex flex-wrap items-center gap-1.5">
                    {!compact && <WorkspaceDocumentBadge item={item} />}
                    {badges ?? <StatusBadge {...taskStatus} />}
                    {urgency}
                </div>
                {!compact && (
                    <h2 className="break-words text-section-title font-semibold tracking-tight">
                        {title ?? item.objectTitle}
                    </h2>
                )}
                {resolvedSubtitle ? (
                    <div className="text-xs text-muted-foreground">
                        {resolvedSubtitle}
                    </div>
                ) : null}
            </div>
            {actions}
        </div>
    )
}
