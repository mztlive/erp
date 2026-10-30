import type { Metadata } from "next"
import { SupplierDepositDetailPage } from "@/features/supplier-deposits/components/detail-page"
export const metadata: Metadata = { title: "预存款账户" }
export default function Page() {
    return <SupplierDepositDetailPage />
}
