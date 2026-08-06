import { Suspense } from "react"

/**
 * 认证区布局：不挂工作台壳，仅提供客户端 Suspense（searchParams）。
 */
export default function AuthLayout({
  children,
}: {
  children: React.ReactNode
}) {
  return (
    <Suspense
      fallback={
        <div className="flex min-h-svh items-center justify-center bg-background text-sm text-muted-foreground">
          加载中…
        </div>
      }
    >
      {children}
    </Suspense>
  )
}
