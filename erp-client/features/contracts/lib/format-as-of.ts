/** 统一「数据截至」时间格式：列表预览与详情页共用。 */
export function formatAsOf(iso: string): string {
    try {
        return new Intl.DateTimeFormat("zh-CN", {
            month: "long",
            day: "numeric",
            hour: "2-digit",
            minute: "2-digit",
            timeZone: "Asia/Shanghai",
        }).format(new Date(iso))
    } catch {
        return iso
    }
}
