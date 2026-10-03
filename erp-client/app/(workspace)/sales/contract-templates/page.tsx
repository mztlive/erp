import type { Metadata } from "next"
import { ContractTemplatesPage } from "@/features/contracts/pages/contract-templates-page"

export const metadata: Metadata = { title: "合同模板与申请" }

export default function ContractTemplatesRoute() {
    return <ContractTemplatesPage />
}
