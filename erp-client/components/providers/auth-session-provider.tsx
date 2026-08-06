"use client"

import * as React from "react"
import { usePathname, useRouter, useSearchParams } from "next/navigation"
import { useQueryClient } from "@tanstack/react-query"

import {
  clearToken,
  isAuthenticated,
  onUnauthorized,
} from "@/lib/api/session"

/**
 * 全局鉴权会话：
 * - 注册 401 回调：清缓存并跳转 /login（带 returnTo）
 * - 不在此组件内做路由门禁（门禁见 RequireAuth）
 */
export function AuthSessionProvider({
  children,
}: {
  children: React.ReactNode
}) {
  const router = useRouter()
  const pathname = usePathname()
  const searchParams = useSearchParams()
  const queryClient = useQueryClient()

  React.useEffect(() => {
    return onUnauthorized(() => {
      queryClient.clear()
      // 登录页本身 401 不应死循环（login 一般不带 token；防御）
      if (pathname === "/login" || pathname.startsWith("/login/")) {
        return
      }
      const search = searchParams.toString()
      const current = `${pathname}${search ? `?${search}` : ""}`
      const returnTo = encodeURIComponent(current || "/workspace")
      router.replace(`/login?returnTo=${returnTo}`)
    })
  }, [pathname, queryClient, router, searchParams])

  return children
}

/**
 * 工作区门禁：无 token 时跳转登录，不渲染业务子树（避免无鉴权刷接口）。
 */
export function RequireAuth({ children }: { children: React.ReactNode }) {
  const router = useRouter()
  const pathname = usePathname()
  const searchParams = useSearchParams()
  const [allowed, setAllowed] = React.useState(false)

  React.useEffect(() => {
    if (isAuthenticated()) {
      setAllowed(true)
      return
    }
    setAllowed(false)
    const search = searchParams.toString()
    const current = `${pathname}${search ? `?${search}` : ""}`
    const returnTo = encodeURIComponent(current || "/workspace")
    router.replace(`/login?returnTo=${returnTo}`)
  }, [pathname, router, searchParams])

  // 再订阅一次 storage/401 后的本地变化：token 被 clear 后立刻挡回登录
  React.useEffect(() => {
    const onStorage = (event: StorageEvent) => {
      if (event.key === "erp.token" && !event.newValue) {
        setAllowed(false)
        router.replace("/login")
      }
    }
    window.addEventListener("storage", onStorage)
    return () => window.removeEventListener("storage", onStorage)
  }, [router])

  React.useEffect(() => {
    return onUnauthorized(() => {
      setAllowed(false)
    })
  }, [])

  if (!allowed) {
    return (
      <div className="flex min-h-svh items-center justify-center bg-background text-sm text-muted-foreground">
        正在验证登录状态…
      </div>
    )
  }

  return children
}

/**
 * 已登录访问登录页时：直接进工作台。
 */
export function RedirectIfAuthenticated({
  children,
}: {
  children: React.ReactNode
}) {
  const router = useRouter()
  const searchParams = useSearchParams()
  const [show, setShow] = React.useState(false)

  React.useEffect(() => {
    if (isAuthenticated()) {
      const raw = searchParams.get("returnTo")
      const target =
        raw && raw.startsWith("/") && !raw.startsWith("//")
          ? raw
          : "/workspace"
      router.replace(target)
      return
    }
    setShow(true)
  }, [router, searchParams])

  if (!show) {
    return (
      <div className="flex min-h-svh items-center justify-center bg-background text-sm text-muted-foreground">
        加载中…
      </div>
    )
  }

  return children
}

/** 登出并跳转登录（侧栏等调用）。可选清空 TanStack Query 缓存。 */
export function logoutAndRedirect(
  router: { replace: (href: string) => void },
  queryClient?: { clear: () => void }
): void {
  clearToken()
  queryClient?.clear()
  router.replace("/login")
}
