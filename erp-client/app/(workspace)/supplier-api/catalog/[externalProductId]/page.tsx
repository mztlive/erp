import type { Metadata } from "next"
import { Suspense } from "react"

import { ExternalProductCenterPage } from "@/features/external-product-supply/external-product-center-page"

export const metadata: Metadata = {
  title: "外部商品与供给中心",
}

export default async function Page({
  params,
}: {
  params: Promise<{ externalProductId: string }>
}) {
  const { externalProductId } = await params
  return (
    <Suspense
      fallback={
        <div className="mx-auto flex w-full max-w-shell flex-col gap-4 p-4 md:p-5">
          <div className="h-10 w-56 animate-pulse rounded-lg bg-muted" />
          <div className="h-40 animate-pulse rounded-2xl bg-muted" />
        </div>
      }
    >
      <ExternalProductCenterPage externalProductId={externalProductId} />
    </Suspense>
  )
}
