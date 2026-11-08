/** 编译期证明类型上不存在某个键。各 NO_APPROVAL 单据用它卡住审批字段。 */
export type ForbidKey<T, K extends string> = K extends keyof T ? never : true

/**
 * 丢掉载荷上误带的 `approval`。字段不存在时原样返回。
 *
 * @param dto HTTP 载荷或投影。
 * @returns 不含 `approval` 的对象。
 */
export function stripApprovalField<T extends object>(
    dto: T,
): Omit<T, "approval"> {
    if (!("approval" in dto)) {
        return dto
    }
    const { approval: _discarded, ...rest } = dto as T & {
        approval?: unknown
    }
    void _discarded
    return rest
}
