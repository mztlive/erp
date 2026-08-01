import type { Metadata } from "next"

import { SalesOrdersListPage } from "@/features/sales-orders/sales-orders-list-page"

export const metadata: Metadata = {
  title: "销售单",
}

export default function SalesOrdersPage() {
  return <SalesOrdersListPage />
}
