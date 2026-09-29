"use client"
import { useQuery } from "@tanstack/react-query"
import { checkAccess, type AccessCheckInput } from "../api/access-check"
/** 只读检查使用查询缓存，配置修改后由统一权限失效机制撤下旧结果。 */
export function useAccessCheck(input: AccessCheckInput | null) {
    return useQuery({
        queryKey: ["admin", "access-check", input],
        queryFn: () => checkAccess(input!),
        enabled: Boolean(input),
        staleTime: 0,
        retry: false,
    })
}
