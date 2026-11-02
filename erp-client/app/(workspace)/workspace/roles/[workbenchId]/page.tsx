import { Suspense } from "react"
import { RoleWorkbenchPage } from "@/features/workspace/pages/role-workbench-page"
export const metadata = { title: "岗位工作台" }
export default async function Page({
    params,
}: {
    params: Promise<{ workbenchId: string }>
}) {
    const { workbenchId } = await params
    return (
        <Suspense fallback={<p>正在加载工作台…</p>}>
            <RoleWorkbenchPage id={workbenchId} />
        </Suspense>
    )
}
