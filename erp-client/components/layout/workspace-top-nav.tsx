"use client"

import type { ReactNode } from "react"
import { SystemFeedbackPanel } from "./system-feedback-panel"

import { InboxPopover } from "@/features/notifications/components/inbox-popover"
import { SidebarTrigger, useSidebar } from "@/components/ui/sidebar"
import { WorkspaceAccountMenu } from "./workspace-account-menu"

export function WorkspaceTopNav({
    tabs,
    actions,
}: {
    tabs: ReactNode
    /** Global tools, such as search, stay outside the scrolling tab strip. */
    actions?: ReactNode
}) {
    const { state, isMobile } = useSidebar()
    const showSidebarTrigger = isMobile || state === "collapsed"

    return (
        <header
            data-slot="workspace-top-nav"
            className="relative flex h-workspace-topbar min-w-0 shrink-0 items-center justify-between gap-2 border-b border-sidebar-border bg-card px-2.5 md:pr-4 md:pl-3"
        >
            {showSidebarTrigger ? (
                <SidebarTrigger
                    id="workspace-topbar-navigation-trigger"
                    title="展开导航栏 (⌘B)"
                    className="relative z-10 shrink-0 text-muted-foreground hover:bg-foreground/[0.05] hover:text-foreground"
                />
            ) : null}
            {tabs}
            <nav
                aria-label="全局功能"
                className="relative z-10 flex min-w-0 shrink-0 items-center gap-1"
            >
                {actions ? <div className="mr-1 min-w-0">{actions}</div> : null}
                <SystemFeedbackPanel />
                <InboxPopover />
                <WorkspaceAccountMenu />
            </nav>
        </header>
    )
}
