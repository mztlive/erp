import type { Metadata } from "next"
import { Suspense } from "react"

import { MasterDataCenterPage } from "@/features/master-data/master-data-center-page"

export const metadata: Metadata = {
  title: "基础资料详情",
}

export default async function Page({
  params,
  searchParams,
}: {
  params: Promise<{ resource: string; stableId: string }>
  searchParams: Promise<{
    section?: string
    revision?: string
    sourceSupplierProductId?: string
    returnTo?: string
  }>
}) {
  const { resource, stableId } = await params
  const { section, sourceSupplierProductId, returnTo } = await searchParams
  return (
    <Suspense
      fallback={
        <div className="p-5 text-sm text-muted-foreground">
          正在加载基础资料对象…
        </div>
      }
    >
      <MasterDataCenterPage
        key={`${resource}-${stableId}-${section ?? "overview"}`}
        resource={resource}
        stableId={stableId}
        section={section}
        sourceSupplierProductId={sourceSupplierProductId}
        returnTo={returnTo}
      />
    </Suspense>
  )
}
