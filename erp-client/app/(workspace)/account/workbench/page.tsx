import { Suspense } from "react"
import { RoleWorkbenchEntry } from "@/features/workspace/pages/role-workbench-entry"
export const metadata = { title: "个人首页设置" }
export default function Page() {
    return (
        <Suspense fallback={<p>正在读取设置…</p>}>
            <RoleWorkbenchEntry settings />
        </Suspense>
    )
}
