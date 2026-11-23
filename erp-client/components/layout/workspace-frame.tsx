"use client"

import * as React from "react"
import { usePathname, useRouter, useSearchParams } from "next/navigation"
import { useQueryClient } from "@tanstack/react-query"
import {
    applyWorkspaceViewportDensity,
    handleWorkspaceCloseShortcut,
    interceptWorkspaceLink,
    resolveWorkspaceViewportDensity,
    tabHref,
    workspaceHost,
    type WorkspaceViewportDensity,
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

    const lastReportedRef = React.useRef<string>("")

    const report = React.useCallback(() => {
        const heading = document.querySelector("h1")?.textContent?.trim()
        const number = document
            .querySelector("[data-workspace-document-number]")
            ?.getAttribute("data-workspace-document-number")
            ?.trim()
        const state = workspaceFormState()
        const baseTitle =
            heading || document.title.replace(/ · 福尚云 ERP$/, "")
        const title =
            number && (!baseTitle || !baseTitle.includes(number))
                ? [baseTitle, number].filter(Boolean).join(" · ")
                : baseTitle
        const payload = {
            href: tabHref(new URL(window.location.href)),
            title,
            dirty: state.dirty,
            busy: state.busy || queryClient.isMutating() > 0,
        }
        const signature = `${payload.href}\0${payload.title}\0${payload.dirty}\0${payload.busy}`
        if (signature === lastReportedRef.current) return
        lastReportedRef.current = signature
        workspaceHost()?.report(window, payload)
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
        const handleQuickSearchShortcut = (event: KeyboardEvent) => {
            if (
                event.defaultPrevented ||
                event.isComposing ||
                event.altKey ||
                event.shiftKey
            )
                return
            if (
                event.key.toLowerCase() === "k" &&
                (event.metaKey || event.ctrlKey)
            ) {
                event.preventDefault()
                workspaceHost()?.openQuickSearch?.()
            }
        }
        document.addEventListener("click", interceptWorkspaceLink, true)
        window.addEventListener("keydown", handleWorkspaceCloseShortcut, true)
        window.addEventListener("keydown", handleQuickSearchShortcut, true)
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
            window.removeEventListener(
                "keydown",
                handleQuickSearchShortcut,
                true,
            )
            window.removeEventListener("erp-workspace-form-state", report)
            window.removeEventListener("hashchange", report)
            delete window.erpWorkspaceGo
        }
    }, [queryClient, report, router])

    React.useEffect(() => {
        const readInitialDensity = (): WorkspaceViewportDensity => {
            const hostDensity = workspaceHost()?.getViewportDensity?.()
            if (hostDensity) return hostDensity
            try {
                const inherited =
                    window.parent?.document?.documentElement?.getAttribute(
                        "data-viewport-density",
                    )
                if (
                    inherited === "comfortable" ||
                    inherited === "compact" ||
                    inherited === "dense"
                ) {
                    return inherited
                }
            } catch {
                // Ignore cross-origin parent inspection errors.
            }
            return resolveWorkspaceViewportDensity(
                window.innerHeight,
                window.innerWidth,
            )
        }
        applyWorkspaceViewportDensity(document, readInitialDensity())

        window.erpWorkspaceSetDensity = (density) => {
            applyWorkspaceViewportDensity(document, density)
        }
        window.erpWorkspaceFocus = () => {
            const current = document.activeElement
            if (
                current instanceof HTMLElement &&
                current !== document.body &&
                current !== document.documentElement
            ) {
                window.focus()
                return
            }
            window.focus()
            const main = document.getElementById("main-content")
            if (main instanceof HTMLElement) {
                main.focus({ preventScroll: true })
            }
        }
        return () => {
            delete window.erpWorkspaceSetDensity
            delete window.erpWorkspaceFocus
        }
    }, [])

    return (
        <WorkspaceTabContext.Provider value={true}>
            <main
                id="main-content"
                tabIndex={-1}
                aria-label="主工作区"
                className="@container/workspace flex h-svh min-w-0 flex-col overflow-auto outline-none"
            >
                <WorkspaceRefreshNotice />
                {children}
            </main>
        </WorkspaceTabContext.Provider>
    )
}
