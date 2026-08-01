import type { Metadata } from "next"

import { ContractsListPage } from "@/features/contracts/contracts-list-page"

export const metadata: Metadata = {
  title: "合同",
}

export default async function ContractsPage({
  searchParams,
}: {
  searchParams: Promise<{ q?: string; search?: string; customerId?: string }>
}) {
  const params = await searchParams
  const initialSearch = params.q ?? params.search ?? ""
  const initialCustomerId = params.customerId ?? ""
  return (
    <ContractsListPage
      key={`${initialSearch}-${initialCustomerId}`}
      initialSearch={initialSearch}
      initialCustomerId={initialCustomerId}
    />
  )
}
