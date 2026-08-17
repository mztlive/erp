import type { Metadata } from "next"

import { ActualProfitLossPage } from "@/features/actual-profit-loss/pages/actual-profit-loss-page"

export const metadata: Metadata = {
    title: "实际经营盈亏（非卡券 · 不含税）",
}

export default function Page() {
    return <ActualProfitLossPage />
}
