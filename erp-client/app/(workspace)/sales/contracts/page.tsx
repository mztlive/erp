import type { Metadata } from "next"

import { ContractsListPage } from "@/features/contracts/contracts-list-page"

export const metadata: Metadata = {
    title: "合同",
}

export default function ContractsPage() {
    return <ContractsListPage />
}
