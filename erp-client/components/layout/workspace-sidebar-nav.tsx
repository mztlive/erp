"use client"

import * as React from "react"
import Link from "next/link"
import {
    BarChart3Icon,
    LayersIcon,
    LayoutGridIcon,
    PackageIcon,
    SearchIcon,
    ShoppingBagIcon,
    SlidersHorizontalIcon,
    WalletIcon,
    type LucideIcon,
} from "lucide-react"
import {
    SidebarMenu,
    SidebarMenuButton,
    SidebarMenuItem,
    SidebarTrigger,
    useSidebar,
} from "@/components/ui/sidebar"
import {
    Tooltip,
    TooltipContent,
    TooltipTrigger,
} from "@/components/ui/tooltip"
import { getErrorMessage } from "@/lib/api/errors"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { isNavItemActive } from "@/lib/nav-active"
import { cn } from "@/lib/utils"
import type {
    WorkspaceNavDomain,
    WorkspaceNavGroup,
} from "@/lib/workspace-registry"
import {
    dispatchWorkspaceQuickSearch,
    WorkspaceQuickSearch,
} from "./workspace-quick-search"
import { badgeCountFor, useWorkspaceSidebar } from "./workspace-sidebar-context"

type DomainMeta = Readonly<{
    shortLabel: string
    title: string
    icon: LucideIcon
    placement: "primary" | "bottom"
}>

const DOMAIN_META: Record<WorkspaceNavDomain, DomainMeta> = {
    work: {
        shortLabel: "工作",
        title: "工作台",
        icon: LayoutGridIcon,
        placement: "primary",
    },
    sales: {
        shortLabel: "销售",
        title: "销售",
        icon: ShoppingBagIcon,
        placement: "primary",
    },
    procurement: {
        shortLabel: "采购",
        title: "采购与库存",
        icon: PackageIcon,
        placement: "primary",
    },
    finance: {
        shortLabel: "财务",
        title: "财务",
        icon: WalletIcon,
        placement: "primary",
    },
    analytics: {
        shortLabel: "分析",
        title: "经营分析",
        icon: BarChart3Icon,
        placement: "primary",
    },
    "master-data": {
        shortLabel: "资料",
        title: "基础资料",
        icon: LayersIcon,
        placement: "bottom",
    },
    governance: {
        shortLabel: "系统",
        title: "系统与治理",
        icon: SlidersHorizontalIcon,
        placement: "bottom",
    },
}

/** 精炼几何品牌徽标：双层协同模块结构，替代通用写字楼图标。 */
function WorkspaceBrandMark() {
    return (
        <div className="flex size-7.5 shrink-0 items-center justify-center rounded-lg bg-gradient-to-br from-primary to-primary/85 text-primary-foreground shadow-xs ring-1 ring-primary/15">
            <svg
                viewBox="0 0 20 20"
                fill="none"
                className="size-4"
                aria-hidden="true"
            >
                <rect
                    x="3"
                    y="3.25"
                    width="6.25"
                    height="6.25"
                    rx="1.75"
                    fill="currentColor"
                />
                <rect
                    x="10.75"
                    y="3.25"
                    width="6.25"
                    height="6.25"
                    rx="1.75"
                    fill="currentColor"
                    fillOpacity="0.45"
                />
                <rect
                    x="3"
                    y="11"
                    width="6.25"
                    height="5.75"
                    rx="1.75"
                    fill="currentColor"
                    fillOpacity="0.65"
                />
                <rect
                    x="10.75"
                    y="11"
                    width="6.25"
                    height="5.75"
                    rx="1.75"
                    fill="currentColor"
                />
            </svg>
        </div>
    )
}

function badgeAriaLabel(
    badgeKey: WorkspaceNavGroup["items"][number]["badge"],
    badgeCount: number | undefined,
): string {
    switch (badgeKey) {
        case "copy-unread-count":
            return `${badgeCount} 条未读抄送`
        case "approval-started-unread-count":
            return `${badgeCount} 条审批有新进展待查看`
        case "inquiry-feedback-count":
            return `${badgeCount} 张询价单有新反馈待处理`
        case "procurement-inquiry-count":
            return `${badgeCount} 张采购询价待处理`
        case "background-job-active-count":
            return `${badgeCount} 项后台任务尚未完成`
        default:
            return `${badgeCount} 项待处理`
    }
}

