import { permanentRedirect } from "next/navigation"

import { pickLegalWorkspaceQuery } from "@/features/workspace/lib/url-state"

/**
 * 旧统一待办路由永久重定向到唯一工作台，不保留第二套页面代码。
 */
export default async function WorkspaceTasksRedirectPage({
    searchParams,
}: {
    searchParams: Promise<Record<string, string | string[] | undefined>>
}) {
    const params = await searchParams
    permanentRedirect(pickLegalWorkspaceQuery(params))
}
