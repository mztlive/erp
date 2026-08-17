/**
 * 「确认通过」打开方案对话框，不代表服务端此刻已接受通过结论。
 * 分行未齐时后端不会给 APPROVE，但仍可凭 SAVE 打开对话框补齐后再提交。
 */
export function canOpenProcurementConfirmPlan(
    allowedActions: readonly string[] | undefined,
): boolean {
    return Boolean(
        allowedActions?.includes("SAVE") || allowedActions?.includes("APPROVE"),
    )
}
