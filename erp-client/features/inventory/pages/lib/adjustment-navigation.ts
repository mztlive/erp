/** 普通单据预览不得继承旧 WorkItem 深链身份。 */
export const openAdjustmentPreviewPatch = (adjustmentId: string) => ({
    view: "adjustment" as const,
    adjustmentId,
    currentWorkItemId: null,
    workItemId: null,
    balanceId: null,
})

/** 关闭库存调整预览时必须同时清除普通与 WorkItem 深链身份。 */
export const closeAdjustmentPreviewPatch = () => ({
    adjustmentId: null,
    currentWorkItemId: null,
    workItemId: null,
})
