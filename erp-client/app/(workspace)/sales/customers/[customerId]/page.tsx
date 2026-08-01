import type { Metadata } from "next"
import { Suspense } from "react"

import { CustomerDetailPage } from "@/features/customers/customer-detail-page"

export const metadata: Metadata = {
  title: "客户对象中心",
}

export default async function Page({
  params,
  searchParams,
}: {
  params: Promise<{ customerId: string }>
  searchParams: Promise<{ section?: string }>
}) {
  const { customerId } = await params
  const { section } = await searchParams
  return (
    <Suspense
      fallback={
        <div className="p-5 text-sm text-muted-foreground">正在加载客户…</div>
      }
    >
      <CustomerDetailPage
        key={`${customerId}-${section ?? "overview"}`}
        customerId={customerId}
        section={section}
      />
    </Suspense>
  )
}
