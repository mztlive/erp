/**
 * 本地时间格式化统一出口。
 *
 * mode 覆盖项目内全部历史变体（见各页面原 formatTime/formatDateTime 副本）：
 * - "full"          → zh-CN toLocaleString，hour12:false，Y/M/D HH:mm（最常用）
 * - "default"       → zh-CN toLocaleString，hour12:false，全字段默认（含秒）
 * - "monthDay"      → zh-CN toLocaleString，hour12:false，MM/DD HH:mm
 * - "monthDayIntl"  → zh-CN Intl.DateTimeFormat，MM/DD HH:mm
 * - "fullIntl"      → zh-CN Intl.DateTimeFormat，Y/M/D HH:mm
 * - "dateStyle"     → zh-CN Intl.DateTimeFormat，dateStyle:"medium" + timeStyle:"short"
 *
 * empty 控制空值语义（与原实现逐处核对）：
 * - "dash"         → 空值返回 "—"（原实现 if (!iso) return "—"）
 * - "passthrough"  → 空值照常走 new Date(...)（null → epoch；""/undefined → catch 后回原值）
 */

export type DateTimeFormatMode =
    | "full"
    | "default"
    | "monthDay"
    | "monthDayIntl"
    | "fullIntl"
    | "dateStyle"

export type DateTimeEmptyHandling = "dash" | "passthrough"

export function formatDateTime(
    iso: string | null | undefined,
    mode: DateTimeFormatMode = "full",
    empty: DateTimeEmptyHandling = "dash",
): string {
    if (empty === "dash" && !iso) return "—"
    try {
        switch (mode) {
            case "default":
                return new Date(iso as string).toLocaleString("zh-CN", {
                    hour12: false,
                })
            case "monthDay":
                return new Date(iso as string).toLocaleString("zh-CN", {
                    hour12: false,
                    month: "2-digit",
                    day: "2-digit",
                    hour: "2-digit",
                    minute: "2-digit",
                })
            case "monthDayIntl":
                return new Intl.DateTimeFormat("zh-CN", {
                    month: "2-digit",
                    day: "2-digit",
                    hour: "2-digit",
                    minute: "2-digit",
                }).format(new Date(iso as string))
            case "fullIntl":
                return new Intl.DateTimeFormat("zh-CN", {
                    year: "numeric",
                    month: "2-digit",
                    day: "2-digit",
                    hour: "2-digit",
                    minute: "2-digit",
                }).format(new Date(iso as string))
            case "dateStyle":
                return new Intl.DateTimeFormat("zh-CN", {
                    dateStyle: "medium",
                    timeStyle: "short",
                }).format(new Date(iso as string))
            default:
                return new Date(iso as string).toLocaleString("zh-CN", {
                    hour12: false,
                    year: "numeric",
                    month: "2-digit",
                    day: "2-digit",
                    hour: "2-digit",
                    minute: "2-digit",
                })
        }
    } catch {
        return (iso as string) ?? ""
    }
}