export function WorkspaceSidebarNav() {
    const { state, isMobile, setOpen, setOpenMobile } = useSidebar()
    const {
        navPathname,
        search,
        profileStatus,
        profileError,
        navGroups,
        allHrefs,
        counts,
        badgeErrorFor,
        domainBadgeFor,
        canViewBackgroundJobs,
        canViewSelectionBooklets,
        currentDomain,
        routeDomain,
        availableDomains,
        setDomain,
    } = useWorkspaceSidebar()

    const [scrolled, setScrolled] = React.useState(false)
    const scrollRef = React.useRef<HTMLElement>(null)

    React.useEffect(() => {
        if (scrollRef.current) {
            scrollRef.current.scrollTop = 0
        }
        setScrolled(false)
    }, [currentDomain])

    const primaryDomains = availableDomains.filter(
        (domain) => DOMAIN_META[domain].placement === "primary",
    )
    const bottomDomains = availableDomains.filter(
        (domain) => DOMAIN_META[domain].placement === "bottom",
    )
    const domainGroups = navGroups.filter(
        (group) => group.domain === currentDomain,
    )
    const activeMeta = DOMAIN_META[currentDomain]
    const showCollapseTrigger = !isMobile && state === "expanded"

    const renderRailButton = (domain: WorkspaceNavDomain) => {
        const meta = DOMAIN_META[domain]
        const Icon = meta.icon
        const selected = currentDomain === domain
        const hasActiveRoute = routeDomain === domain && !selected
        const badge = domainBadgeFor(domain)
        const deliveryFallback =
            domain === "governance" &&
            canViewBackgroundJobs &&
            currentDomain !== "governance"
                ? "governance-background-jobs"
                : domain === "sales" &&
                    canViewSelectionBooklets &&
                    currentDomain !== "sales"
                  ? "sales-selection"
                  : undefined

        const button = (
            <button
                key={domain}
                id={`workspace-sidebar-domain-${domain}`}
                type="button"
                data-workspace-nav={deliveryFallback}
                aria-pressed={selected}
                aria-label={meta.title}
                onClick={() => {
                    if (state === "collapsed" && !isMobile) {
                        setOpen(true)
                    }
                    setDomain(domain)
                }}
                className={cn(
                    "group/rail-btn relative flex w-11.5 cursor-pointer flex-col items-center justify-center gap-1 rounded-lg py-1.5 transition-[background-color,color,box-shadow] duration-150 outline-none focus-visible:ring-2 focus-visible:ring-sidebar-ring",
                    selected
                        ? "bg-card text-primary shadow-2xs ring-1 ring-sidebar-border/80"
                        : "text-sidebar-foreground/75 hover:bg-foreground/[0.045] hover:text-foreground",
                )}
            >
                <Icon
                    className={cn(
                        "size-4.5 shrink-0 stroke-[1.7] transition-colors",
                        selected
                            ? "text-primary"
                            : "text-sidebar-foreground/75 group-hover/rail-btn:text-foreground",
                    )}
                    aria-hidden="true"
                />
                <span
                    className={cn(
                        "text-2xs leading-none tracking-tight",
                        selected
                            ? "font-semibold text-foreground"
                            : "font-medium",
                    )}
                >
                    {meta.shortLabel}
                </span>
                {badge.count > 0 ? (
                    <span
                        aria-label={`${meta.title}有 ${badge.count} 项待关注`}
                        className={cn(
                            "pointer-events-none absolute top-1.5 right-1.5 size-1.5 rounded-full ring-2 ring-sidebar-rail",
                            selected && "ring-card",
                            badge.urgent ? "bg-destructive" : "bg-primary",
                        )}
                    />
                ) : hasActiveRoute ? (
                    <span
                        aria-hidden="true"
                        className="pointer-events-none absolute top-1/2 left-0 h-3 w-0.5 -translate-y-1/2 rounded-r-full bg-primary/70"
                    />
                ) : null}
            </button>
        )

        if (state === "collapsed" && !isMobile) {
            return (
                <Tooltip key={domain}>
                    <TooltipTrigger render={button} />
                    <TooltipContent side="right" align="center">
                        {badge.count > 0
                            ? `${meta.title}（${badge.count}）`
                            : meta.title}
                    </TooltipContent>
                </Tooltip>
            )
        }

        return button
    }

    const renderNavItem = (
        groupLabel: string,
        item: WorkspaceNavGroup["items"][number],
    ) => {
        const isActive = isNavItemActive(
            navPathname,
            item.href,
            allHrefs,
            search,
        )
        const badgeCount = item.badge
            ? badgeCountFor(item.badge, counts)
            : undefined
        const badgeError = badgeErrorFor(item.badge, item.label)
        const badgeLabel = badgeAriaLabel(item.badge, badgeCount)

        return (
            <SidebarMenuItem key={`${groupLabel}-${item.href}`}>
                <SidebarMenuButton
                    id={`workspace-sidebar-nav-${toAutomationIdSegment(item.href)}`}
                    data-workspace-nav={toAutomationIdSegment(item.href)}
                    size="sm"
                    isActive={isActive}
                    render={<Link href={item.href} />}
                    onClick={() => {
                        if (isMobile) setOpenMobile(false)
                    }}
                >
                    <span>{item.label}</span>
                    {badgeError ||
                    (badgeCount !== undefined && badgeCount > 0) ? (
                        <span
                            data-workspace-nav-badge=""
                            aria-label={badgeError ?? badgeLabel}
                            title={badgeError ?? badgeLabel}
                            className={cn(
                                "num ml-auto flex h-4.5 min-w-4.5 items-center justify-center rounded-full px-1.5 text-2xs font-medium",
                                badgeError
                                    ? "bg-destructive text-destructive-foreground"
                                    : item.badge ===
                                        "background-job-active-count"
                                      ? "bg-info-soft text-info-soft-foreground"
                                      : item.badge === "todo-count" ||
                                          item.badge === "copy-unread-count"
                                        ? "bg-destructive-soft text-destructive-soft-foreground"
                                        : isActive
                                          ? "bg-card text-foreground shadow-2xs"
                                          : "bg-foreground/[0.06] text-muted-foreground",
                            )}
                        >
                            {badgeError ? "!" : badgeCount}
                        </span>
                    ) : null}
                </SidebarMenuButton>
            </SidebarMenuItem>
        )
    }

    const showGroupHeadings = domainGroups.length > 1

    return (
        <div className="flex h-full w-full min-w-0 flex-row overflow-hidden">
            {/* 左栏 60px：品牌徽标 + 7 大业务域主轨（常驻清晰、1 键直达） */}
            <aside
                aria-label="业务域主轨"
                className="flex h-full w-15 shrink-0 flex-col items-center border-r border-sidebar-border/60 bg-sidebar-rail"
            >
                <div className="flex h-workspace-topbar w-full shrink-0 items-center justify-center">
                    <Link
                        id="workspace-sidebar-brand-home"
                        href="/workspace"
                        title="福尚云 ERP · 我的工作台"
                        className="rounded-lg outline-none focus-visible:ring-2 focus-visible:ring-sidebar-ring"
                        onClick={() => {
                            if (availableDomains.includes("work")) {
                                setDomain("work")
                            }
                            if (isMobile) setOpenMobile(false)
                        }}
                    >
                        <WorkspaceBrandMark />
                    </Link>
                </div>

                {/* 仅在折叠窄轨模式显露的快捷搜索入口 */}
                <div className="hidden w-full justify-center pt-1 pb-1.5 group-data-[collapsible=icon]:flex">
                    <Tooltip>
                        <TooltipTrigger
                            render={
                                <button
                                    id="workspace-sidebar-rail-search"
                                    type="button"
                                    aria-label="快速跳转菜单"
                                    onClick={dispatchWorkspaceQuickSearch}
                                    className="flex size-9 cursor-pointer items-center justify-center rounded-lg text-sidebar-foreground/75 transition-colors outline-none hover:bg-foreground/[0.05] hover:text-foreground focus-visible:ring-2 focus-visible:ring-sidebar-ring"
                                />
                            }
                        >
                            <SearchIcon
                                className="size-4 stroke-[1.75]"
                                aria-hidden="true"
                            />
                        </TooltipTrigger>
                        <TooltipContent side="right" align="center">
                            快速跳转 (⌘K)
                        </TooltipContent>
                    </Tooltip>
                </div>

                <div className="no-scrollbar flex min-h-0 w-full flex-1 flex-col items-center gap-1 overflow-y-auto px-1.5 pt-1.5 pb-2">
                    {primaryDomains.map(renderRailButton)}
                </div>

                {bottomDomains.length > 0 ? (
                    <div className="flex w-full shrink-0 flex-col items-center gap-1 px-1.5 pt-1.5 pb-3">
                        <span
                            aria-hidden="true"
                            className="mb-1 h-px w-6 bg-sidebar-border/80"
                        />
                        {bottomDomains.map(renderRailButton)}
                    </div>
                ) : null}
            </aside>

            {/* 右栏 180px：当前业务域纯净子导航（折叠时平滑收起） */}
            <div className="flex h-full min-w-0 flex-1 flex-col bg-sidebar group-data-[collapsible=icon]:hidden">
                <div className="flex h-workspace-topbar shrink-0 items-center justify-between gap-1 pr-2 pl-4.5">
                    <span className="truncate text-body-compact font-semibold tracking-tight text-foreground">
                        福尚云 ERP
                    </span>
                    {showCollapseTrigger ? (
                        <SidebarTrigger
                            id="workspace-topbar-navigation-trigger"
                            title="折叠导航栏 (⌘B)"
                            className="size-7 shrink-0 text-muted-foreground/65 hover:bg-foreground/[0.05] hover:text-foreground"
                        />
                    ) : null}
                </div>

                <div className="shrink-0 px-2 pt-0.5 pb-2">
                    <WorkspaceQuickSearch />
                </div>

                <nav
                    ref={scrollRef}
                    aria-label={`${activeMeta.title}导航`}
                    onScroll={(event) =>
                        setScrolled(event.currentTarget.scrollTop > 2)
                    }
                    className={cn(
                        "no-scrollbar flex min-h-0 flex-1 flex-col overflow-y-auto border-t px-2 pt-1.5 pb-4 transition-colors duration-150",
                        scrolled
                            ? "border-sidebar-border/75"
                            : "border-transparent",
                    )}
                >
                    {profileStatus === "pending" ? (
                        <p className="px-2.5 py-2 text-xs text-sidebar-foreground/70">
                            加载菜单…
                        </p>
                    ) : profileStatus === "error" ? (
                        <p className="px-2.5 py-2 text-xs text-sidebar-foreground/70">
                            {getErrorMessage(
                                profileError,
                                "无法加载权限，菜单暂不可用。",
                            )}
                        </p>
                    ) : navGroups.length === 0 ? (
                        <p className="px-2.5 py-2 text-xs text-sidebar-foreground/70">
                            当前账号暂无可用菜单
                        </p>
                    ) : (
                        <>
                            <div className="px-2.5 pb-2 text-xs font-semibold tracking-tight text-foreground">
                                {activeMeta.title}
                            </div>

                            {domainGroups.map((group, index) => {
                                // 「工作台」域下的首项「我的工作台」直接置顶，不重复展示「工作」小标题
                                const omitHeading =
                                    !showGroupHeadings ||
                                    (currentDomain === "work" &&
                                        group.label === "工作")

                                return (
                                    <div
                                        key={group.label}
                                        className={cn(
                                            "flex flex-col",
                                            index > 0 && "mt-4",
                                        )}
                                    >
                                        {!omitHeading ? (
                                            <div className="mb-1 px-2.5 text-tiny font-medium tracking-wide text-muted-foreground/75">
                                                {group.label}
                                            </div>
                                        ) : null}
                                        <SidebarMenu className="gap-0.5">
                                            {group.items.map((item) =>
                                                renderNavItem(
                                                    group.label,
                                                    item,
                                                ),
                                            )}
                                        </SidebarMenu>
                                    </div>
                                )
                            })}
                        </>
                    )}
                </nav>
            </div>
        </div>
    )
}
