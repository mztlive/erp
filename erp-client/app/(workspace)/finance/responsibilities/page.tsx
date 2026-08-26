import type { Metadata } from "next"

import { FinanceResponsibilityRulesPage } from "@/features/finance-responsibilities/components/finance-responsibility-rules-page"

export const metadata: Metadata = {
    title: "财务责任配置",
}

export default function Page() {
    return <FinanceResponsibilityRulesPage />
}
