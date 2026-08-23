"use client"

import * as React from "react"

import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs"
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
    /** 分区导航列表额外 class；默认已含吸顶与底边。 */
    listClassName?: string
    /** 传给 TabsList 的无障碍标签。 */
    listLabel?: string
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
    className,
    listClassName,
    listLabel = "对象分区",
    ...props
}: ObjectSectionTabsProps) {
    return (
        <Tabs
            data-slot="object-section-tabs"
            value={value}
            onValueChange={(next) => {
                if (next) onValueChange(next)
            }}
            className={cn("gap-0", className)}
            {...props}
        >
            <TabsList
                variant="line"
                aria-label={listLabel}
                className={cn(
                    "sticky top-0 z-10 h-auto w-full justify-start gap-1 overflow-x-auto rounded-none border-b border-grid bg-card/95 px-4 py-1.5 md:px-5",
                    "backdrop-blur supports-backdrop-filter:bg-card/80",
                    "group-data-horizontal/tabs:h-auto",
                    listClassName,
                )}
            >
                {items.map((item) => (
                    <TabsTrigger
                        key={item.id}
                        value={item.id}
                        title={item.title}
                        className="h-10 flex-none gap-1.5 rounded-none px-3 text-sm after:inset-x-3 after:bottom-0 after:h-0.5 data-active:font-semibold"
                    >
                        {item.label}
                        {item.badge != null ? item.badge : null}
                    </TabsTrigger>
                ))}
            </TabsList>
            {children}
        </Tabs>
    )
}

const objectSectionPanelClassName = "space-y-6 p-5 md:p-6"

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
