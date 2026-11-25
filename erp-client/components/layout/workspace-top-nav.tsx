"use client"

import type { ReactNode } from "react"
import { SidebarTrigger, useSidebar } from "@/components/ui/sidebar"
import { useWorkspaceShellBridge } from "@/lib/workspace-shell-bridge"
import { SystemFeedbackPanel } from "./system-feedback-panel"
import { WorkspaceAccountMenu } from "./workspace-account-menu"

export function WorkspaceTopNav({
    tabs,
    actions,
    inbox,
}: {
    tabs: ReactNode
    /** Global tools, such as search, stay outside the scrolling tab strip. */
    actions?: ReactNode
    /** Optional inbox slot override; falls back to the shell bridge adapter. */
    inbox?: ReactNode
}) {
    const { state, isMobile } = useSidebar()
    const { inboxComponent: InboxSlot } = useWorkspaceShellBridge()
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
                    className="relative z-10 shrink-0 text-muted-foreground hover:bg-shell-hover hover:text-foreground"
                />
            ) : null}
            {tabs}
            <nav
                aria-label="全局功能"
                className="relative z-10 flex min-w-0 shrink-0 items-center gap-1"
            >
                {actions ? <div className="mr-1 min-w-0">{actions}</div> : null}
                <SystemFeedbackPanel />
                {inbox ?? (InboxSlot ? <InboxSlot /> : null)}
                <WorkspaceAccountMenu />
            </nav>
        </header>
    )
}
