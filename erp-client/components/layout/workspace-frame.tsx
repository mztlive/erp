"use client"

import * as React from "react"
import { usePathname, useRouter, useSearchParams } from "next/navigation"
import { focusManager, useQueryClient } from "@tanstack/react-query"
import {
    handleWorkspaceCloseShortcut,
    interceptWorkspaceLink,
    tabHref,
    workspaceHost,
} from "@/lib/workspace-tabs"
import { workspaceFormState } from "@/lib/workspace-form-guard"
import { subscribeNavDelivery } from "@/lib/nav-delivery"
import { WorkspaceTabContext } from "./workspace-tab-context"

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

        const activate = (active: boolean) => {
            focusManager.setFocused(
                active && document.visibilityState === "visible",
            )
            if (
                active &&
                !workspaceFormState().dirty &&
                !workspaceFormState().busy &&
                queryClient.isMutating() === 0
            ) {
                void queryClient.refetchQueries({ type: "active", stale: true })
            }
        }
        const visibility = () =>
            activate(
                window.frameElement?.getAttribute("data-workspace-active") ===
                    "true",
            )
        window.erpWorkspaceActivate = activate
        window.erpWorkspaceGo = (href) =>
            router.replace(href, { scroll: false })
        window.erpWorkspaceInvalidate = () => {
            void queryClient.invalidateQueries({ refetchType: "none" })
            visibility()
        }
        document.addEventListener("visibilitychange", visibility)
        visibility()
        const unsubscribe = queryClient
            .getMutationCache()
            .subscribe((event) => {
                report()
                if (
                    event.type === "updated" &&
                    event.action.type === "success"
                ) {
                    workspaceHost()?.changed(window)
                }
            })
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
            document.removeEventListener("visibilitychange", visibility)
            window.removeEventListener("erp-workspace-form-state", report)
            window.removeEventListener("hashchange", report)
            delete window.erpWorkspaceActivate
            delete window.erpWorkspaceGo
            delete window.erpWorkspaceInvalidate
            focusManager.setFocused(undefined)
        }
    }, [queryClient, report, router])

    return (
        <WorkspaceTabContext.Provider value={true}>
            <main
                id="main-content"
                aria-label="主工作区"
                className="flex h-svh min-w-0 flex-col overflow-auto"
            >
                {children}
            </main>
        </WorkspaceTabContext.Provider>
    )
}
