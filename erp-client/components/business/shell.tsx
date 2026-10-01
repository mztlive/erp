"use client"

import * as React from "react"
import {
    Sidebar,
    SidebarContent,
    SidebarFooter,
    SidebarHeader,
    SidebarInset,
    SidebarProvider,
    SidebarRail,
} from "@/components/ui/sidebar"
type SidebarProps = React.ComponentProps<typeof Sidebar>

export interface ErpAppShellProps {
    children: React.ReactNode
    sidebarContent: React.ReactNode
    sidebarHeader?: React.ReactNode
    sidebarFooter?: React.ReactNode
    topbar?: React.ReactNode
    maintenanceBanner?: React.ReactNode
    defaultSidebarOpen?: boolean
    sidebarOpen?: boolean
    onSidebarOpenChange?: (open: boolean) => void
    sidebarCollapsible?: SidebarProps["collapsible"]
    sidebarSide?: SidebarProps["side"]
    sidebarVariant?: SidebarProps["variant"]
    showSidebarRail?: boolean
    contentId?: string
    contentLabel?: string
    className?: string
}

function ErpAppShell({
    children,
    sidebarContent,
    sidebarHeader,
    sidebarFooter,
    topbar,
    maintenanceBanner,
    defaultSidebarOpen = true,
    sidebarOpen,
    onSidebarOpenChange,
    sidebarCollapsible = "none",
    sidebarSide = "left",
    sidebarVariant = "sidebar",
    showSidebarRail = false,
    contentId = "main-content",
    contentLabel,
    className,
}: ErpAppShellProps) {
    return (
        <SidebarProvider
            defaultOpen={defaultSidebarOpen}
            open={sidebarOpen}
            onOpenChange={onSidebarOpenChange}
            className={className}
        >
            <Sidebar
                collapsible={sidebarCollapsible}
                side={sidebarSide}
                variant={sidebarVariant}
            >
                {sidebarHeader ? (
                    <SidebarHeader>{sidebarHeader}</SidebarHeader>
                ) : null}
                <SidebarContent>{sidebarContent}</SidebarContent>
                {sidebarFooter ? (
                    <SidebarFooter>{sidebarFooter}</SidebarFooter>
                ) : null}
                {showSidebarRail && sidebarCollapsible !== "none" ? (
                    <SidebarRail />
                ) : null}
            </Sidebar>
            <SidebarInset
                id={contentId}
                aria-label={contentLabel}
                className="min-h-0 min-w-0 overflow-hidden"
            >
                {topbar}
                {maintenanceBanner}
                <div
                    data-slot="erp-shell-content"
                    className="flex min-h-0 flex-1 flex-col overflow-auto"
                >
                    {children}
                </div>
            </SidebarInset>
        </SidebarProvider>
    )
}

export { ErpAppShell }
