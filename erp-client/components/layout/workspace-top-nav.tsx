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
            className="flex h-14 min-w-0 shrink-0 items-center gap-2 border-b border-border bg-card px-workspace-inline md:gap-3 md:px-page-inline min-[75rem]:px-page-inline-lg"
        >
            <SidebarTrigger
                id="workspace-topbar-navigation-trigger"
                className="shrink-0 lg:hidden"
            />
            {tabs}
            <nav
                aria-label="全局功能"
                className="flex min-w-0 shrink-0 items-center gap-2"
            >
                {actions}
                <SystemFeedbackPanel />
                <InboxPopover />
                <WorkspaceAccountMenu />
            </nav>
        </header>
    )
}
