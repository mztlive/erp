import type { Metadata } from "next"
import { SupplierDepositsPage } from "@/features/supplier-deposits/components/accounts-page"
export const metadata: Metadata = { title: "供应商预存款" }
export default function Page() {
    return <SupplierDepositsPage />
}
