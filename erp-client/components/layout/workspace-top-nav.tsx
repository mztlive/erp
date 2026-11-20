"use client"

import type { ReactNode } from "react"
import { SystemFeedbackPanel } from "./system-feedback-panel"

import { InboxPopover } from "@/features/notifications/components/inbox-popover"
import { SidebarTrigger } from "@/components/ui/sidebar"
import { WorkspaceAccountMenu } from "./workspace-account-menu"

export function WorkspaceTopNav({
    tabs,
    actions,
}: {
    tabs: ReactNode
    /** Global tools, such as search, stay outside the scrolling tab strip. */
    actions?: ReactNode
}) {
    return (
        <header
            data-slot="workspace-top-nav"
            className="relative flex h-workspace-topbar min-w-0 shrink-0 items-center justify-between gap-2 bg-muted/85 px-2.5 after:pointer-events-none after:absolute after:inset-x-0 after:bottom-0 after:z-0 after:h-px after:bg-border/80 md:pr-4 md:pl-3"
        >
            <SidebarTrigger
                id="workspace-topbar-navigation-trigger"
                title="折叠或展开导航栏 (⌘B)"
                className="relative z-10 shrink-0 text-muted-foreground hover:bg-card/75 hover:text-foreground"
            />
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
