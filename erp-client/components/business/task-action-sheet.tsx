"use client"

import type * as React from "react"
import { ChevronLeftIcon, ChevronRightIcon, Maximize2Icon } from "lucide-react"

import { Button } from "@/components/ui/button"
import {
    Sheet,
    SheetContent,
    SheetDescription,
    SheetFooter,
    SheetHeader,
    SheetTitle,
} from "@/components/ui/sheet"
import { cn } from "@/lib/utils"

type SheetProps = React.ComponentProps<typeof Sheet>
type SheetContentProps = React.ComponentProps<typeof SheetContent>

export type TaskActionSheetProps = Readonly<{
    open: boolean
    onOpenChange: (open: boolean) => void
    title: React.ReactNode
    subtitle?: React.ReactNode
    badges?: React.ReactNode
    headerActions?: React.ReactNode
    onPrev?: () => void
    onNext?: () => void
    hasPrev?: boolean
    hasNext?: boolean
    positionLabel?: string
    onOpenFullPage?: () => void
    /** 完整页面按钮文案，默认「完整页面」。 */
    openFullPageLabel?: string
    footerLeading?: React.ReactNode
    footerActions?: React.ReactNode
    children: React.ReactNode
    idPrefix?: string
    finalFocus?: SheetContentProps["finalFocus"]
    onOpenChangeComplete?: SheetProps["onOpenChangeComplete"]
    bodyClassName?: string
}>

/**
 * 有界轻作业抽屉（ADR-3 双轨规范之轻作业轨道）。
 *
 * 固定采用 `size="detail"`（复用 `--spacing-detail` = 768px，最大 92vw），
 * 专用于工作台窄屏快审、库存调整与财务侧边审批等有界轻作业场景：
 * - 固定顶栏（`SheetHeader` 自动 `pr-12`，含队列步进、完整页面入口与自定义动作）
 * - 独立滚动正文（`px-sheet-inline py-5`，与头尾严格左对齐并响应高度紧凑断点）
 * - 固定底栏（`SheetFooter`，左次要/状态提示，右主操作）
 *
 * 禁止在抽屉内部再套娃打开第二个抽屉（Sheet-on-Sheet）。
 */
export function TaskActionSheet({
    open,
    onOpenChange,
    title,
    subtitle,
    badges,
    headerActions,
    onPrev,
    onNext,
    hasPrev,
    hasNext,
    positionLabel,
    onOpenFullPage,
    openFullPageLabel = "完整页面",
    footerLeading,
    footerActions,
    children,
    idPrefix,
    finalFocus,
    onOpenChangeComplete,
    bodyClassName,
}: TaskActionSheetProps) {
    const closeButtonId = idPrefix ? `${idPrefix}-close` : undefined
    const prevButtonId = idPrefix ? `${idPrefix}-prev` : undefined
    const nextButtonId = idPrefix ? `${idPrefix}-next` : undefined
    const fullPageButtonId = idPrefix ? `${idPrefix}-open-full-page` : undefined

    const showStepper = Boolean(onPrev || onNext || positionLabel)
    const showHeaderBar = Boolean(
        showStepper || onOpenFullPage || headerActions,
    )
    const showFooter = Boolean(footerLeading || footerActions)

    return (
        <Sheet
            open={open}
            onOpenChange={(nextOpen) => onOpenChange(nextOpen)}
            onOpenChangeComplete={onOpenChangeComplete}
        >
            <SheetContent
                side="right"
                size="detail"
                closeButtonId={closeButtonId}
                finalFocus={finalFocus}
                data-business-component="task-action-sheet"
            >
                <SheetHeader className="gap-2.5 border-b border-border bg-card">
                    {showHeaderBar ? (
                        <div
                            data-slot="task-action-sheet-toolbar"
                            className="flex flex-wrap items-center justify-between gap-2"
                        >
                            {showStepper ? (
                                <div className="flex items-center gap-1">
                                    {positionLabel ? (
                                        <span className="num mr-1 text-xs text-muted-foreground">
                                            {positionLabel}
                                        </span>
                                    ) : null}
                                    {onPrev ? (
                                        <Button
                                            id={prevButtonId}
                                            type="button"
                                            variant="ghost"
                                            size="icon-sm"
                                            disabled={!hasPrev}
                                            aria-label="上一项"
                                            title="上一项"
                                            onClick={onPrev}
                                        >
                                            <ChevronLeftIcon
                                                className="size-4"
                                                aria-hidden="true"
                                            />
                                        </Button>
                                    ) : null}
                                    {onNext ? (
                                        <Button
                                            id={nextButtonId}
                                            type="button"
                                            variant="ghost"
                                            size="icon-sm"
                                            disabled={!hasNext}
                                            aria-label="下一项"
                                            title="下一项"
                                            onClick={onNext}
                                        >
                                            <ChevronRightIcon
                                                className="size-4"
                                                aria-hidden="true"
                                            />
                                        </Button>
                                    ) : null}
                                </div>
                            ) : (
                                <div />
                            )}
                            {onOpenFullPage || headerActions ? (
                                <div className="flex flex-wrap items-center gap-1.5">
                                    {headerActions}
                                    {onOpenFullPage ? (
                                        <Button
                                            id={fullPageButtonId}
                                            type="button"
                                            variant="ghost"
                                            size="xs"
                                            className="text-muted-foreground hover:text-foreground"
                                            onClick={onOpenFullPage}
                                        >
                                            <Maximize2Icon
                                                className="size-3.5"
                                                aria-hidden="true"
                                            />
                                            <span>{openFullPageLabel}</span>
                                        </Button>
                                    ) : null}
                                </div>
                            ) : null}
                        </div>
                    ) : null}

                    <div className="flex min-w-0 flex-col gap-1">
                        <SheetTitle>{title}</SheetTitle>
                        {subtitle ? (
                            <SheetDescription>{subtitle}</SheetDescription>
                        ) : null}
                    </div>

                    {badges ? (
                        <div
                            data-slot="task-action-sheet-badges"
                            className="flex flex-wrap items-center gap-1.5"
                        >
                            {badges}
                        </div>
                    ) : null}
                </SheetHeader>

                <div
                    data-slot="task-action-sheet-body"
                    className={cn(
                        "min-h-0 flex-1 overflow-y-auto overscroll-contain px-sheet-inline py-5",
                        bodyClassName,
                    )}
                >
                    {children}
                </div>

                {showFooter ? (
                    <SheetFooter
                        data-slot="task-action-sheet-footer"
                        className="justify-between border-t border-border bg-card"
                    >
                        <div
                            data-slot="task-action-sheet-footer-leading"
                            className="flex min-w-0 flex-wrap items-center gap-2"
                        >
                            {footerLeading}
                        </div>
                        <div
                            data-slot="task-action-sheet-footer-actions"
                            className="flex shrink-0 flex-wrap items-center justify-end gap-2"
                        >
                            {footerActions}
                        </div>
                    </SheetFooter>
                ) : null}
            </SheetContent>
        </Sheet>
    )
}
