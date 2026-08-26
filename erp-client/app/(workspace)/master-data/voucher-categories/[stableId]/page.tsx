import type { Metadata } from "next"

import { ListRouteRedirect } from "@/features/master-data/pages/list-route-redirect"

export const metadata: Metadata = {
    title: "卡券类目",
}

export default function Page() {
    return (
        <ListRouteRedirect
            href="/master-data/voucher-categories"
            label="卡券类目列表"
        />
    )
}
