import { redirect } from "next/navigation"

import { resolveMasterDataRoleDefault } from "@/features/master-data/data"

export default async function MasterDataIndexPage({
  searchParams,
}: {
  searchParams: Promise<{ demoRole?: string }>
}) {
  const { demoRole } = await searchParams
  redirect(`/master-data/${resolveMasterDataRoleDefault(demoRole)}`)
}
