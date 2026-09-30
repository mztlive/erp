"use client"
import Link from "next/link"
/** 角色只承载操作权限，旧角色范围入口不再提供写入。 */
export function SubjectScopesPanel({
    roleName,
}: {
    roleId: string
    roleName: string
}) {
    return (
        <div className="space-y-3 text-sm">
            <p>{roleName}只决定操作权限。数据范围请按人员设置。</p>
            <Link
                id="role-scope-person-entry"
                href="/system/accounts"
                className="text-primary"
            >
                进入人员账号
            </Link>
        </div>
    )
}
