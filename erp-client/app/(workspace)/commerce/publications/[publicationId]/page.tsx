import type { Metadata } from "next"
import { Suspense } from "react"

import { PublicationCenterPage } from "@/features/product-publications/publication-center-page"

export const metadata: Metadata = {
  title: "商品发布",
}

function CenterFallback() {
  return (
    <div className="mx-auto flex w-full max-w-shell flex-col gap-4 p-4 md:p-5">
      <div className="h-10 w-56 animate-pulse rounded-lg bg-muted" />
      <div className="h-24 animate-pulse rounded-xl bg-muted" />
      <div className="h-40 animate-pulse rounded-2xl bg-muted" />
      <div className="grid gap-4 xl:grid-cols-[1fr_20rem]">
        <div className="h-96 animate-pulse rounded-2xl bg-muted" />
        <div className="h-48 animate-pulse rounded-2xl bg-muted" />
      </div>
    </div>
  )
}

/**
 * SPA 壳：publicationId 为稳定页签身份；section / revision 由客户端 URL 恢复。
 * 业务数据不在服务端 fetch。
 */
export default async function Page({
  params,
}: {
  params: Promise<{ publicationId: string }>
}) {
  const { publicationId } = await params
  return (
    <Suspense fallback={<CenterFallback />}>
      <PublicationCenterPage publicationId={publicationId} />
    </Suspense>
  )
}
