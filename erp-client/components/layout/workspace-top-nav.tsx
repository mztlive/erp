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
            className="flex h-workspace-topbar min-w-0 shrink-0 items-center justify-between gap-3 border-b border-border bg-card px-3 md:pr-5 md:pl-4"
        >
            <SidebarTrigger
                id="workspace-topbar-navigation-trigger"
                className="shrink-0 lg:hidden"
            />
            {tabs}
            <nav
                aria-label="全局功能"
                className="flex min-w-0 shrink-0 items-center gap-1"
            >
                {actions ? <div className="mr-1 min-w-0">{actions}</div> : null}
                <SystemFeedbackPanel />
                <InboxPopover />
                <WorkspaceAccountMenu />
            </nav>
        </header>
    )
}
