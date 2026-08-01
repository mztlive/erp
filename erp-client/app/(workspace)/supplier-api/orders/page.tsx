import type { Metadata } from "next"
import { Suspense } from "react"

import { SupplierOrdersListPage } from "@/features/supplier-orders/supplier-orders-list-page"

export const metadata: Metadata = {
  title: "供应商订单",
}

function OrdersFallback() {
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
 * SPA 壳：URL 恢复 view / q / supplierId / 三轨状态 / paidFrom-To / page / preview。
 * 业务数据不在服务端 fetch。
 */
export default function Page() {
  return (
    <Suspense fallback={<OrdersFallback />}>
      <SupplierOrdersListPage />
    </Suspense>
  )
}
