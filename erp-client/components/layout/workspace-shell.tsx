"use client"

import * as React from "react"

import { ErpAppShell } from "@/components/business"
import { NavDeliveryOverlay } from "@/components/layout/nav-delivery-overlay"
import { WorkspaceSidebarProvider } from "@/components/layout/workspace-sidebar-context"
import { WorkspaceSidebarNav } from "@/components/layout/workspace-sidebar-nav"
import { WorkspaceTopNav } from "@/components/layout/workspace-top-nav"
import { isWorkspaceFrame } from "@/lib/workspace-tabs"
import { WorkspaceFrame } from "./workspace-frame"
import { WorkspaceTabs } from "./workspace-tabs"

export function WorkspaceShell({ children }: { children: React.ReactNode }) {
    const [embedded, setEmbedded] = React.useState<boolean | null>(null)
    React.useEffect(() => {
        setEmbedded(isWorkspaceFrame())
    }, [])
    if (embedded === null) return <div className="min-h-svh bg-background" />
    if (embedded) return <WorkspaceFrame>{children}</WorkspaceFrame>
    return (
        <WorkspaceTabs>
            {(tabbar, panels) => (
                <WorkspaceSidebarProvider>
                    <ErpAppShell
                        className="h-svh overflow-hidden"
                        contentLabel="主工作区"
                        sidebarCollapsible="icon"
                        showSidebarRail={true}
                        sidebarContentClassName="gap-0 overflow-hidden p-0"
                        sidebarContent={<WorkspaceSidebarNav />}
                        topbar={<WorkspaceTopNav tabs={tabbar} />}
                    >
                        {panels}
                        <NavDeliveryOverlay />
                    </ErpAppShell>
                </WorkspaceSidebarProvider>
            )}
        </WorkspaceTabs>
    )
}
