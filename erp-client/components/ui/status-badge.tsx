import {
    BanIcon,
    CircleAlertIcon,
    CircleCheckIcon,
    CircleDashedIcon,
    CircleDotIcon,
    type LucideIcon,
    TriangleAlertIcon,
} from "lucide-react"

import { Badge } from "@/components/ui/badge"
import { cn } from "@/lib/utils"

/**
 * 单据状态徽章。
 *
 * 交互规范 §4.5 / §19：状态必须同时使用文字、图标和颜色，禁止只用红黄绿圆点。
 * 因此 `label` 为必填，且始终渲染图标 —— 从类型上杜绝「仅靠颜色表意」的用法。
 */
type StatusTone =
    | "neutral" // 草稿、未开始、不适用
    | "info" // 处理中、待接收、待复核
    | "success" // 已生效、已完成、已结清
    | "warning" // 待审批、部分履约、版本差异
    | "destructive" // 接收失败、待处理异常
    | "void" // 已作废、已关闭

const toneIcon: Record<StatusTone, LucideIcon> = {
    neutral: CircleDashedIcon,
    info: CircleDotIcon,
    success: CircleCheckIcon,
    warning: CircleAlertIcon,
    destructive: TriangleAlertIcon,
    void: BanIcon,
}

const toneVariant: Record<
    StatusTone,
    "neutral" | "info" | "success" | "warning" | "destructive"
> = {
    neutral: "neutral",
    info: "info",
    success: "success",
    warning: "warning",
    destructive: "destructive",
    void: "neutral",
}

/** 时间轴标记等状态底色；与徽章使用的语义色一致。 */
const statusToneMarkerClass: Record<StatusTone, string> = {
    neutral:
        "border-neutral-border bg-neutral-soft text-neutral-soft-foreground",
    info: "border-info-border bg-info-soft text-info-soft-foreground",
    success:
        "border-success-border bg-success-soft text-success-soft-foreground",
    warning:
        "border-warning-border bg-warning-soft text-warning-soft-foreground",
    destructive:
        "border-destructive-border bg-destructive-soft text-destructive-soft-foreground",
    void: "border-neutral-border bg-neutral-soft text-neutral-soft-foreground",
}

const quietIconClass: Record<StatusTone, string> = {
    neutral: "text-muted-foreground/80",
    info: "text-info",
    success: "text-success",
    warning: "text-warning",
    destructive: "text-destructive",
    void: "text-muted-foreground/70",
}

function StatusBadge({
    tone = "neutral",
    icon,
    label,
    appearance = "badge",
    className,
    ...props
}: Omit<React.ComponentProps<typeof Badge>, "variant" | "children"> & {
    tone?: StatusTone
    /** 状态文字。必填：状态不得仅靠颜色表达。 */
    label: string
    /** 覆盖默认图标，例如用 ClockIcon 表示「等待中」。 */
    icon?: LucideIcon
    /**
     * - badge（默认）：浅底语义胶囊，适合异常或需强提示的状态；
     * - text：无底色语义文字与图标；
     * - quiet：无底色语义图标 + 沉稳正文文字，适合列表中的常态健康状态降噪。
     */
    appearance?: "badge" | "text" | "quiet"
}) {
    const Icon = icon ?? toneIcon[tone]
    const isQuiet = appearance === "quiet"

    return (
        <Badge
            variant={toneVariant[tone]}
            className={cn(
                "gap-1 rounded-sm border-transparent px-2 py-0 text-xs font-medium tracking-normal",
                tone === "void" && "line-through opacity-80",
                appearance === "text" &&
                    "border-transparent bg-transparent px-0 shadow-none",
                appearance === "text" &&
                    tone === "neutral" &&
                    "text-muted-foreground",
                isQuiet &&
                    "gap-1.5 border-transparent bg-transparent px-0 font-normal text-foreground/80 shadow-none",
                isQuiet &&
                    (tone === "neutral" || tone === "void") &&
                    "text-muted-foreground",
                className,
            )}
            {...props}
        >
            <Icon
                data-icon="inline-start"
                aria-hidden="true"
                className={cn(
                    "size-3 shrink-0",
                    isQuiet
                        ? cn("size-3.5 opacity-90", quietIconClass[tone])
                        : "opacity-80",
                )}
            />
            {label}
        </Badge>
    )
}

export { StatusBadge, statusToneMarkerClass, type StatusTone }
