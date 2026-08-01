import type { Metadata } from "next"

import { MasterDataPage } from "@/features/master-data/master-data-page"

export const metadata: Metadata = {
  title: "主数据",
}

export default async function Page({
  params,
}: {
  params: Promise<{ resource: string }>
}) {
  const { resource } = await params
  return <MasterDataPage resource={resource} />
}
