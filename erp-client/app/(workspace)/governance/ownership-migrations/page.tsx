import type { Metadata } from "next"
import { Suspense } from "react"

import { OwnershipMigrationPage } from "@/features/ownership-migration/ownership-migration-page"

export const metadata: Metadata = {
  title: "主责迁移批次",
}

function OwnershipMigrationFallback() {
  return (
    <div className="mx-auto flex w-full max-w-shell flex-col gap-4 p-4 md:p-5">
      <div className="h-10 w-56 animate-pulse rounded-lg bg-muted" />
      <div className="h-16 animate-pulse rounded-xl bg-muted" />
      <div className="h-24 animate-pulse rounded-2xl bg-muted" />
      <div className="h-72 animate-pulse rounded-2xl bg-muted" />
    </div>
  )
}

/**
 * SPA 壳：URL 由客户端恢复 mall / customer / status / batchId / stage / panel / role 等。
 * 业务数据不在服务端 fetch。
 */
export default function Page() {
  return (
    <Suspense fallback={<OwnershipMigrationFallback />}>
      <OwnershipMigrationPage />
    </Suspense>
  )
}
