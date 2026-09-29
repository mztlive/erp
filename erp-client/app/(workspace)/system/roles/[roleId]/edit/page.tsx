import type { Metadata } from "next"

import { RoleFormPage } from "@/features/admin/pages/role-form-page"

export const metadata: Metadata = {
    title: "编辑角色",
}

/**
 * 系统 · 编辑角色（整页表单）。
 * roleId 为角色稳定 ID；业务数据由客户端 TanStack Query 加载。
 */
export default async function SystemRoleEditRoutePage({
    params,
    searchParams,
}: {
    params: Promise<{ roleId: string }>
    searchParams: Promise<{ returnTo?: string }>
}) {
    const { roleId } = await params
    const { returnTo } = await searchParams
    return <RoleFormPage roleId={roleId} returnTo={returnTo} />
}
