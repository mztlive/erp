export type ChangeOrderBlockerSection = readonly [key: string, label: string]

/** 按字段错误归并出尚未填好的改单区块，供提交按钮旁的分区提示使用。 */
export function blockedChangeOrderSections(
    fieldMeta: Readonly<
        Record<string, { errors?: readonly unknown[] } | undefined>
    >,
    classify: (fieldName: string) => string,
): string {
    const sections = new Set<string>()
    for (const [name, meta] of Object.entries(fieldMeta)) {
        if (meta?.errors?.length) sections.add(classify(name))
    }
    return Array.from(sections).join(",")
}

/** 提交按钮不可用时说明哪几块还没填好，点击跳到该块。 */
export function ChangeOrderBlockers({
    id,
    blocked,
    sections,
}: {
    id: string
    blocked: string
    sections: readonly ChangeOrderBlockerSection[]
}) {
    if (!blocked) return null
    const active = new Set(blocked.split(","))
    return (
        <p
            role="alert"
            className="flex flex-wrap items-center text-xs text-destructive"
        >
            待处理：
            {sections
                .filter(([key]) => active.has(key))
                .map(([key, label], index) => (
                    <span key={key} className="inline-flex items-center">
                        {index ? "、" : null}
                        <button
                            id={`${id}-blocker-${key}`}
                            type="button"
                            className="cursor-pointer rounded-xs underline underline-offset-2 outline-none hover:text-destructive/80 focus-visible:ring-3 focus-visible:ring-ring"
                            onClick={() =>
                                document
                                    .getElementById(`${id}-${key}-title`)
                                    ?.scrollIntoView({
                                        behavior: "smooth",
                                        block: "start",
                                    })
                            }
                        >
                            {label}
                        </button>
                    </span>
                ))}
        </p>
    )
}
