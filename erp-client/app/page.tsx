import { redirect } from "next/navigation"

/**
 * 根路由：统一落地到今日工作台。
 *
 * 演示阶段无登录（mock 数据驱动，界面已有「演示环境」标识）；接真实后端后，
 * 这里应先按登录态分流（未登录 → /login，已登录 → /workspace）。
 *
 * `demoRole` 等演示参数原样带过，避免根入口丢失角色上下文。
 */
export default async function HomePage({
  searchParams,
}: {
  searchParams: Promise<Record<string, string | string[] | undefined>>
}) {
  const params = await searchParams
  const query = new URLSearchParams()
  for (const [key, value] of Object.entries(params)) {
    if (typeof value === "string") query.set(key, value)
  }
  const search = query.toString()
  redirect(`/workspace${search ? `?${search}` : ""}`)
}
