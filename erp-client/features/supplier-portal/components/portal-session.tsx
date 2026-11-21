"use client"
import {
    createContext,
    useContext,
    useEffect,
    useState,
    type ReactNode,
} from "react"
import { usePathname, useRouter, useSearchParams } from "next/navigation"
import { useQueryClient } from "@tanstack/react-query"
import { NavDeliveryOverlay } from "@/components/layout/nav-delivery-overlay"
import { clearToken, isAuthenticated, onUnauthorized } from "@/lib/api/session"
import { portalKeys, usePortalSession } from "../hooks/queries"
import type { PortalProfile } from "../types"
import { PortalError } from "./surface"
import { PortalShell } from "./portal-shell"

const PortalContext = createContext<PortalProfile | null>(null)
export const usePortalProfile = () => useContext(PortalContext)
/** 门户会话与后台会话分别存取，失效后立即停止渲染业务子树。 */
export function PortalSession({ children }: { children: ReactNode }) {
    const router = useRouter()
    const pathname = usePathname()
    const search = useSearchParams()
    const client = useQueryClient()
    const [hasToken, setHasToken] = useState(false)
    const profile = usePortalSession(hasToken)
    useEffect(() => {
        const signedIn = isAuthenticated("supplier-portal")
        setHasToken(signedIn)
        if (!signedIn)
            router.replace(
                `/supplier-portal/login?returnTo=${encodeURIComponent(pathname + (search.size ? `?${search}` : ""))}`,
            )
    }, [pathname, router, search])
    useEffect(() => {
        const expire = () => {
            setHasToken(false)
            void client.cancelQueries({ queryKey: portalKeys.all })
            client.removeQueries({ queryKey: portalKeys.all })
            router.replace("/supplier-portal/login")
        }
        const unsubscribe = onUnauthorized(expire, "supplier-portal")
        const storage = (event: StorageEvent) => {
            if (event.key === "erp.supplier-portal.token") expire()
        }
        window.addEventListener("storage", storage)
        return () => {
            unsubscribe()
            window.removeEventListener("storage", storage)
        }
    }, [client, router])
    if (!hasToken || profile.isPending)
        return (
            <div className="p-8 text-body-compact text-muted-foreground">
                正在验证登录状态…
            </div>
        )
    if (profile.error || !profile.data)
        return (
            <div className="p-8">
                <PortalError
                    error={profile.error ?? new Error("登录资料暂不可用")}
                    retry={() => void profile.refetch()}
                />
            </div>
        )
    return (
        <PortalContext.Provider value={profile.data}>
            <PortalShell
                profile={profile.data}
                onLogout={() => {
                    clearToken("supplier-portal")
                    void client.cancelQueries({ queryKey: portalKeys.all })
                    client.removeQueries({ queryKey: portalKeys.all })
                    setHasToken(false)
                    router.replace("/supplier-portal/login")
                }}
            >
                {children}
                <NavDeliveryOverlay />
            </PortalShell>
        </PortalContext.Provider>
    )
}
