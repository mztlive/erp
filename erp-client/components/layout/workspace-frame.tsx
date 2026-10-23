"use client"

import * as React from "react"
import { usePathname, useRouter, useSearchParams } from "next/navigation"
import { useQueryClient } from "@tanstack/react-query"
import {
    handleWorkspaceCloseShortcut,
    interceptWorkspaceLink,
    tabHref,
    workspaceHost,
} from "@/lib/workspace-tabs"
import { workspaceFormState } from "@/lib/workspace-form-guard"
import { subscribeNavDelivery } from "@/lib/nav-delivery"
import { WorkspaceTabContext } from "./workspace-tab-context"
import { WorkspaceRefreshNotice } from "./workspace-refresh-notice"

/** Each retained document owns its router, portals, focus, scroll and form instances. */
export function WorkspaceFrame({ children }: { children: React.ReactNode }) {
    const pathname = usePathname()
    const search = useSearchParams()
    const router = useRouter()
    const queryClient = useQueryClient()

    const report = React.useCallback(() => {
        const heading = document.querySelector("h1")?.textContent?.trim()
        const number = document
            .querySelector("[data-workspace-document-number]")
            ?.getAttribute("data-workspace-document-number")
        const state = workspaceFormState()
        workspaceHost()?.report(window, {
            href: tabHref(new URL(window.location.href)),
            title: [
                heading || document.title.replace(/ · 福尚云 ERP$/, ""),
                number,
            ]
                .filter(Boolean)
                .join(" · "),
            dirty: state.dirty,
            busy: state.busy || queryClient.isMutating() > 0,
        })
    }, [queryClient])

    React.useEffect(() => {
        report()
    }, [pathname, search, report])

    React.useEffect(() => {
        let frame = 0
        const observer = new MutationObserver(() => {
            cancelAnimationFrame(frame)
            frame = requestAnimationFrame(report)
        })
        observer.observe(document.body, {
            subtree: true,
            childList: true,
            characterData: true,
        })
        observer.observe(document.head, {
            subtree: true,
            childList: true,
            characterData: true,
        })
        document.addEventListener("click", interceptWorkspaceLink, true)
        window.addEventListener("keydown", handleWorkspaceCloseShortcut, true)
        window.addEventListener("erp-workspace-form-state", report)
        window.addEventListener("hashchange", report)

        window.erpWorkspaceGo = (href) =>
            router.replace(href, { scroll: false })
        const unsubscribe = queryClient.getMutationCache().subscribe(report)
        const unsubscribeDelivery = subscribeNavDelivery((request) =>
            workspaceHost()?.deliver(window, request),
        )
        report()
        return () => {
            cancelAnimationFrame(frame)
            observer.disconnect()
            unsubscribe()
            unsubscribeDelivery()
            document.removeEventListener("click", interceptWorkspaceLink, true)
            window.removeEventListener(
                "keydown",
                handleWorkspaceCloseShortcut,
                true,
            )
            window.removeEventListener("erp-workspace-form-state", report)
            window.removeEventListener("hashchange", report)
            delete window.erpWorkspaceGo
        }
    }, [queryClient, report, router])

    return (
        <WorkspaceTabContext.Provider value={true}>
            <main
                id="main-content"
                aria-label="主工作区"
                className="flex h-svh min-w-0 flex-col overflow-auto"
            >
                <WorkspaceRefreshNotice />
                {children}
            </main>
        </WorkspaceTabContext.Provider>
    )
}
