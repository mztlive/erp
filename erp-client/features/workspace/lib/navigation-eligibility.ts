/**
 * W01 只把服务端动作当作打开处理器的资格，不在工作台提交责任命令。
 * `START_PROCESSING` 表示目标处理器可继续建立 POOL 任务个人责任。
 */
export function canOpenWorkItemHandler(
    allowedActions: readonly string[],
    hasProcessBlocker: boolean,
): boolean {
    return (
        !hasProcessBlocker &&
        (allowedActions.includes("PROCESS") ||
            allowedActions.includes("START_PROCESSING"))
    )
}
