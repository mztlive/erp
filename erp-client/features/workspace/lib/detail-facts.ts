/**
 * 工作台详情的键值段分层。
 *
 * 服务端简报把 10 余个键值段平铺下发（见 `work_item/brief.rs`），全部等权上屏会让
 * 审批人找不到落点。这里按「金额条 / 常显字段 / 其余字段」三层拆开：金额条是决策
 * 重心，常显字段影响审批判断，其余是查证用的背书信息。作业面全部展开，只用来排序。
 */

export type DetailSection = Readonly<{
    label: string
    value: string
    numeric?: boolean
}>

/** 抽到金额条的段，并按此顺序上屏；第一项作为主金额放大。 */
const AMOUNT_ORDER: readonly string[] = [
    "含税金额",
    "不含税金额",
    "税额",
    "未分配",
]

/** 与标题行往来方同义的段，值一致时不重复上屏。 */
const COUNTERPARTY_LABELS = new Set(["客户", "供应商", "往来方"])

/** 影响审批判断、排在单据信息前面的段。 */
const KEY_LABELS = new Set([
    "业务性质",
    "付款条件",
    "提交来源",
    "到账日",
    "银行流水",
])

/** 32 位十六进制或标准 UUID。 */
const OPAQUE_ID =
    /^(?:[0-9a-f]{24,}|[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12})$/i

export type WorkspaceDetailFacts = Readonly<{
    /** 金额条，主金额在首位。 */
    amounts: readonly DetailSection[]
    /** 影响审批判断的字段，排在单据信息前面。 */
    keyFields: readonly DetailSection[]
    /** 其余单据字段，作业面与常显字段一并展开。 */
    moreFields: readonly DetailSection[]
    /** 已解析成人名的提交人，用于标题副行。 */
    submitter?: string
}>

/**
 * 值是否只是一串不可读的对象 id。
 *
 * 服务端 `submitter_name` 偶尔回落成用户 id，这种值对审批人没有信息量，
 * 不应占据决策视线。
 */
export function isOpaqueId(value: string): boolean {
    return OPAQUE_ID.test(value.trim())
}

/**
 * 把简报键值段拆成详情的三层。
 *
 * # 参数
 * * `sections` - 服务端或单据事实给出的键值段
 * * `counterparty` - 标题行已展示的往来方名称
 *
 * # 返回
 * 金额条、常显字段、折叠字段与提交人。空值段一律丢弃。
 */
export function splitDetailSections(
    sections: readonly DetailSection[] | undefined,
    counterparty?: string,
): WorkspaceDetailFacts {
    const amounts: DetailSection[] = []
    const keyFields: DetailSection[] = []
    const moreFields: DetailSection[] = []
    let submitter: string | undefined
    const shown = counterparty?.trim()

    for (const section of sections ?? []) {
        const value = section.value.trim()
        if (!value) continue
        const entry: DetailSection = { ...section, value }

        if (COUNTERPARTY_LABELS.has(section.label) && value === shown) continue

        if (section.label === "提交人") {
            // 未解析成人名时服务端回落成用户 id。内部 ID 不上屏（见 AGENTS.md §5），
            // 且对审批人没有信息量，整段丢弃。
            if (!isOpaqueId(value)) submitter = value
            continue
        }
        if (AMOUNT_ORDER.includes(section.label)) {
            amounts.push(entry)
            continue
        }
        if (KEY_LABELS.has(section.label)) {
            keyFields.push(entry)
            continue
        }
        moreFields.push(entry)
    }

    amounts.sort(
        (a, b) => AMOUNT_ORDER.indexOf(a.label) - AMOUNT_ORDER.indexOf(b.label),
    )
    return { amounts, keyFields, moreFields, submitter }
}
