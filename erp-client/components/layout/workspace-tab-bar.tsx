"use client"

import * as React from "react"
import {
    CheckIcon,
    ChevronLeftIcon,
    ChevronRightIcon,
    FileTextIcon,
    PanelsTopLeftIcon,
    XIcon,
} from "lucide-react"
import { Button } from "@/components/ui/button"
import {
    DropdownMenu,
    DropdownMenuContent,
    DropdownMenuGroup,
    DropdownMenuItem,
    DropdownMenuLabel,
    DropdownMenuSeparator,
    DropdownMenuShortcut,
    DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu"
import { WORKSPACE_NAV_GROUPS } from "@/lib/workspace-registry"
import { WORKSPACE_CLOSE_SHORTCUT } from "@/lib/workspace-tabs"
import { cn } from "@/lib/utils"
import type { WorkspaceTab } from "./workspace-tabs"

const navigationItems = WORKSPACE_NAV_GROUPS.flatMap(
    (group) => group.items,
).toSorted((a, b) => b.href.split("?")[0].length - a.href.split("?")[0].length)

function tabIcon(href: string) {
    const pathname = href.split(/[?#]/)[0]
    const navPathname = pathname.startsWith("/supplier-api/catalog")
        ? "/supplier-api/connections"
        : pathname
    return (
        navigationItems.find((item) => {
            const path = item.href.split("?")[0]
            return navPathname === path || navPathname.startsWith(`${path}/`)
        })?.icon ?? FileTextIcon
    )
}

export function WorkspaceTabBar({
    tabs,
    active,
    onSelect,
    onClose,
}: {
    tabs: WorkspaceTab[]
    active: string
    onSelect: (tab: WorkspaceTab) => void
    onClose: (ids: string[]) => void
}) {
    const strip = React.useRef<HTMLDivElement>(null)
    const track = React.useRef<HTMLDivElement>(null)
    const activeTitle = tabs.find((tab) => tab.id === active)?.title
    const [scroll, setScroll] = React.useState({
        left: false,
        right: false,
        overflow: false,
    })
    const measure = React.useCallback(() => {
        const node = strip.current
        if (!node) return
        const next = {
            left: node.scrollLeft > 1,
            right: node.scrollLeft + node.clientWidth < node.scrollWidth - 1,
            overflow: node.scrollWidth > node.clientWidth + 1,
        }
        setScroll((previous) =>
            previous.left === next.left &&
            previous.right === next.right &&
            previous.overflow === next.overflow
                ? previous
                : next,
        )
    }, [])

    React.useEffect(() => {
        const observer = new ResizeObserver(measure)
        if (strip.current) observer.observe(strip.current)
        if (track.current) observer.observe(track.current)
        measure()
        return () => observer.disconnect()
    }, [measure])

    React.useEffect(() => {
        const node = document.getElementById(
            `workspace-tab-${active}`,
        )?.parentElement
        const container = strip.current
        if (!node || !container) return
        // Scroll only the tab strip, never the outer workspace or the retained page.
        const bounds = node.getBoundingClientRect()
        const viewport = container.getBoundingClientRect()
        if (bounds.left < viewport.left)
            container.scrollLeft -= viewport.left - bounds.left
        else if (bounds.right > viewport.right)
            container.scrollLeft += bounds.right - viewport.right
        measure()
    }, [active, activeTitle, tabs.length, scroll.overflow, measure])

    const moveFocus = (event: React.KeyboardEvent, index: number) => {
        const next =
            event.key === "ArrowRight"
                ? (index + 1) % tabs.length
                : event.key === "ArrowLeft"
                  ? (index - 1 + tabs.length) % tabs.length
                  : event.key === "Home"
                    ? 0
                    : event.key === "End"
                      ? tabs.length - 1
                      : -1
        if (next < 0) return
        event.preventDefault()
        onSelect(tabs[next])
        document
            .getElementById(`workspace-tab-${tabs[next].id}`)
            ?.focus({ preventScroll: true })
    }

    const scrollBy = (direction: number) => {
        const node = strip.current
        if (!node) return
        node.scrollBy({
            left: direction * node.clientWidth * 0.75,
            behavior: window.matchMedia("(prefers-reduced-motion: reduce)")
                .matches
                ? "instant"
                : "smooth",
        })
    }

    return (
        <div
            data-slot="workspace-tab-bar"
            className="flex h-topbar shrink-0 items-center gap-2 border-b border-border bg-card px-workspace-inline md:px-page-inline min-[75rem]:px-page-inline-lg"
        >
            <div
                ref={strip}
                onScroll={measure}
                className="h-full min-w-0 flex-1 overflow-x-auto overscroll-x-contain [scrollbar-width:none] [&::-webkit-scrollbar]:hidden"
            >
                <div
                    ref={track}
                    role="tablist"
                    aria-label="已打开的工作页"
                    className="flex h-full w-max min-w-full items-center gap-1.5"
                >
                    {tabs.map((tab, index) => {
                        const selected = tab.id === active
                        const Icon = tabIcon(tab.href)
                        return (
                            <div
                                key={tab.id}
                                className={cn(
                                    "group/workspace-tab relative flex h-8 max-w-72 shrink-0 items-center rounded-md motion-safe:transition-colors",
                                    selected
                                        ? "bg-muted text-foreground"
                                        : "text-muted-foreground hover:bg-muted/55 hover:text-foreground",
                                )}
                            >
                                <button
                                    id={`workspace-tab-${tab.id}`}
                                    type="button"
                                    role="tab"
                                    aria-selected={selected}
                                    aria-controls={`workspace-panel-${tab.id}`}
                                    tabIndex={selected ? 0 : -1}
                                    title={`${tab.title}${tab.dirty ? "（输入已修改）" : ""}`}
                                    className={cn(
                                        "flex h-full min-w-0 flex-1 items-center gap-2 rounded-md pr-1 pl-2.5 text-body-compact outline-none focus-visible:ring-2 focus-visible:ring-ring/50 focus-visible:ring-inset",
                                        selected && "font-medium",
                                    )}
                                    onClick={() => onSelect(tab)}
                                    onKeyDown={(event) =>
                                        moveFocus(event, index)
                                    }
                                >
                                    <Icon
                                        className={cn(
                                            "size-3.5 shrink-0",
                                            selected
                                                ? "text-foreground/75"
                                                : "text-muted-foreground/75",
                                        )}
                                        aria-hidden="true"
                                    />
                                    <span className="truncate">
                                        {tab.title}
                                    </span>
                                </button>
                                <span className="relative mr-1 flex size-control-xs shrink-0 items-center justify-center">
                                    {tab.dirty ? (
                                        <span
                                            className="pointer-events-none size-1.5 rounded-full bg-warning motion-safe:transition-opacity group-hover/workspace-tab:opacity-0 group-focus-within/workspace-tab:opacity-0 [@media(hover:none)]:hidden"
                                            aria-label="输入已修改"
                                        />
                                    ) : null}
                                    <button
                                        id={`workspace-tab-close-${tab.id}`}
                                        type="button"
                                        aria-label={`关闭${tab.title}`}
                                        title={`关闭${tab.title}${selected ? `（${WORKSPACE_CLOSE_SHORTCUT}）` : ""}`}
                                        aria-keyshortcuts={
                                            selected
                                                ? WORKSPACE_CLOSE_SHORTCUT
                                                : undefined
                                        }
                                        className={cn(
                                            "absolute inset-0 flex items-center justify-center rounded-md text-muted-foreground outline-none hover:bg-foreground/8 hover:text-foreground focus-visible:opacity-100 focus-visible:ring-2 focus-visible:ring-ring/50 motion-safe:transition-opacity",
                                            tab.dirty
                                                ? "pointer-events-none opacity-0 group-hover/workspace-tab:pointer-events-auto group-hover/workspace-tab:opacity-70 group-focus-within/workspace-tab:pointer-events-auto group-focus-within/workspace-tab:opacity-100 [@media(hover:none)]:pointer-events-auto [@media(hover:none)]:opacity-70"
                                                : selected
                                                  ? "opacity-70"
                                                  : "opacity-0 group-hover/workspace-tab:opacity-70 group-focus-within/workspace-tab:opacity-70 [@media(hover:none)]:opacity-70",
                                        )}
                                        onClick={() => onClose([tab.id])}
                                    >
                                        <XIcon
                                            className="size-3.5"
                                            aria-hidden="true"
                                        />
                                    </button>
                                </span>
                            </div>
                        )
                    })}
                </div>
            </div>
            <div className="flex shrink-0 items-center gap-0.5 pl-2">
                {scroll.overflow ? (
                    <>
                        <Button
                            id="workspace-tabs-scroll-left"
                            variant="ghost"
                            size="icon-sm"
                            aria-label="向左滚动页签"
                            title="向左滚动页签"
                            disabled={!scroll.left}
                            className="text-muted-foreground"
                            onClick={() => scrollBy(-1)}
                        >
                            <ChevronLeftIcon className="size-3.5" />
                        </Button>
                        <Button
                            id="workspace-tabs-scroll-right"
                            variant="ghost"
                            size="icon-sm"
                            aria-label="向右滚动页签"
                            title="向右滚动页签"
                            disabled={!scroll.right}
                            className="text-muted-foreground"
                            onClick={() => scrollBy(1)}
                        >
                            <ChevronRightIcon className="size-3.5" />
                        </Button>
                    </>
                ) : null}
                <DropdownMenu>
                    <DropdownMenuTrigger
                        id="workspace-tabs-actions"
                        render={
                            <Button
                                variant="ghost"
                                size="sm"
                                className="gap-2 text-muted-foreground"
                                aria-label={`页签操作，已打开 ${tabs.length} 个页面`}
                                title="查看已打开的页面"
                            />
                        }
                    >
                        <PanelsTopLeftIcon className="size-3.5" />
                        <span className="num text-xs">{tabs.length}</span>
                    </DropdownMenuTrigger>
                    <DropdownMenuContent
                        align="end"
                        className="w-72 max-w-[calc(100vw-2rem)]"
                    >
                        <DropdownMenuGroup>
                            <DropdownMenuLabel>
                                已打开的页面 · {tabs.length}
                            </DropdownMenuLabel>
                            <div className="max-h-72 overflow-y-auto">
                                {tabs.map((tab) => {
                                    const Icon = tabIcon(tab.href)
                                    return (
                                        <DropdownMenuItem
                                            key={tab.id}
                                            id={`workspace-tabs-switch-${tab.id}`}
                                            aria-current={
                                                tab.id === active
                                                    ? "page"
                                                    : undefined
                                            }
                                            title={tab.title}
                                            onClick={() => onSelect(tab)}
                                        >
                                            <Icon
                                                className="text-muted-foreground"
                                                aria-hidden="true"
                                            />
                                            <span className="min-w-0 flex-1 truncate">
                                                {tab.title}
                                            </span>
                                            {tab.dirty ? (
                                                <span
                                                    className="size-1.5 shrink-0 rounded-full bg-warning"
                                                    aria-label="输入已修改"
                                                />
                                            ) : null}
                                            {tab.id === active ? (
                                                <CheckIcon aria-label="当前页面" />
                                            ) : null}
                                        </DropdownMenuItem>
                                    )
                                })}
                            </div>
                        </DropdownMenuGroup>
                        <DropdownMenuSeparator />
                        <DropdownMenuItem
                            id="workspace-tabs-close-current"
                            aria-keyshortcuts={WORKSPACE_CLOSE_SHORTCUT}
                            onClick={() => onClose([active])}
                        >
                            关闭当前页签
                            <DropdownMenuShortcut>
                                {WORKSPACE_CLOSE_SHORTCUT}
                            </DropdownMenuShortcut>
                        </DropdownMenuItem>
                        <DropdownMenuItem
                            id="workspace-tabs-close-others"
                            disabled={tabs.length < 2}
                            onClick={() =>
                                onClose(
                                    tabs
                                        .filter((tab) => tab.id !== active)
                                        .map((tab) => tab.id),
                                )
                            }
                        >
                            关闭其他页签
                        </DropdownMenuItem>
                        <DropdownMenuItem
                            id="workspace-tabs-close-right"
                            disabled={tabs.at(-1)?.id === active}
                            onClick={() =>
                                onClose(
                                    tabs
                                        .slice(
                                            tabs.findIndex(
                                                (tab) => tab.id === active,
                                            ) + 1,
                                        )
                                        .map((tab) => tab.id),
                                )
                            }
                        >
                            关闭右侧页签
                        </DropdownMenuItem>
                    </DropdownMenuContent>
                </DropdownMenu>
            </div>
        </div>
    )
}
