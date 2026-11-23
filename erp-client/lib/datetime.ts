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

/** 本地时区今天（YYYY-MM-DD）。 */
export function todayDate(): string {
    const date = new Date()
    const pad = (value: number) => String(value).padStart(2, "0")
    return `${date.getFullYear()}-${pad(date.getMonth() + 1)}-${pad(date.getDate())}`
}

/** 本地时间 HH:mm（zh-CN，24 小时制）；无法解析时原样返回。 */
export function formatClock(iso: string): string {
    try {
        return new Intl.DateTimeFormat("zh-CN", {
            hour: "2-digit",
            minute: "2-digit",
            hour12: false,
        }).format(new Date(iso))
    } catch {
        return iso
    }
}

/** 本地时间 HH:mm:ss（zh-CN，24 小时制）；无法解析时原样返回。 */
export function formatTime(iso: string | null | undefined): string {
    if (!iso) return "—"
    const parsed = new Date(iso)
    if (Number.isNaN(parsed.getTime())) return iso
    return new Intl.DateTimeFormat("zh-CN", {
        hour: "2-digit",
        minute: "2-digit",
        second: "2-digit",
        hour12: false,
    }).format(parsed)
}

/** 本地日期 YYYY/MM/DD（zh-CN）；无效值返回「—」。 */
export function formatDate(
    iso: string | null | undefined,
    options?: { timeZone?: string },
): string {
    if (!iso) return "—"
    const parsed = new Date(iso)
    if (Number.isNaN(parsed.getTime())) return "—"
    return new Intl.DateTimeFormat("zh-CN", {
        year: "numeric",
        month: "2-digit",
        day: "2-digit",
        ...(options?.timeZone ? { timeZone: options.timeZone } : {}),
    }).format(parsed)
}

/** ISO 日期 YYYY-MM-DD（支持指定时区）；无效值返回 null。 */
export function formatIsoDate(
    iso: string | null | undefined,
    options?: { timeZone?: string },
): string | null {
    if (!iso) return null
    const parsed = new Date(iso)
    if (Number.isNaN(parsed.getTime())) return null
    return new Intl.DateTimeFormat("sv-SE", {
        year: "numeric",
        month: "2-digit",
        day: "2-digit",
        ...(options?.timeZone ? { timeZone: options.timeZone } : {}),
    }).format(parsed)
}

/** 指定时区日期时间 YYYY/MM/DD HH:mm（zh-CN，24 小时制）；无效值返回「—」。 */
export function formatDateTimeInZone(
    iso: string | null | undefined,
    timeZone = "Asia/Shanghai",
): string {
    if (!iso) return "—"
    const parsed = new Date(iso)
    if (Number.isNaN(parsed.getTime())) return "—"
    return new Intl.DateTimeFormat("zh-CN", {
        timeZone,
        year: "numeric",
        month: "2-digit",
        day: "2-digit",
        hour: "2-digit",
        minute: "2-digit",
        hour12: false,
    }).format(parsed)
}

/** Unix 秒转 ISO；空值、非有限值或非正数返回空串。 */
export function unixSecondsToIsoOrEmpty(
    secs: number | null | undefined,
): string {
    if (secs == null || !Number.isFinite(Number(secs)) || Number(secs) <= 0)
        return ""
    return new Date(Number(secs) * 1000).toISOString()
}

/** Unix 秒转 ISO；空值或非有限值回退当前时间。 */
export function unixSecondsToIsoOrNow(
    seconds: number | null | undefined,
): string {
    return seconds == null || !Number.isFinite(seconds)
        ? new Date().toISOString()
        : new Date(seconds * 1000).toISOString()
}

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
