"use client"

/* eslint-disable jsx-a11y/no-noninteractive-tabindex -- 独立滚动区使用具名 region，并须支持键盘翻页。 */

import * as React from "react"

import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { cn } from "@/lib/utils"

export type ObjectSectionTabItem = Readonly<{
    id: string
    label: React.ReactNode
    /** 悬停提示；长标签场景可用。 */
    title?: string
    /** 角标（待办 / 改单中等），渲染在标签右侧。 */
    badge?: React.ReactNode
}>

export type ObjectSectionTabsProps = Omit<
    React.ComponentProps<typeof Tabs>,
    "children" | "onValueChange" | "value"
> & {
    value: string
    onValueChange: (value: string) => void
    items: readonly ObjectSectionTabItem[]
    children: React.ReactNode
    /** 分区正文右侧的摘要；与正文共用起点，在窄屏下移至正文之后。 */
    sidebar?: React.ReactNode
    /** 固定高度详情页中，正文与侧栏独立滚动，分区导航保持可见。 */
    scrollMode?: "page" | "panels"
    /** 操作结果或专项处理区，与分区正文一起滚动。 */
    beforeContent?: React.ReactNode
    /** 分区导航列表额外 class；默认已含吸顶与底边。 */
    listClassName?: string
    /** 传给 TabsList 的无障碍标签。 */
    listLabel?: string
    id?: string
    idPrefix?: string
}

/**
 * 对象中心分区导航：吸顶 line Tabs + 统一内边距的内容区。
 * 各业务页只提供 items 与 TabsContent，不再复制 sticky / 底边 class。
 */
function ObjectSectionTabs({
    value,
    onValueChange,
    items,
    children,
    sidebar,
    scrollMode = "page",
    beforeContent,
    className,
    listClassName,
    listLabel = "对象分区",
    id,
    idPrefix,
    ...props
}: ObjectSectionTabsProps) {
    const baseId = idPrefix ?? id
    const panelScroll = scrollMode === "panels"
    const bodyRef = React.useRef<HTMLDivElement>(null)
    const contentRef = React.useRef<HTMLDivElement>(null)
    React.useEffect(() => {
        if (!panelScroll) return
        bodyRef.current?.scrollTo({ top: 0 })
        contentRef.current?.scrollTo({ top: 0 })
    }, [value, panelScroll])
    return (
        <Tabs
            id={baseId}
            data-slot="object-section-tabs"
            value={value}
            onValueChange={(next) => {
                if (next) onValueChange(next)
            }}
            className={cn(
                "gap-0",
                panelScroll && "min-h-0 flex-1 overflow-hidden",
                className,
            )}
            {...props}
        >
            <TabsList
                variant="line"
                aria-label={listLabel}
                className={cn(
                    "sticky top-0 z-10 h-auto w-full justify-start gap-5 overflow-x-auto rounded-none border-b border-border bg-card px-0 py-0",

                    "group-data-horizontal/tabs:h-auto",
                    panelScroll && "shrink-0",
                    listClassName,
                )}
            >
                {items.map((item) => (
                    <TabsTrigger
                        key={item.id}
                        id={
                            baseId
                                ? `${baseId}-tab-${toAutomationIdSegment(item.id)}`
                                : undefined
                        }
                        value={item.id}
                        title={item.title}
                        className="h-12 flex-none gap-2 rounded-none px-0 text-[13px] after:inset-x-0 after:bottom-0 after:h-0.5 data-active:font-semibold"
                    >
                        {item.label}
                        {item.badge != null ? item.badge : null}
                    </TabsTrigger>
                ))}
            </TabsList>
            {sidebar != null ? (
                <div
                    ref={bodyRef}
                    id={
                        panelScroll && baseId
                            ? `${baseId}-body-scroll`
                            : undefined
                    }
                    tabIndex={panelScroll ? 0 : undefined}
                    role={panelScroll ? "region" : undefined}
                    aria-label={panelScroll ? `${listLabel}内容区` : undefined}
                    className={cn(
                        "grid min-w-0 items-start gap-6 py-6 xl:grid-cols-[minmax(0,1fr)_340px] 2xl:grid-cols-[minmax(0,1fr)_400px]",
                        panelScroll &&
                            "min-h-0 flex-1 overflow-y-auto overscroll-contain xl:grid-rows-[minmax(0,1fr)] xl:overflow-hidden",
                    )}
                >
                    <div
                        ref={contentRef}
                        id={
                            panelScroll && baseId
                                ? `${baseId}-content-scroll`
                                : undefined
                        }
                        tabIndex={panelScroll ? 0 : undefined}
                        role={panelScroll ? "region" : undefined}
                        aria-label={
                            panelScroll ? `${listLabel}明细` : undefined
                        }
                        className={cn(
                            "min-w-0 [&>[data-slot=object-section-tabs-panel]]:py-0",
                            panelScroll &&
                                "xl:h-full xl:min-h-0 xl:overflow-y-auto xl:overscroll-contain",
                        )}
                    >
                        {beforeContent}
                        {children}
                    </div>
                    {panelScroll ? (
                        <div
                            tabIndex={0}
                            id={baseId ? `${baseId}-sidebar-scroll` : undefined}
                            role="region"
                            aria-label={`${listLabel}摘要`}
                            className="min-w-0 xl:max-h-full xl:overflow-y-auto xl:overscroll-contain"
                        >
                            {sidebar}
                        </div>
                    ) : (
                        sidebar
                    )}
                </div>
            ) : (
                <>
                    {beforeContent}
                    {children}
                </>
            )}
        </Tabs>
    )
}

const objectSectionPanelClassName = cn(
    "min-w-0 space-y-6 px-0 py-6",
    "[&_[data-slot=card]]:rounded-none [&_[data-slot=card]]:border-0 [&_[data-slot=card]]:bg-card [&_[data-slot=card]]:shadow-none",
    "[&_[data-slot=card-header]]:px-0 [&_[data-slot=card-content]]:px-0 [&_[data-slot=card-footer]]:px-0",
)

function ObjectSectionTabsPanel({
    className,
    ...props
}: React.ComponentProps<typeof TabsContent>) {
    return (
        <TabsContent
            data-slot="object-section-tabs-panel"
            className={cn(objectSectionPanelClassName, className)}
            {...props}
        />
    )
}

export {
    ObjectSectionTabs,
    ObjectSectionTabsPanel,
    objectSectionPanelClassName,
}
