import type { Metadata } from "next"

import { RoleFormPage } from "@/features/admin/role-form-page"

export const metadata: Metadata = {
    title: "新建角色",
}

/** 系统 · 新建角色（整页表单；权限目录较大，不使用弹窗）。 */
export default function SystemRoleCreateRoutePage() {
    return <RoleFormPage roleId={null} />
}
