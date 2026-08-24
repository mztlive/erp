import type { Metadata } from "next"

import { ProcurementResponsibilityRulesPage } from "@/features/procurement-responsibilities/components/procurement-responsibility-rules-page"

export const metadata: Metadata = {
    title: "采购责任规则",
}

export default function Page() {
    return <ProcurementResponsibilityRulesPage />
}
