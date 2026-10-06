"use client"
import {
    createContext,
    useContext,
    useEffect,
    useState,
    type ReactNode,
} from "react"
import Link from "next/link"
import { usePathname, useRouter, useSearchParams } from "next/navigation"
import { useQueryClient } from "@tanstack/react-query"
import { Button } from "@/components/ui/button"
import { clearToken, isAuthenticated, onUnauthorized } from "@/lib/api/session"
import { portalKeys, usePortalSession } from "../hooks/queries"
import type { PortalProfile } from "../types"
import { PortalError } from "./surface"

const PortalContext = createContext<PortalProfile | null>(null)
export const usePortalProfile = () => useContext(PortalContext)
const navigation = [
    ["offerings", "我的供给"],
    ["quotes", "已有商品报价"],
    ["new-products", "新品提报"],
    ["applications", "我的申请"],
    ["cooperation", "合作资料"],
    ["account", "账号设置"],
] as const
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
            <div className="p-8 text-sm text-muted-foreground">
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
            <div className="min-h-svh bg-background">
                <header className="border-b bg-card px-4 py-4 md:px-8">
                    <div className="mx-auto flex max-w-7xl flex-wrap items-center justify-between gap-3">
                        <div>
                            <p className="font-semibold">供应商门户</p>
                            <p className="text-xs text-muted-foreground">
                                {profile.data.supplier_name ?? "供应合作"} ·{" "}
                                {profile.data.name} ·{" "}
                                {profile.data.role === "read_only"
                                    ? "只读人员"
                                    : "供给维护员"}
                            </p>
                        </div>
                        <Button
                            id="supplier-portal-logout"
                            variant="outline"
                            size="sm"
                            onClick={() => {
                                clearToken("supplier-portal")
                                void client.cancelQueries({
                                    queryKey: portalKeys.all,
                                })
                                client.removeQueries({
                                    queryKey: portalKeys.all,
                                })
                                setHasToken(false)
                                router.replace("/supplier-portal/login")
                            }}
                        >
                            退出登录
                        </Button>
                    </div>
                    <nav
                        aria-label="供应商门户导航"
                        className="mx-auto mt-4 flex max-w-7xl flex-wrap gap-2"
                    >
                        {navigation.map(([path, label]) => (
                            <Link
                                id={`supplier-portal-nav-${path}`}
                                key={path}
                                href={`/supplier-portal/${path}`}
                                className={`rounded-md px-3 py-2 text-sm ${pathname.startsWith(`/supplier-portal/${path}`) ? "bg-primary text-primary-foreground" : "hover:bg-muted"}`}
                            >
                                {label}
                            </Link>
                        ))}
                    </nav>
                </header>
                {profile.data.role === "read_only" && (
                    <p className="mx-auto max-w-7xl px-4 pt-4 text-sm text-muted-foreground">
                        当前账号可查看资料；提交申请和更新可供请联系本供应商的供给维护员。
                    </p>
                )}
                <main>{children}</main>
            </div>
        </PortalContext.Provider>
    )
}
