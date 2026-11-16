import type { Metadata } from "next"

import { ListRouteRedirect } from "@/features/master-data/pages/list-route-redirect"

export const metadata: Metadata = {
    title: "批量任务记录",
}

/** 导出任务已并入批量任务记录：旧地址保留跳转，避免书签与历史链接失效。 */
export default function Page() {
    return (
        <ListRouteRedirect
            href="/governance/background-jobs"
            label="批量任务记录"
        />
    )
}
