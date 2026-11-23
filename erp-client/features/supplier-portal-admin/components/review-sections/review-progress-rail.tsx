"use client"

import {
    CheckCircle2Icon,
    ChevronsDownUpIcon,
    ChevronsUpDownIcon,
    TriangleAlertIcon,
} from "lucide-react"
import { Button } from "@/components/ui/button"
import { Progress } from "@/components/ui/progress"
import { cn } from "@/lib/utils"
import type { ReviewSectionId, ReviewSectionSummary } from "./types"

export function ReviewProgressRail({
    sections,
    activeSectionId,
    canProcess,
    allExpanded,
    onSelectSection,
    onToggleAll,
    onJumpFirstIssue,
}: {
    sections: readonly ReviewSectionSummary[]
    activeSectionId: ReviewSectionId
    canProcess: boolean
    allExpanded: boolean
    onSelectSection: (sectionId: ReviewSectionId) => void
    onToggleAll: () => void
    onJumpFirstIssue: () => void
}) {
    const totalCompleted = sections.reduce(
        (sum, section) => sum + section.completedCount,
        0,
    )
    const totalRequired = sections.reduce(
        (sum, section) => sum + section.totalCount,
        0,
    )
    const totalIssues = sections.reduce(
        (sum, section) => sum + section.issues.length,
        0,
    )
    const progressPercent =
        totalRequired > 0
            ? Math.round((totalCompleted / totalRequired) * 100)
            : 100

    return (
        <>
            {/* 窄屏：吸顶横向快捷跳转条 (< 960px) */}
            <nav
                aria-label="审核步骤快捷跳转"
                className="sticky top-[calc(var(--detail-chrome-height,12px)-12px)] z-10 -mx-1 border-b border-border/60 bg-card/95 px-1 py-2 backdrop-blur-xs @[960px]/workspace:hidden"
            >
                <div className="flex items-center gap-1.5 overflow-x-auto pb-0.5">
                    {sections.map((section) => {
                        const isActive = activeSectionId === section.id
                        const hasIssues =
                            canProcess && section.issues.length > 0
                        return (
                            <button
                                key={section.id}
                                id={`supplier-portal-review-mobile-anchor-${section.id}`}
                                type="button"
                                aria-current={isActive ? "step" : undefined}
                                onClick={() => onSelectSection(section.id)}
                                className={cn(
                                    "inline-flex shrink-0 cursor-pointer items-center gap-1.5 rounded-md border px-2.5 py-1.5 text-xs transition-colors outline-none focus-visible:ring-2 focus-visible:ring-ring",
                                    isActive
                                        ? "border-primary/40 bg-sidebar-accent/80 font-medium text-foreground"
                                        : "border-border/70 bg-card text-muted-foreground hover:bg-muted/60 hover:text-foreground",
                                )}
                            >
                                <span className="num text-tiny text-muted-foreground">
                                    {section.step}.
                                </span>
                                <span>{section.shortTitle}</span>
                                {section.totalCount > 0 ? (
                                    <span className="num text-tiny text-muted-foreground">
                                        {section.completedCount}/
                                        {section.totalCount}
                                    </span>
                                ) : null}
                                {hasIssues ? (
                                    <span className="inline-flex items-center gap-0.5 rounded-sm bg-destructive-soft px-1 py-0 text-tiny font-medium text-destructive-soft-foreground">
                                        <TriangleAlertIcon
                                            aria-hidden="true"
                                            className="size-3 shrink-0"
                                        />
                                        <span className="num">
                                            {section.issues.length}
                                        </span>
                                    </span>
                                ) : section.totalCount > 0 &&
                                  section.isComplete ? (
                                    <CheckCircle2Icon
                                        aria-hidden="true"
                                        className="size-3.5 shrink-0 text-success"
                                    />
                                ) : null}
                            </button>
                        )
                    })}
                </div>
            </nav>

            {/* 宽屏：左侧粘性进度锚点轨 (>= 960px) */}
            <aside
                aria-label="审核步骤进度导航"
                className="hidden @[960px]/workspace:sticky @[960px]/workspace:top-[var(--detail-chrome-height,1rem)] @[960px]/workspace:block @[960px]/workspace:self-start"
            >
                <div className="space-y-3 border-r border-border/60 pr-4">
                    <div className="space-y-2">
                        <div className="flex items-center justify-between gap-2">
                            <span className="text-xs font-semibold text-foreground">
                                审核进度
                            </span>
                            <span className="num text-xs font-medium text-muted-foreground">
                                {totalCompleted}/{totalRequired}
                            </span>
                        </div>
                        <Progress
                            value={progressPercent}
                            aria-label={`已完成 ${totalCompleted} 项，共 ${totalRequired} 项`}
                        />
                        {canProcess && totalIssues > 0 ? (
                            <p className="flex items-center gap-1 text-tiny text-destructive-soft-foreground">
                                <TriangleAlertIcon
                                    aria-hidden="true"
                                    className="size-3 shrink-0 text-destructive"
                                />
                                <span>
                                    尚有{" "}
                                    <span className="num font-semibold">
                                        {totalIssues}
                                    </span>{" "}
                                    项待补齐
                                </span>
                            </p>
                        ) : canProcess && totalRequired > 0 ? (
                            <p className="flex items-center gap-1 text-tiny text-success-soft-foreground">
                                <CheckCircle2Icon
                                    aria-hidden="true"
                                    className="size-3 shrink-0 text-success"
                                />
                                <span>全部核对项已就绪</span>
                            </p>
                        ) : null}
                    </div>

                    <ol className="space-y-1">
                        {sections.map((section) => {
                            const isActive = activeSectionId === section.id
                            const hasIssues =
                                canProcess && section.issues.length > 0
                            return (
                                <li key={section.id}>
                                    <button
                                        id={`supplier-portal-review-anchor-${section.id}`}
                                        type="button"
                                        aria-current={
                                            isActive ? "step" : undefined
                                        }
                                        onClick={() =>
                                            onSelectSection(section.id)
                                        }
                                        className={cn(
                                            "flex w-full cursor-pointer items-start gap-2 rounded-r-md border-l-2 px-2.5 py-2 text-left transition-colors outline-none focus-visible:ring-2 focus-visible:ring-ring",
                                            isActive
                                                ? "border-primary bg-sidebar-accent/70 text-foreground"
                                                : hasIssues
                                                  ? "border-destructive-border text-foreground hover:bg-muted/60"
                                                  : "border-transparent text-muted-foreground hover:bg-muted/50 hover:text-foreground",
                                        )}
                                    >
                                        <span
                                            aria-hidden="true"
                                            className={cn(
                                                "num mt-0.5 inline-flex size-4.5 shrink-0 items-center justify-center rounded-sm text-tiny font-semibold",
                                                hasIssues
                                                    ? "bg-destructive text-destructive-foreground"
                                                    : section.totalCount > 0 &&
                                                        section.isComplete
                                                      ? "bg-success-soft text-success-soft-foreground"
                                                      : isActive
                                                        ? "bg-primary text-primary-foreground"
                                                        : "bg-muted text-muted-foreground",
                                            )}
                                        >
                                            {section.step}
                                        </span>
                                        <span className="min-w-0 flex-1 space-y-1">
                                            <span
                                                className={cn(
                                                    "block truncate text-body-compact leading-snug",
                                                    isActive
                                                        ? "font-medium text-foreground"
                                                        : "font-normal",
                                                )}
                                            >
                                                {section.title}
                                            </span>
                                            <span className="flex flex-wrap items-center gap-1.5">
                                                {section.totalCount > 0 ? (
                                                    <span className="num text-tiny text-muted-foreground">
                                                        {section.completedCount}
                                                        /{section.totalCount}
                                                    </span>
                                                ) : (
                                                    <span className="text-tiny text-muted-foreground">
                                                        概览
                                                    </span>
                                                )}
                                                {hasIssues ? (
                                                    <span className="inline-flex items-center gap-0.5 rounded-sm bg-destructive-soft px-1.5 py-0 text-tiny font-medium text-destructive-soft-foreground">
                                                        <TriangleAlertIcon
                                                            aria-hidden="true"
                                                            className="size-2.5 shrink-0"
                                                        />
                                                        <span className="num">
                                                            {
                                                                section.issues
                                                                    .length
                                                            }{" "}
                                                            待补
                                                        </span>
                                                    </span>
                                                ) : section.totalCount > 0 &&
                                                  section.isComplete ? (
                                                    <span className="inline-flex items-center gap-0.5 text-tiny text-success-soft-foreground">
                                                        <CheckCircle2Icon
                                                            aria-hidden="true"
                                                            className="size-3 shrink-0 text-success"
                                                        />
                                                        已就绪
                                                    </span>
                                                ) : null}
                                            </span>
                                        </span>
                                    </button>
                                </li>
                            )
                        })}
                    </ol>

                    <div className="space-y-1.5 border-t border-border/60 pt-2.5">
                        {canProcess && totalIssues > 0 ? (
                            <Button
                                id="supplier-portal-review-jump-first-issue"
                                type="button"
                                size="sm"
                                variant="outline"
                                className="w-full justify-start border-destructive-border text-xs text-destructive-soft-foreground hover:bg-destructive-soft"
                                onClick={onJumpFirstIssue}
                            >
                                <TriangleAlertIcon
                                    data-icon="inline-start"
                                    aria-hidden="true"
                                />
                                定位首个待补项
                            </Button>
                        ) : null}
                        <Button
                            id="supplier-portal-review-sections-toggle-all"
                            type="button"
                            size="sm"
                            variant="ghost"
                            className="w-full justify-start text-xs text-muted-foreground hover:text-foreground"
                            onClick={onToggleAll}
                        >
                            {allExpanded ? (
                                <>
                                    <ChevronsDownUpIcon
                                        data-icon="inline-start"
                                        aria-hidden="true"
                                    />
                                    全部收起
                                </>
                            ) : (
                                <>
                                    <ChevronsUpDownIcon
                                        data-icon="inline-start"
                                        aria-hidden="true"
                                    />
                                    全部展开
                                </>
                            )}
                        </Button>
                    </div>
                </div>
            </aside>
        </>
    )
}
