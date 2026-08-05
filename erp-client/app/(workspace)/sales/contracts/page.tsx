import type { Metadata } from "next"

import { ContractsListPage } from "@/features/contracts/contracts-list-page"

export const metadata: Metadata = {
  title: "合同",
}

export default async function ContractsPage({
  searchParams,
}: {
  searchParams: Promise<{
    q?: string
    search?: string
    customerId?: string
    metric?: string
    page?: string
    pageSize?: string
  }>
}) {
  const params = await searchParams
  // 旧链接兼容：search 别名只读不写回。
  const initialSearch = params.q ?? params.search ?? ""
  const initialCustomerId = params.customerId ?? ""
  const initialMetric =
    params.metric === "effective" ||
    params.metric === "expiring_30d" ||
    params.metric === "expired" ||
    params.metric === "terminated"
      ? params.metric
      : "all"
  const initialPage = Math.max(
    1,
    Number.parseInt(params.page ?? "1", 10) || 1
  )
  const initialPageSize = Math.min(
    100,
    Math.max(1, Number.parseInt(params.pageSize ?? "20", 10) || 20)
  )
  return (
    <ContractsListPage
      initialSearch={initialSearch}
      initialCustomerId={initialCustomerId}
      initialMetric={initialMetric}
      initialPage={initialPage}
      initialPageSize={initialPageSize}
    />
  )
}
